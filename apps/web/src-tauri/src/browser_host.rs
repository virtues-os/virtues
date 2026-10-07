//! The owner's browser, driven by the assistant on their box.
//!
//! A window of the app's own ("Browser") holds a WKWebView with a persistent
//! cookie jar apart from the app's UI and from Safari. While the app runs it
//! keeps a websocket open to the box's `/ws/browser`; each message is one
//! `browser_*` tool call from the assistant, performed here and answered.
//!
//! How WebKit is driven without a DevTools protocol, all measured in
//! agents/record/webkit-agent-spike.md:
//! - Reading: Playwright's injected script (`vendor/playwright`, Apache-2.0)
//!   runs in an isolated content world, so the page can neither see nor tamper
//!   with it, and returns the refs-per-element outline models already know.
//! - Acting: AppKit events sent straight to the webview's responder methods.
//!   They arrive `isTrusted`, and they work with the window behind other apps
//!   without taking the owner's focus. Synthetic DOM events cannot even type: a
//!   synthetic mousedown never moves focus.
//! - Hidden pages: a covered window stops `requestAnimationFrame`, which hung
//!   the spike. The inactive scheduling policy is set to `none`, and nothing
//!   here waits on a frame.

use std::time::Duration;

use base64::Engine;
use block2::RcBlock;
use futures_util::{SinkExt, StreamExt};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, MainThreadMarker};
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSEvent, NSEventModifierFlags, NSEventType, NSImage,
};
use objc2_foundation::{NSDictionary, NSError, NSNumber, NSPoint, NSProcessInfo, NSString};
use objc2_web_kit::{WKContentWorld, WKInactiveSchedulingPolicy, WKSnapshotConfiguration, WKWebView};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

const LABEL: &str = "browser";
const INJECTED: &str = include_str!("../vendor/playwright/injected.js");
/// Safari's own user agent; a bare WKWebView omits `Version/… Safari/…`.
const SAFARI_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 \
                         (KHTML, like Gecko) Version/26.0 Safari/605.1.15";
/// An outline longer than this is cut, with a note saying how to read the
/// rest. About 10k tokens; Hacker News's front page alone is 44k characters.
const MAX_OUTLINE_CHARS: usize = 40_000;

// ─── The connection ─────────────────────────────────────────────────────────

/// Keep a connection to the box's `/ws/browser` for as long as the app runs.
/// `paired` is asked before each attempt: an unpaired app has no box to serve.
pub fn start(app: AppHandle, paired: fn() -> bool) {
    tauri::async_runtime::spawn(async move {
        let mut backoff = Duration::from_secs(2);
        loop {
            if let Some(url) = endpoint(paired) {
                match tokio_tungstenite::connect_async(url.as_str()).await {
                    Ok((socket, _)) => {
                        backoff = Duration::from_secs(2);
                        serve(&app, socket).await;
                    }
                    Err(e) => eprintln!("[browser] could not reach {url}: {e}"),
                }
            }
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_secs(30));
        }
    });
}

fn endpoint(paired: fn() -> bool) -> Option<String> {
    // A dev override, for driving the browser from a scratch core without
    // pairing. Debug builds only: a release app talks to its own box.
    #[cfg(debug_assertions)]
    if let Ok(url) = std::env::var("VIRTUES_BROWSER_WS") {
        return Some(url);
    }
    paired().then(|| format!("ws://127.0.0.1:{}/ws/browser", tauri_plugin_reach::loopback_port()))
}

async fn serve<S>(app: &AppHandle, socket: tokio_tungstenite::WebSocketStream<S>)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let writer = tauri::async_runtime::spawn(async move {
        while let Some(m) = rx.recv().await {
            if sink.send(m).await.is_err() {
                break;
            }
        }
    });
    // The loopback splice and the relay both drop a connection that says
    // nothing for long enough.
    let pinger = {
        let tx = tx.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(25)).await;
                if tx.send(Message::Ping(Vec::new())).is_err() {
                    break;
                }
            }
        })
    };

    // One worker, in arrival order. A model that calls browser_open and
    // browser_snapshot in the same step means "open, then read", and reading
    // a page mid-navigation fails.
    let (jobs, mut queue) = mpsc::unbounded_channel::<Value>();
    let worker = {
        let (app, tx) = (app.clone(), tx.clone());
        tauri::async_runtime::spawn(async move {
            while let Some(req) = queue.recv().await {
                let id = req.get("id").cloned().unwrap_or(Value::Null);
                let op = req.get("op").and_then(|o| o.as_str()).unwrap_or("").to_string();
                let args = req.get("args").cloned().unwrap_or(Value::Null);
                let reply = match perform(&app, &op, &args).await {
                    Ok(result) => json!({ "id": id, "ok": true, "result": result }),
                    Err(error) => json!({ "id": id, "ok": false, "error": error }),
                };
                let _ = tx.send(Message::Text(reply.to_string()));
            }
        })
    };

    while let Some(Ok(msg)) = stream.next().await {
        let Message::Text(text) = msg else { continue };
        if let Ok(req) = serde_json::from_str::<Value>(&text) {
            let _ = jobs.send(req);
        }
    }
    worker.abort();
    pinger.abort();
    writer.abort();
}

async fn perform(app: &AppHandle, op: &str, args: &Value) -> Result<Value, String> {
    let arg = |k: &str| args.get(k).and_then(|v| v.as_str()).map(str::to_string);
    match op {
        "open" => open(app, &arg("url").ok_or("`url` is required")?).await,
        "snapshot" => {
            let depth = args.get("depth").and_then(|v| v.as_u64());
            snapshot(&window(app)?, arg("ref"), depth).await
        }
        "click" => click(&window(app)?, &arg("ref").ok_or("`ref` is required")?).await,
        "type" => {
            let win = window(app)?;
            if let Some(r) = arg("ref") {
                click(&win, &r).await?;
            }
            let text = arg("text").ok_or("`text` is required")?;
            on_main(&win, move |wk, _| unsafe {
                focus_view(wk);
                let _: () = msg_send![wk, insertText: &*NSString::from_str(&text)];
            })
            .await?;
            if args.get("submit").and_then(|v| v.as_bool()) == Some(true) {
                tokio::time::sleep(Duration::from_millis(100)).await;
                press(&win, "Enter").await?;
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
            Ok(json!({ "typed": true, "url": current_url(&win) }))
        }
        "press" => {
            let win = window(app)?;
            press(&win, &arg("key").ok_or("`key` is required")?).await?;
            tokio::time::sleep(Duration::from_millis(500)).await;
            Ok(json!({ "pressed": true, "url": current_url(&win) }))
        }
        "scroll" => {
            let up = arg("direction").as_deref() == Some("up");
            let px = args.get("pixels").and_then(|v| v.as_f64()).unwrap_or(800.0).clamp(100.0, 4000.0);
            scroll(&window(app)?, if up { -px } else { px }).await
        }
        "screenshot" => screenshot(&window(app)?).await,
        // Development only: run a script body in the page and return its
        // string. Never reachable from a release build, so never from a model.
        #[cfg(debug_assertions)]
        "eval" => {
            let js = arg("js").ok_or("`js` is required")?;
            let agent = arg("world").as_deref() == Some("agent");
            Ok(json!({ "value": eval(&window(app)?, &js, agent).await? }))
        }
        other => Err(format!("unknown browser operation `{other}`")),
    }
}

// ─── The window ─────────────────────────────────────────────────────────────

fn window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window(LABEL)
        .ok_or_else(|| "The browser window is not open. Call browser_open with a URL first.".into())
}

fn current_url(win: &WebviewWindow) -> String {
    win.url().map(|u| u.to_string()).unwrap_or_default()
}

/// A cookie jar of the browser's own, apart from the app's UI. A dev profile
/// gets a different one, as its main window does.
fn store_id() -> [u8; 16] {
    let mut id = *b"virtues-browser!";
    if let Some(p) = tauri_plugin_reach::profile() {
        for (i, b) in p.bytes().enumerate() {
            id[i % 16] ^= b;
        }
    }
    id
}

async fn open(app: &AppHandle, url: &str) -> Result<Value, String> {
    let target: tauri::Url = url.parse().map_err(|_| format!("`{url}` is not a URL"))?;
    if !matches!(target.scheme(), "https" | "http") {
        return Err("Only http and https pages can be opened.".into());
    }
    let win = match app.get_webview_window(LABEL) {
        Some(win) => {
            // Mark the current document so the new one can be told apart.
            let _ = eval(&win, "window.__virtuesNav = 1; return '1'", false).await;
            win.navigate(target.clone()).map_err(|e| e.to_string())?;
            win
        }
        None => {
            let win = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::External(target.clone()))
                .title("Browser")
                .inner_size(1200.0, 860.0)
                // Shown, but not focused: the assistant opening a page must not
                // pull the owner out of whatever they are doing.
                .focused(false)
                .user_agent(SAFARI_UA)
                .data_store_identifier(store_id())
                .on_new_window(|_, _| tauri::webview::NewWindowResponse::Allow)
                .build()
                .map_err(|e| format!("could not open the browser window: {e}"))?;
            on_main(&win, |wk, _| unsafe {
                // macOS 14+. The app supports 13.3, where the selector does not
                // exist and calling it would crash; there a covered page is
                // throttled, and nothing here waits on a frame, so it is slower
                // rather than stuck.
                let prefs = wk.configuration().preferences();
                let sel = objc2::sel!(setInactiveSchedulingPolicy:);
                let can: bool = msg_send![&*prefs, respondsToSelector: sel];
                if can {
                    prefs.setInactiveSchedulingPolicy(WKInactiveSchedulingPolicy::None);
                }
            })
            .await?;
            win
        }
    };

    // Wait for the new document to finish loading. A same-document navigation
    // never replaces the marker, so give up waiting on it after a while and
    // report where the page is.
    let started = std::time::Instant::now();
    loop {
        tokio::time::sleep(Duration::from_millis(300)).await;
        let state = eval(
            &win,
            "return JSON.stringify({ fresh: !window.__virtuesNav, ready: document.readyState, \
             url: location.href, title: document.title })",
            false,
        )
        .await
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok());
        let done = state.as_ref().is_some_and(|s| {
            s["ready"] == "complete" && s["url"] != "about:blank" && s["fresh"] == true
        });
        if done || started.elapsed() > Duration::from_secs(40) {
            let s = state.unwrap_or(Value::Null);
            return Ok(json!({ "url": s["url"], "title": s["title"], "loaded": done }));
        }
    }
}

// ─── Reading ────────────────────────────────────────────────────────────────

async fn ensure_agent(win: &WebviewWindow) -> Result<(), String> {
    if eval(win, "return window.__virtuesAgent ? '1' : ''", true).await? == "1" {
        return Ok(());
    }
    let bootstrap = format!(
        "const module = {{}};\n{INJECTED}\nwindow.__virtuesAgent = new (module.exports.InjectedScript())(window, \
         {{ isUnderTest: false, sdkLanguage: 'javascript', frameSeq: 0, testIdAttributeName: 'data-testid', \
         stableRafCount: 1, browserName: 'webkit', shouldPrependErrorPrefix: true, isUtilityWorld: true, \
         customEngines: [] }});\nreturn '1';"
    );
    eval(win, &bootstrap, true).await.map(|_| ())
}

async fn snapshot(win: &WebviewWindow, scope: Option<String>, depth: Option<u64>) -> Result<Value, String> {
    ensure_agent(win).await?;
    // A scoped read starts from the element a ref names in the last outline.
    // Playwright keeps an element's ref across snapshots, so refs read here
    // still work, and refs outside the scope keep working too.
    let body = format!(
        "const scope = {scope};\n\
         const agent = window.__virtuesAgent;\n\
         const root = scope ? agent._lastAriaSnapshotForQuery?.info?.get(scope)?.element : document.body;\n\
         if (!root || !root.isConnected) return JSON.stringify({{ error: 'stale' }});\n\
         const options = {{ mode: 'ai' }};\n\
         if ({depth} > 0) options.depth = {depth};\n\
         return JSON.stringify({{ url: location.href, title: document.title, outline: agent.ariaSnapshot(root, options) }});",
        scope = serde_json::to_string(&scope).unwrap_or_else(|_| "null".into()),
        depth = depth.unwrap_or(0),
    );
    let raw = eval(win, &body, true).await?;
    if raw.contains("\"error\":\"stale\"") {
        return Err("That ref is not on the page any more. Take a fresh browser_snapshot.".into());
    }
    let mut v: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    if let Some(outline) = v.get("outline").and_then(|o| o.as_str()).map(str::to_string) {
        if outline.len() > MAX_OUTLINE_CHARS {
            let cut: String = outline.chars().take(MAX_OUTLINE_CHARS).collect();
            v["outline"] = json!(cut);
            v["truncated"] = json!(format!(
                "The outline was cut at {MAX_OUTLINE_CHARS} characters of {}. Snapshot with `depth` for an overview, then with `ref` to read one part.",
                outline.len()
            ));
        }
    }
    Ok(v)
}

// ─── Acting ─────────────────────────────────────────────────────────────────

async fn click(win: &WebviewWindow, r#ref: &str) -> Result<Value, String> {
    ensure_agent(win).await?;
    // Resolve the ref in the latest outline, bring it into view, and find the
    // point that really lands on it. A ref from an older outline on a page that
    // re-rendered can point at the wrong element, so refuse rather than guess.
    let body = format!(
        "const ref = {ref_json};\n\
         const info = window.__virtuesAgent._lastAriaSnapshotForQuery?.info?.get(ref);\n\
         const e = info?.element;\n\
         if (!e || !e.isConnected) return JSON.stringify({{ error: 'stale' }});\n\
         e.scrollIntoView({{ block: 'center', inline: 'center' }});\n\
         await new Promise(r => setTimeout(r, 80));\n\
         const rect = [...e.getClientRects()].find(r => r.width > 0 && r.height > 0) || e.getBoundingClientRect();\n\
         const x = rect.left + rect.width / 2, y = rect.top + rect.height / 2;\n\
         const hit = document.elementFromPoint(x, y);\n\
         const lands = !!hit && (hit === e || e.contains(hit) || hit.contains(e));\n\
         return JSON.stringify({{ x, y, lands, viewport: innerHeight, name: (e.getAttribute('aria-label') || e.innerText || '').trim().slice(0, 60) }});",
        ref_json = serde_json::to_string(r#ref).unwrap_or_default()
    );
    let at: Value = serde_json::from_str(&eval(win, &body, true).await?).map_err(|e| e.to_string())?;
    if at.get("error").is_some() {
        return Err(format!(
            "`{}` is not on the page any more. Take a fresh browser_snapshot and use a ref from it.",
            r#ref
        ));
    }
    if at["lands"] != true {
        return Err(format!(
            "Something covers `{}` (a dialog or banner, perhaps). Take a fresh browser_snapshot.",
            r#ref
        ));
    }
    let (x, y) = (at["x"].as_f64().unwrap_or(0.0), at["y"].as_f64().unwrap_or(0.0));
    let viewport = at["viewport"].as_f64().unwrap_or(0.0);
    on_main(win, move |wk, _| {
        focus_view(wk);
        mouse(wk, NSEventType::MouseMoved, x, y, viewport);
    })
    .await?;
    tokio::time::sleep(Duration::from_millis(30)).await;
    on_main(win, move |wk, _| mouse(wk, NSEventType::LeftMouseDown, x, y, viewport)).await?;
    tokio::time::sleep(Duration::from_millis(40)).await;
    on_main(win, move |wk, _| mouse(wk, NSEventType::LeftMouseUp, x, y, viewport)).await?;
    tokio::time::sleep(Duration::from_millis(350)).await;
    Ok(json!({ "clicked": at["name"], "url": current_url(win) }))
}

fn now() -> f64 {
    NSProcessInfo::processInfo().systemUptime()
}

/// Make the webview its window's first responder, so keys and inserted text
/// reach the page. Window-level only: it does not bring the app forward.
fn focus_view(wk: &WKWebView) {
    if let Some(window) = wk.window() {
        let responder: &objc2_app_kit::NSResponder = wk;
        window.makeFirstResponder(Some(responder));
    }
}

/// A mouse event at a CSS point of the viewport, delivered to the view itself
/// (not the window), which is what keeps it working with the window behind
/// other apps.
///
/// `viewport` is the page's `innerHeight`. Tauri's webview runs under the
/// title bar, so the page starts below the top of the view by an inset that no
/// public API reports; the view's height minus the page's is that inset. Without
/// it every click landed one title bar too high.
fn mouse(wk: &WKWebView, kind: NSEventType, x: f64, y: f64, viewport: f64) {
    let bounds = wk.bounds();
    let inset = (bounds.size.height - viewport).clamp(0.0, 200.0);
    let from_top = y + inset;
    let local = NSPoint::new(x, if wk.isFlipped() { from_top } else { bounds.size.height - from_top });
    let at = wk.convertPoint_toView(local, None);
    let number = wk.window().map(|w| w.windowNumber()).unwrap_or(0);
    let pressure = if kind == NSEventType::LeftMouseUp { 0.0 } else { 1.0 };
    let Some(event) = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
        kind, at, NSEventModifierFlags::empty(), now(), number, None, 0, 1, pressure,
    ) else {
        return;
    };
    match kind {
        NSEventType::LeftMouseUp => wk.mouseUp(&event),
        NSEventType::MouseMoved => wk.mouseMoved(&event),
        _ => wk.mouseDown(&event),
    }
}

/// (characters, macOS key code) for the keys the tool offers.
fn key(name: &str) -> Option<(&'static str, u16, bool)> {
    Some(match name {
        "Enter" => ("\r", 36, false),
        "Tab" => ("\t", 48, false),
        "Escape" => ("\u{1b}", 53, false),
        "Backspace" => ("\u{7f}", 51, false),
        "ArrowLeft" => ("\u{F702}", 123, true),
        "ArrowRight" => ("\u{F703}", 124, true),
        "ArrowDown" => ("\u{F701}", 125, true),
        "ArrowUp" => ("\u{F700}", 126, true),
        "PageUp" => ("\u{F72C}", 116, true),
        "PageDown" => ("\u{F72D}", 121, true),
        _ => return None,
    })
}

async fn press(win: &WebviewWindow, name: &str) -> Result<(), String> {
    let (chars, code, function) = key(name).ok_or_else(|| format!("unknown key `{name}`"))?;
    on_main(win, move |wk, _| {
        let flags = if function {
            NSEventModifierFlags::NumericPad | NSEventModifierFlags::Function
        } else {
            NSEventModifierFlags::empty()
        };
        focus_view(wk);
        let number = wk.window().map(|w| w.windowNumber()).unwrap_or(0);
        let s = NSString::from_str(chars);
        for kind in [NSEventType::KeyDown, NSEventType::KeyUp] {
            let Some(event) = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
                kind, NSPoint::new(0.0, 0.0), flags, now(), number, None, &s, &s, false, code,
            ) else {
                continue;
            };
            if kind == NSEventType::KeyDown {
                wk.keyDown(&event);
            } else {
                wk.keyUp(&event);
            }
        }
    })
    .await
}

/// Scroll whatever scrolls at the middle of the viewport: the page itself on
/// most sites, a pane in app-like ones.
async fn scroll(win: &WebviewWindow, dy: f64) -> Result<Value, String> {
    let body = format!(
        "const dy = {dy};\n\
         let el = document.elementFromPoint(innerWidth / 2, innerHeight / 2);\n\
         const scrolls = n => n && n.scrollHeight > n.clientHeight + 1 && /(auto|scroll)/.test(getComputedStyle(n).overflowY);\n\
         while (el && el !== document.body && !scrolls(el)) el = el.parentElement;\n\
         const target = scrolls(el) ? el : (document.scrollingElement || document.documentElement);\n\
         const before = target.scrollTop;\n\
         target.scrollBy({{ top: dy, behavior: 'instant' }});\n\
         await new Promise(r => setTimeout(r, 600));\n\
         return JSON.stringify({{ moved: Math.round(target.scrollTop - before), atEnd: target.scrollTop + target.clientHeight >= target.scrollHeight - 2 }});"
    );
    serde_json::from_str(&eval(win, &body, false).await?).map_err(|e| e.to_string())
}

async fn screenshot(win: &WebviewWindow) -> Result<Value, String> {
    let (tx, rx) = oneshot::channel::<Result<String, String>>();
    let tx = std::sync::Mutex::new(Some(tx));
    on_main(win, move |wk, mtm| unsafe {
        let config = WKSnapshotConfiguration::new(mtm);
        // Points; a Retina screen doubles it. Enough to read, small enough to
        // send with every look.
        config.setSnapshotWidth(Some(&NSNumber::new_f64(900.0)));
        let done = RcBlock::new(move |image: *mut NSImage, error: *mut NSError| {
            let out = if image.is_null() {
                Err(if error.is_null() {
                    "no image".to_string()
                } else {
                    (*error).localizedDescription().to_string()
                })
            } else {
                jpeg_base64(&*image).ok_or_else(|| "could not encode the image".to_string())
            };
            if let Some(tx) = tx.lock().ok().and_then(|mut t| t.take()) {
                let _ = tx.send(out);
            }
        });
        wk.takeSnapshotWithConfiguration_completionHandler(Some(&config), &done);
    })
    .await?;
    let jpeg = tokio::time::timeout(Duration::from_secs(15), rx)
        .await
        .map_err(|_| "the screenshot timed out".to_string())?
        .map_err(|_| "the screenshot was dropped".to_string())??;
    Ok(json!({ "url": current_url(win), "jpeg_base64": jpeg }))
}

fn jpeg_base64(image: &NSImage) -> Option<String> {
    let tiff = image.TIFFRepresentation()?;
    let rep = NSBitmapImageRep::imageRepWithData(&tiff)?;
    let quality = NSNumber::new_f64(0.6);
    let properties = NSDictionary::from_slices(
        &[unsafe { objc2_app_kit::NSImageCompressionFactor }],
        &[AsRef::<AnyObject>::as_ref(&*quality)],
    );
    let data = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::JPEG, &properties) }?;
    Some(base64::engine::general_purpose::STANDARD.encode(data.to_vec()))
}

// ─── Plumbing ───────────────────────────────────────────────────────────────

/// WebKit's own words for a script error. `localizedDescription` is only ever
/// "A JavaScript exception occurred"; the message and line are in `userInfo`.
fn js_error(error: &NSError) -> String {
    let generic = error.localizedDescription().to_string();
    unsafe {
        let info: *mut AnyObject = msg_send![error, userInfo];
        if info.is_null() {
            return generic;
        }
        let key = NSString::from_str("WKJavaScriptExceptionMessage");
        let message: *mut AnyObject = msg_send![info, objectForKey: &*key];
        if message.is_null() {
            return generic;
        }
        let text: Retained<NSString> = msg_send![message, description];
        let line_key = NSString::from_str("WKJavaScriptExceptionLineNumber");
        let line: *mut AnyObject = msg_send![info, objectForKey: &*line_key];
        let line = if line.is_null() {
            String::new()
        } else {
            let l: Retained<NSString> = msg_send![line, description];
            format!(" (line {l})")
        };
        format!("{generic}: {text}{line}")
    }
}

/// Run `f` against the window's WKWebView on the main thread.
async fn on_main<F>(win: &WebviewWindow, f: F) -> Result<(), String>
where
    F: FnOnce(&WKWebView, MainThreadMarker) + Send + 'static,
{
    let (tx, rx) = oneshot::channel::<()>();
    win.with_webview(move |pw| {
        let mtm = MainThreadMarker::new().expect("with_webview runs on the main thread");
        // SAFETY: on macOS `inner()` is the window's WKWebView, alive for the
        // duration of this callback.
        let wk: &WKWebView = unsafe { &*pw.inner().cast::<WKWebView>() };
        f(wk, mtm);
        let _ = tx.send(());
    })
    .map_err(|e| e.to_string())?;
    rx.await.map_err(|_| "the browser window went away".to_string())
}

/// The isolated world the injected script lives in, held for the life of the
/// app. WebKit discards a world, and every global in it, once nothing retains
/// it: looked up afresh per call, the snapshot script installed by one call was
/// gone by the next.
fn agent_world_on_main(mtm: MainThreadMarker) -> Retained<WKContentWorld> {
    thread_local! {
        static WORLD: std::cell::OnceCell<Retained<WKContentWorld>> = const { std::cell::OnceCell::new() };
    }
    WORLD.with(|w| {
        w.get_or_init(|| unsafe { WKContentWorld::worldWithName(&NSString::from_str("virtues-agent"), mtm) })
            .clone()
    })
}

/// Evaluate an async function body in the page, in the agent's isolated world
/// or the page's own, and return the string it returns.
async fn eval(win: &WebviewWindow, body: &str, agent_world: bool) -> Result<String, String> {
    let (tx, rx) = oneshot::channel::<Result<String, String>>();
    let tx = std::sync::Mutex::new(Some(tx));
    let body = body.to_string();
    on_main(win, move |wk, mtm| unsafe {
        let world: Retained<WKContentWorld> = if agent_world {
            agent_world_on_main(mtm)
        } else {
            WKContentWorld::pageWorld(mtm)
        };
        let done = RcBlock::new(move |result: *mut AnyObject, error: *mut NSError| {
            let out = if !error.is_null() {
                Err(js_error(&*error))
            } else if result.is_null() {
                Ok(String::new())
            } else {
                let text: Retained<NSString> = msg_send![result, description];
                Ok(text.to_string())
            };
            if let Some(tx) = tx.lock().ok().and_then(|mut t| t.take()) {
                let _ = tx.send(out);
            }
        });
        wk.callAsyncJavaScript_arguments_inFrame_inContentWorld_completionHandler(
            &NSString::from_str(&body),
            None,
            None,
            &world,
            Some(&done),
        );
    })
    .await?;
    tokio::time::timeout(Duration::from_secs(20), rx)
        .await
        .map_err(|_| "the page did not answer within 20 seconds".to_string())?
        .map_err(|_| "the page went away".to_string())?
}

//! The owner's browser, driven by the assistant on their box.
//!
//! The Browser is a WKWebView inside the app's main window, beside whatever the
//! owner is looking at: the UI opens a Browser tab in its right pane and reports
//! where that pane sits, and the shell places this native view over it (a native
//! view draws above the page, so it hides whenever the tab is not on screen). It
//! has a persistent cookie jar apart from the app's UI and from Safari: logins
//! the owner makes in it, including "Log in to X", stay in it. While the app runs it
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
//!
//! The owner always sees who is driving. While the assistant acts, the UI's
//! Browser tab shows a bar with Take control and Stop (`browser:agent`), each
//! click shows a cursor on the page, and every step lands in the tab's step log
//! with a thumbnail (`browser:step`). The owner clicking or typing in the page
//! takes control: the assistant's next step is refused until they hand it back.
//! What only the owner can do (a password, a code, a CAPTCHA) the assistant
//! asks for with a handoff, which waits for their Done; it never types into a
//! password, card or one-time-code field.

use std::ptr::NonNull;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use base64::Engine;
use block2::RcBlock;
use futures_util::{SinkExt, StreamExt};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, MainThreadMarker, Message as _};
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSEvent, NSEventMask, NSEventModifierFlags, NSEventType,
    NSImage, NSView,
};
use objc2_foundation::{NSDictionary, NSError, NSNumber, NSPoint, NSProcessInfo, NSString};
use objc2_web_kit::{WKContentWorld, WKInactiveSchedulingPolicy, WKSnapshotConfiguration, WKWebView};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Webview, WebviewBuilder, WebviewUrl};
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
                let reply = match step(&app, &op, &args).await {
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
            // Development only: press the Browser tab's bar buttons from a
            // test, beside the queue, since a handoff holds the queue.
            #[cfg(debug_assertions)]
            if req.get("op").and_then(|o| o.as_str()) == Some("control") {
                let action = req["args"]["action"].as_str().unwrap_or("");
                let reply = match control(app, action) {
                    Ok(()) => json!({ "id": req["id"], "ok": true, "result": null }),
                    Err(e) => json!({ "id": req["id"], "ok": false, "error": e }),
                };
                let _ = tx.send(Message::Text(reply.to_string()));
                continue;
            }
            let _ = jobs.send(req);
        }
    }
    worker.abort();
    pinger.abort();
    writer.abort();
}

// ─── Who is driving ─────────────────────────────────────────────────────────

/// How long after a step the assistant still counts as driving: the gap while
/// the model thinks between two steps.
const DRIVING_FOR: Duration = Duration::from_secs(15);

/// A handoff gives up a little before the box stops waiting on it, so the
/// assistant hears this side's reason rather than a bare timeout.
const HANDOFF_WAIT: Duration = Duration::from_secs(15 * 60 - 10);

const TAKEN_OVER: &str = "The owner has taken over the browser. Leave it alone until they hand it back: \
                          tell them where you got to and what is left, and end your turn.";

struct Agent {
    /// A step is running now.
    busy: bool,
    last_step: Option<Instant>,
    /// The owner took control; steps are refused until they hand it back.
    paused: bool,
    /// What the assistant asked the owner to do, and where their answer goes:
    /// true for Done, false for "I can't".
    handoff: Option<(String, oneshot::Sender<bool>)>,
}

static AGENT: Mutex<Agent> = Mutex::new(Agent { busy: false, last_step: None, paused: false, handoff: None });

fn agent() -> MutexGuard<'static, Agent> {
    AGENT.lock().unwrap_or_else(|e| e.into_inner())
}

fn driving(a: &Agent) -> bool {
    a.busy || a.last_step.is_some_and(|t| t.elapsed() < DRIVING_FOR)
}

/// Tell the UI who is driving, for the Browser tab's bar.
fn announce(app: &AppHandle) {
    let state = {
        let a = agent();
        json!({
            "driving": driving(&a),
            "paused": a.paused,
            "handoff": a.handoff.as_ref().map(|(reason, _)| reason.clone()),
        })
    };
    let _ = app.emit_to("main", "browser:agent", state);
}

/// The Browser tab's bar: `take` control, `resume` (hand it back), `done` or
/// `decline` a handoff, or `stop` (the UI also stops the chat's turn).
pub fn control(app: &AppHandle, action: &str) -> Result<(), String> {
    {
        let mut a = agent();
        match action {
            "take" => a.paused = true,
            "resume" => a.paused = false,
            "done" | "decline" => {
                if let Some((_, answer)) = a.handoff.take() {
                    let _ = answer.send(action == "done");
                }
            }
            "stop" => {
                // Dropping the sender ends a waiting handoff as stopped.
                a.handoff = None;
                a.last_step = None;
            }
            other => return Err(format!("unknown browser control `{other}`")),
        }
    }
    announce(app);
    Ok(())
}

/// The owner clicked or typed in the page. While the assistant is driving,
/// that takes control from it, as in every browser agent: the owner's hand
/// beats the assistant's. Not during a handoff, which asked for exactly this.
fn owner_input(app: &AppHandle) {
    {
        let mut a = agent();
        if a.paused || a.handoff.is_some() || !driving(&a) {
            return;
        }
        a.paused = true;
    }
    announce(app);
}

/// Run one of the assistant's steps: refused while the owner has control,
/// shown as driving while it runs, and recorded in the step log.
async fn step(app: &AppHandle, op: &str, args: &Value) -> Result<Value, String> {
    #[cfg(debug_assertions)]
    if op == "eval" {
        return perform(app, op, args).await;
    }
    if agent().paused {
        return Err(TAKEN_OVER.into());
    }
    agent().busy = true;
    announce(app);
    let out = if op == "handoff" { handoff(app, args).await } else { perform(app, op, args).await };
    {
        let mut a = agent();
        a.busy = false;
        a.last_step = Some(Instant::now());
    }
    announce(app);
    {
        // Once the assistant has been quiet long enough, the bar goes.
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(DRIVING_FOR + Duration::from_millis(100)).await;
            announce(&app);
        });
    }
    record(app, op, args, &out).await;
    out
}

/// Ask the owner to do something only they can, and wait for their Done.
async fn handoff(app: &AppHandle, args: &Value) -> Result<Value, String> {
    let reason: String = args
        .get("reason")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .ok_or("`reason` is required")?
        .chars()
        .take(200)
        .collect();
    let view = pane(app)?;
    let (tx, rx) = oneshot::channel();
    {
        let mut a = agent();
        a.paused = false;
        a.handoff = Some((reason, tx));
    }
    // Bring the Browser tab forward, wherever the owner is.
    let _ = app.emit_to("main", "browser:open", json!({ "url": current_url(&view) }));
    announce(app);
    let answer = tokio::time::timeout(HANDOFF_WAIT, rx).await;
    agent().handoff = None;
    match answer {
        Ok(Ok(true)) => Ok(json!({
            "done": true,
            "url": current_url(&view),
            "note": "The owner pressed Done. Take a fresh browser_snapshot before acting: the page has likely changed.",
        })),
        Ok(Ok(false)) => Err("The owner said they can't do that step. Ask them in the chat how they'd like to go on.".into()),
        Ok(Err(_)) => Err("The owner pressed Stop.".into()),
        Err(_) => Err("The owner didn't press Done within 15 minutes. Ask in the chat whether they still want this.".into()),
    }
}

/// Watch for the owner's own clicks and keys in the page. Real input passes
/// through the app's event queue; the assistant's goes straight to the view
/// (`mouse`, `press`), so this sees only the owner's.
fn watch_owner_input(app: AppHandle, wk: &WKWebView, mtm: MainThreadMarker) {
    let view: Retained<WKWebView> = wk.retain();
    let handler = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: AppKit hands the monitor a live event.
        if touches(&view, unsafe { event.as_ref() }, mtm) {
            owner_input(&app);
        }
        event.as_ptr()
    });
    let mask = NSEventMask::LeftMouseDown | NSEventMask::RightMouseDown | NSEventMask::OtherMouseDown | NSEventMask::KeyDown;
    // SAFETY: the handler returns the event it was given.
    let monitor = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &handler) };
    // The pane lives as long as the app, and so does the monitor.
    std::mem::forget(monitor);
}

/// Whether a real event lands in the browser: a click inside it, or a key
/// while the page has focus.
fn touches(view: &WKWebView, event: &NSEvent, mtm: MainThreadMarker) -> bool {
    if view.isHiddenOrHasHiddenAncestor() {
        return false;
    }
    let Some(window) = view.window() else { return false };
    if event.window(mtm).is_none_or(|w| !std::ptr::eq(&*w, &*window)) {
        return false;
    }
    let view_ref: &NSView = view;
    if event.r#type() == NSEventType::KeyDown {
        return window
            .firstResponder()
            .and_then(|r| r.downcast::<NSView>().ok())
            .is_some_and(|r| r.isDescendantOf(view_ref));
    }
    let p = view.convertPoint_fromView(event.locationInWindow(), None);
    let b = view.bounds();
    p.x >= 0.0 && p.y >= 0.0 && p.x <= b.size.width && p.y <= b.size.height
}

// ─── The step log ───────────────────────────────────────────────────────────

/// Send the UI one line of the step log: what the assistant did, where, and a
/// thumbnail of the page after it. Kept in the app only, for the owner.
async fn record(app: &AppHandle, op: &str, args: &Value, out: &Result<Value, String>) {
    let Ok(view) = pane(app) else { return };
    let thumb = jpeg(&view, 320.0).await.ok().map(|b| format!("data:image/jpeg;base64,{b}"));
    let _ = app.emit_to(
        "main",
        "browser:step",
        json!({
            "what": describe(op, args, out),
            "ok": out.is_ok(),
            "url": current_url(&view),
            "thumb": thumb,
        }),
    );
}

fn describe(op: &str, args: &Value, out: &Result<Value, String>) -> String {
    let arg = |k: &str| args.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let short = |t: String, n: usize| {
        let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
        if t.chars().count() > n { format!("{}…", t.chars().take(n).collect::<String>()) } else { t }
    };
    let host = || {
        arg("url")
            .parse::<tauri::Url>()
            .ok()
            .and_then(|u| u.host_str().map(|h| h.trim_start_matches("www.").to_string()))
            .unwrap_or_else(|| "a page".into())
    };
    if out.is_err() {
        return match op {
            "open" => format!("Couldn't open {}", host()),
            "snapshot" => "Couldn't read the page".into(),
            "click" => "Couldn't click".into(),
            "type" => "Couldn't type".into(),
            "press" => format!("Couldn't press {}", arg("key")),
            "scroll" => "Couldn't scroll".into(),
            "screenshot" => "Couldn't look at the page".into(),
            "handoff" => "Asked for your help; not done".into(),
            _ => format!("Couldn't {op}"),
        };
    }
    match op {
        "open" => format!("Opened {}", host()),
        "snapshot" => "Read the page".into(),
        "click" => {
            let name = out.as_ref().ok().and_then(|v| v["clicked"].as_str()).unwrap_or("").to_string();
            if name.trim().is_empty() { "Clicked".into() } else { format!("Clicked \u{201c}{}\u{201d}", short(name, 40)) }
        }
        "type" => {
            let enter = if args.get("submit").and_then(|v| v.as_bool()) == Some(true) { " and pressed Enter" } else { "" };
            format!("Typed \u{201c}{}\u{201d}{enter}", short(arg("text"), 40))
        }
        "press" => format!("Pressed {}", arg("key")),
        "scroll" => format!("Scrolled {}", if arg("direction") == "up" { "up" } else { "down" }),
        "screenshot" => "Looked at the page".into(),
        "handoff" => format!("You: {}", short(arg("reason"), 60)),
        _ => op.to_string(),
    }
}

async fn perform(app: &AppHandle, op: &str, args: &Value) -> Result<Value, String> {
    let arg = |k: &str| args.get(k).and_then(|v| v.as_str()).map(str::to_string);
    match op {
        "open" => open(app, &arg("url").ok_or("`url` is required")?).await,
        "snapshot" => {
            let depth = args.get("depth").and_then(|v| v.as_u64());
            snapshot(&pane(app)?, arg("ref"), depth).await
        }
        "click" => click(&pane(app)?, &arg("ref").ok_or("`ref` is required")?).await,
        "type" => {
            let win = pane(app)?;
            refuse_secret_field(&win, arg("ref")).await?;
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
            let win = pane(app)?;
            press(&win, &arg("key").ok_or("`key` is required")?).await?;
            tokio::time::sleep(Duration::from_millis(500)).await;
            Ok(json!({ "pressed": true, "url": current_url(&win) }))
        }
        "scroll" => {
            let up = arg("direction").as_deref() == Some("up");
            let px = args.get("pixels").and_then(|v| v.as_f64()).unwrap_or(800.0).clamp(100.0, 4000.0);
            scroll(&pane(app)?, if up { -px } else { px }).await
        }
        "screenshot" => screenshot(&pane(app)?).await,
        // Development only: run a script body in the page and return its
        // string. Never reachable from a release build, so never from a model.
        #[cfg(debug_assertions)]
        "eval" => {
            let js = arg("js").ok_or("`js` is required")?;
            let agent = arg("world").as_deref() == Some("agent");
            Ok(json!({ "value": eval(&pane(app)?, &js, agent).await? }))
        }
        other => Err(format!("unknown browser operation `{other}`")),
    }
}

// ─── The pane ───────────────────────────────────────────────────────────────

fn pane(app: &AppHandle) -> Result<Webview, String> {
    app.get_webview(LABEL)
        .ok_or_else(|| "The browser is not open. Call browser_open with a URL first.".into())
}

fn current_url(view: &Webview) -> String {
    view.url().map(|u| u.to_string()).unwrap_or_default()
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

fn parse_page(url: &str) -> Result<tauri::Url, String> {
    let target: tauri::Url = url.parse().map_err(|_| format!("`{url}` is not a URL"))?;
    if !matches!(target.scheme(), "https" | "http") {
        return Err("Only http and https pages can be opened.".into());
    }
    Ok(target)
}

/// Show `target` in the pane, creating it the first time. `announce` asks the
/// UI to open the Browser tab beside the current view; the UI's own Browser
/// tab passes false, since it is already the one asking.
pub async fn show(app: &AppHandle, target: &tauri::Url, announce: bool) -> Result<Webview, String> {
    let view = match app.get_webview(LABEL) {
        Some(view) => {
            if view.url().ok().as_ref() != Some(target) {
                // Mark the current document so the new one can be told apart.
                let _ = eval(&view, "window.__virtuesNav = 1; return '1'", false).await;
                view.navigate(target.clone()).map_err(|e| e.to_string())?;
            }
            view
        }
        None => create(app, target).await?,
    };
    if announce {
        let _ = app.emit_to("main", "browser:open", json!({ "url": target.as_str() }));
    }
    Ok(view)
}

async fn create(app: &AppHandle, target: &tauri::Url) -> Result<Webview, String> {
    let main = app
        .get_window("main")
        .ok_or("The app's window is closed, so the browser has nowhere to open.")?;
    let builder = WebviewBuilder::new(LABEL, WebviewUrl::External(target.clone()))
        .user_agent(SAFARI_UA)
        .data_store_identifier(store_id())
        .on_new_window(|_, _| tauri::webview::NewWindowResponse::Allow)
        .on_page_load(|view, payload| {
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                let _ = view
                    .app_handle()
                    .emit_to("main", "browser:navigated", json!({ "url": payload.url().as_str() }));
            }
        });
    // Created hidden, at the size of a laptop browser window: the UI reports
    // where the pane is once its Browser tab lays out (`set_bounds`), and until
    // then (or while that tab is off screen) the assistant still needs a page
    // laid out at a real width. Created at 1x1, every click missed.
    let view = main
        .add_child(builder, LogicalPosition::new(0.0, 0.0), LogicalSize::new(1100.0, 800.0))
        .map_err(|e| format!("could not open the browser: {e}"))?;
    let _ = view.hide();
    let owner = app.clone();
    on_main(&view, move |wk, mtm| unsafe {
        watch_owner_input(owner, wk, mtm);
        wk.configuration()
            .preferences()
            .setInactiveSchedulingPolicy(WKInactiveSchedulingPolicy::None);
        // A covered window is "hidden" to WebKit, and X's timeline does not
        // load more for a hidden page: the assistant scrolling it behind the
        // owner's other windows reached six posts and stopped. WebKit SPI,
        // under the macos-private-api flag this app already ships with.
        let sel = objc2::sel!(_setWindowOcclusionDetectionEnabled:);
        let can: bool = msg_send![wk, respondsToSelector: sel];
        if can {
            let _: () = msg_send![wk, _setWindowOcclusionDetectionEnabled: false];
        }
    })
    .await?;
    Ok(view)
}

/// Where the UI's Browser pane is, in the main window's logical points, or
/// that it is not on screen.
pub fn set_bounds(app: &AppHandle, x: f64, y: f64, width: f64, height: f64, visible: bool) -> Result<(), String> {
    let Some(view) = app.get_webview(LABEL) else { return Ok(()) };
    if !visible || width < 1.0 || height < 1.0 {
        return view.hide().map_err(|e| e.to_string());
    }
    view.set_position(LogicalPosition::new(x, y)).map_err(|e| e.to_string())?;
    view.set_size(LogicalSize::new(width, height)).map_err(|e| e.to_string())?;
    view.show().map_err(|e| e.to_string())
}

/// Back, forward or reload, from the Browser tab's toolbar.
pub async fn go(app: &AppHandle, action: &str) -> Result<(), String> {
    let view = pane(app)?;
    match action {
        "back" => eval(&view, "history.back(); return ''", false).await.map(|_| ()),
        "forward" => eval(&view, "history.forward(); return ''", false).await.map(|_| ()),
        "reload" => view.reload().map_err(|e| e.to_string()),
        other => Err(format!("unknown browser action `{other}`")),
    }
}

/// Open a source's login page in the pane and resolve with the site's cookies
/// once every name in `cookies` is set. The session stays in the browser's jar,
/// so the assistant browsing that site afterwards is logged in too.
pub async fn login(app: &AppHandle, url: &str, cookies: &[String], timeout: Duration) -> Result<Vec<(String, String)>, String> {
    let page = parse_page(url)?;
    let view = show(app, &page, true).await?;
    let started = std::time::Instant::now();
    loop {
        tokio::time::sleep(Duration::from_millis(800)).await;
        if started.elapsed() > timeout {
            return Err("timeout".into());
        }
        let Ok(all) = view.cookies() else { continue };
        let jar = crate::browser::jar_for(all, &page);
        let has = |name: &str| jar.iter().any(|c| c.name() == name && !c.value().is_empty());
        if cookies.iter().all(|n| has(n)) {
            return Ok(jar.iter().map(|c| (c.name().to_string(), c.value().to_string())).collect());
        }
    }
}

async fn open(app: &AppHandle, url: &str) -> Result<Value, String> {
    let target = parse_page(url)?;
    let win = show(app, &target, true).await?;

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

async fn ensure_agent(win: &Webview) -> Result<(), String> {
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

async fn snapshot(win: &Webview, scope: Option<String>, depth: Option<u64>) -> Result<Value, String> {
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

/// The assistant never types a password, a card or a one-time code: it does not
/// have them, and a page that asks the assistant for one is a page to be wary
/// of. Checks the field `ref` names, or the focused one.
async fn refuse_secret_field(win: &Webview, r#ref: Option<String>) -> Result<(), String> {
    ensure_agent(win).await?;
    let body = format!(
        "const ref = {ref_json};\n\
         const e = ref ? window.__virtuesAgent._lastAriaSnapshotForQuery?.info?.get(ref)?.element : document.activeElement;\n\
         if (!e || !e.getAttribute) return '';\n\
         const type = (e.getAttribute('type') || '').toLowerCase();\n\
         const auto = (e.getAttribute('autocomplete') || '').toLowerCase();\n\
         if (type === 'password' || /(^|\\s)(current|new)-password/.test(auto)) return 'password';\n\
         if (/one-time-code/.test(auto)) return 'code';\n\
         if (/(^|\\s)cc-/.test(auto)) return 'card';\n\
         return '';",
        ref_json = serde_json::to_string(&r#ref).unwrap_or_else(|_| "null".into())
    );
    match eval(win, &body, true).await?.as_str() {
        "password" => Err("That is a password field, and you never type passwords. Call browser_handoff and ask the owner to sign in.".into()),
        "code" => Err("That field wants a one-time code, which only the owner has. Call browser_handoff and ask them to enter it.".into()),
        "card" => Err("That is a payment card field. Call browser_handoff and let the owner enter their card.".into()),
        _ => Ok(()),
    }
}

/// Show the owner where the assistant is about to click: a cursor at the point
/// and a pulse around it. Pointer-events none, so the click goes through it,
/// and in a closed shadow root, so the page's styles cannot reach it. It jumps
/// rather than glides: a pane off screen freezes transitions part way.
async fn show_cursor(win: &Webview, x: f64, y: f64) {
    let body = format!(
        "const x = {x}, y = {y};\n\
         let s = window.__virtuesCursor;\n\
         if (!s || !s.host.isConnected) {{\n\
           const host = document.createElement('div');\n\
           host.style.cssText = 'all:initial;position:fixed;inset:0;pointer-events:none;z-index:2147483647';\n\
           const root = host.attachShadow({{ mode: 'closed' }});\n\
           root.innerHTML = `<style>\n\
             .c{{position:fixed;left:0;top:0;width:22px;height:22px;transition:opacity .3s;opacity:0;filter:drop-shadow(0 1px 2px rgba(0,0,0,.35))}}\n\
             .r{{position:fixed;left:0;top:0;width:28px;height:28px;margin:-14px 0 0 -14px;border-radius:50%;border:2px solid #4f7cff;opacity:0}}\n\
             .r.go{{animation:p .6s ease-out}}\n\
             @keyframes p{{from{{opacity:.9;transform:var(--at) scale(.4)}}to{{opacity:0;transform:var(--at) scale(1.6)}}}}\n\
           </style><div class=r></div><svg class=c viewBox=\"0 0 22 22\"><path d=\"M3 2l15 8.5-6.6 1.5L8.2 18z\" fill=\"#4f7cff\" stroke=\"#fff\" stroke-width=\"1.5\" stroke-linejoin=\"round\"/></svg>`;\n\
           document.documentElement.appendChild(host);\n\
           s = window.__virtuesCursor = {{ host, c: root.querySelector('.c'), r: root.querySelector('.r') }};\n\
         }}\n\
         const at = `translate(${{x}}px, ${{y}}px)`;\n\
         s.c.style.opacity = '1';\n\
         s.c.style.transform = `translate(${{x - 3}}px, ${{y - 2}}px)`;\n\
         s.r.style.setProperty('--at', at);\n\
         s.r.classList.remove('go'); void s.r.offsetWidth; s.r.classList.add('go');\n\
         clearTimeout(s.t); s.t = setTimeout(() => {{ s.c.style.opacity = '0'; }}, 2500);\n\
         return '';"
    );
    let _ = eval(win, &body, true).await;
}

async fn click(win: &Webview, r#ref: &str) -> Result<Value, String> {
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
         const said = n => {{ const t = n.closest('[role=dialog],dialog,[aria-modal=true]') || n; const name = (t.getAttribute('aria-label') || t.innerText || '').trim().replace(/\\s+/g, ' ').slice(0, 80); return (t.getAttribute('role') || t.tagName.toLowerCase()) + (name ? ' \"' + name + '\"' : ''); }};\n\
         return JSON.stringify({{ x, y, lands, viewport: innerHeight, width: innerWidth, covering: hit && !lands ? said(hit) : null, name: (e.getAttribute('aria-label') || e.innerText || '').trim().slice(0, 60) }});",
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
        let covering = at["covering"].as_str().unwrap_or("something");
        return Err(format!(
            "`{}` is covered by {covering} at the point it would be clicked (page {}x{}). \
             Close or dismiss that first, or take a fresh browser_snapshot and pick another ref.",
            r#ref, at["width"], at["viewport"]
        ));
    }
    let (x, y) = (at["x"].as_f64().unwrap_or(0.0), at["y"].as_f64().unwrap_or(0.0));
    let viewport = at["viewport"].as_f64().unwrap_or(0.0);
    show_cursor(win, x, y).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
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

async fn press(win: &Webview, name: &str) -> Result<(), String> {
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
async fn scroll(win: &Webview, dy: f64) -> Result<Value, String> {
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

async fn screenshot(win: &Webview) -> Result<Value, String> {
    // The click cursor is for the owner; the model sees the page alone.
    let cursor = "const s = window.__virtuesCursor; if (s) s.host.style.display = DISPLAY; return ''";
    let _ = eval(win, &cursor.replace("DISPLAY", "'none'"), true).await;
    // Points; a Retina screen doubles it. Enough to read, small enough to send
    // with every look.
    let jpeg = jpeg(win, 900.0).await;
    let _ = eval(win, &cursor.replace("DISPLAY", "''"), true).await;
    let jpeg = jpeg?;
    Ok(json!({ "url": current_url(win), "jpeg_base64": jpeg }))
}

/// The visible page as a base64 JPEG, `width` points wide.
async fn jpeg(win: &Webview, width: f64) -> Result<String, String> {
    let (tx, rx) = oneshot::channel::<Result<String, String>>();
    let tx = std::sync::Mutex::new(Some(tx));
    on_main(win, move |wk, mtm| unsafe {
        let config = WKSnapshotConfiguration::new(mtm);
        config.setSnapshotWidth(Some(&NSNumber::new_f64(width)));
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
    tokio::time::timeout(Duration::from_secs(15), rx)
        .await
        .map_err(|_| "the screenshot timed out".to_string())?
        .map_err(|_| "the screenshot was dropped".to_string())?
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
async fn on_main<F>(win: &Webview, f: F) -> Result<(), String>
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
async fn eval(win: &Webview, body: &str, agent_world: bool) -> Result<String, String> {
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

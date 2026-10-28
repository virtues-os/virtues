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
//! asks for with a handoff, which waits for their Done.
//!
//! What the assistant cannot reach through the pane:
//! - Local addresses. The pane refuses localhost, loopback, private and
//!   link-local addresses and `.local` names, for `browser_open` and for every
//!   navigation inside the page (`pane_may_load`). For the address it is asked
//!   to open, it also looks the name up and refuses one that points at a local
//!   address (`resolves_local`). On this Mac 127.0.0.1:7117 is the reach
//!   loopback, which the box answers as the owner, so a page that sent the
//!   pane there could have shown the assistant saved passwords.
//! - Secret fields. The assistant cannot type into a field the pane
//!   recognizes as a password, card or one-time-code field, into a frame from
//!   another site, or into a page component whose inside is closed to scripts.
//!   Snapshots and screenshots show such a field as filled without its value
//!   (`virtuesSecretField` in the vendored script). A screenshot also blanks
//!   frames from another site that look like payment or code forms, since the
//!   fields inside them cannot be checked.
//!
//! All of these are best-effort checks against pages that behave normally.

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
/// The page's corner radius, inside the Browser tab's 12px card and its 2px
/// frame (`.stage` in BrowserView.svelte).
const PAGE_RADIUS: f64 = 10.0;

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

/// The ops this app handles beyond the browser, named in the socket URL. The
/// box offers a tool that needs one only to an app that names it, so an op
/// added to `serve` must be added here too.
const OPS: &str = "imessage_send";

fn endpoint(paired: fn() -> bool) -> Option<String> {
    // A dev override, for driving the browser from a scratch core without
    // pairing. Debug builds only: a release app talks to its own box.
    #[cfg(debug_assertions)]
    if let Ok(url) = std::env::var("VIRTUES_BROWSER_WS") {
        let sep = if url.contains('?') { '&' } else { '?' };
        return Some(format!("{url}{sep}ops={OPS}"));
    }
    paired().then(|| format!("ws://127.0.0.1:{}/ws/browser?ops={OPS}", tauri_plugin_reach::loopback_port()))
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
            // A message for Messages, not a step in the page: beside the
            // queue, which a handoff can hold for minutes.
            if req.get("op").and_then(|o| o.as_str()) == Some("imessage_send") {
                let (app, tx) = (app.clone(), tx.clone());
                tauri::async_runtime::spawn(async move {
                    let reply = match crate::messages_send::send(&app, &req["args"]).await {
                        Ok(result) => json!({ "id": req["id"], "ok": true, "result": result }),
                        Err(error) => json!({ "id": req["id"], "ok": false, "error": error }),
                    };
                    let _ = tx.send(Message::Text(reply.to_string()));
                });
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

/// How long the owner's takeover outlives the assistant's last try. Long
/// enough to stop the task they interrupted, whose next step comes within
/// seconds; short enough that the next chat finds the browser free. Held
/// until Hand back, it outlived its task: a click in the page just after a
/// reply finished left every later chat refused.
const TAKEOVER_LAPSES: Duration = Duration::from_secs(20);

const TAKEN_OVER: &str = "The owner has taken over the browser. Leave it alone until they hand it back: \
                          tell them where you got to and what is left, and end your turn.";

struct Agent {
    /// A step is running now.
    busy: bool,
    last_step: Option<Instant>,
    /// The owner took control; steps are refused until they hand it back, or
    /// until the assistant has been quiet for [`TAKEOVER_LAPSES`].
    paused: bool,
    /// When the owner took control, or the assistant last tried a step since.
    paused_touch: Option<Instant>,
    /// What the assistant asked the owner to do, and where their answer goes:
    /// true for Done, false for "I can't".
    handoff: Option<(String, oneshot::Sender<bool>)>,
}

static AGENT: Mutex<Agent> = Mutex::new(Agent { busy: false, last_step: None, paused: false, paused_touch: None, handoff: None });

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
            "take" => {
                a.paused = true;
                a.paused_touch = Some(Instant::now());
            }
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
    if action == "take" {
        lapse_later(app);
    }
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
        a.paused_touch = Some(Instant::now());
    }
    announce(app);
    lapse_later(app);
}

/// Let the owner's takeover go once the assistant has been quiet long enough.
fn lapse_later(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(TAKEOVER_LAPSES + Duration::from_millis(100)).await;
        let lapsed = {
            let mut a = agent();
            let quiet = a.paused_touch.is_none_or(|t| t.elapsed() >= TAKEOVER_LAPSES);
            if a.paused && quiet {
                a.paused = false;
                true
            } else {
                false
            }
        };
        if lapsed {
            announce(&app);
        }
    });
}

/// Run one of the assistant's steps: refused while the owner has control,
/// shown as driving while it runs, and recorded in the step log.
async fn step(app: &AppHandle, op: &str, args: &Value) -> Result<Value, String> {
    #[cfg(debug_assertions)]
    if op == "eval" {
        return perform(app, op, args).await;
    }
    {
        let mut a = agent();
        if a.paused {
            if a.paused_touch.is_some_and(|t| t.elapsed() < TAKEOVER_LAPSES) {
                // Still the task the owner interrupted: refuse, and keep the
                // takeover alive while it keeps trying.
                a.paused_touch = Some(Instant::now());
                drop(a);
                lapse_later(app);
                return Err(TAKEN_OVER.into());
            }
            a.paused = false;
        }
    }
    agent().busy = true;
    announce(app);
    let started = Instant::now();
    let mut out = if op == "handoff" { handoff(app, args).await } else { perform(app, op, args).await };
    {
        let mut a = agent();
        a.busy = false;
        a.last_step = Some(Instant::now());
    }
    // A link, a redirect or a form that tried to take the pane somewhere it
    // refuses: say so, or the step reads as if the page simply did nothing.
    if let Ok(result) = out.as_mut() {
        if let Some((_, refused)) = refused_since(started) {
            if result.is_object() && result.get("note").is_none() {
                result["note"] = json!(refused.note());
            }
        }
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
            "op": op,
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
            "handoff" => "Your turn: not done".into(),
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
        "handoff" => format!("Your turn: {}", short(arg("reason"), 60)),
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
            let text = arg("text").ok_or("`text` is required")?;
            if let Some(r) = arg("ref") {
                refuse_secret_field(&win, Some(&r)).await?;
                click(&win, &r).await?;
            }
            // Checked again where the text will land. Clicking a label focuses
            // its input, and a page can move focus on its own.
            refuse_secret_field(&win, None).await?;
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

const LOCAL_REFUSED: &str = "The Browser does not open local addresses: localhost, 127.0.0.1, private network \
                             addresses and .local names. It opens public websites only.";

/// What the pane last refused, for telling the assistant.
#[derive(Clone, Debug, PartialEq)]
enum Refused {
    /// A local address.
    Local,
    /// A link that is not a web page, such as `mailto:`, another app's link
    /// or the app's own scheme.
    Scheme(String),
}

impl Refused {
    fn of(url: &tauri::Url) -> Self {
        match url.scheme() {
            "http" | "https" => Refused::Local,
            // Capped, since the page picks it.
            other => Refused::Scheme(other.chars().take(24).collect()),
        }
    }

    /// For `open`, when the page it asked for went somewhere the pane refused.
    fn redirect(&self) -> String {
        match self {
            Refused::Local => {
                "The page sent the Browser to a local address, which it does not open. It opens public websites only."
                    .into()
            }
            Refused::Scheme(scheme) => {
                format!("The page sent the Browser to a `{scheme}:` link, which it does not open. It opens web pages only.")
            }
        }
    }

    /// For any other step, as a note on its result.
    fn note(&self) -> String {
        match self {
            Refused::Local => "The page tried to load a local address, and the Browser refused it.".into(),
            Refused::Scheme(scheme) => format!(
                "The page tried to open a `{scheme}:` link, and the Browser refused it. It opens web pages only."
            ),
        }
    }
}

/// A page the Browser may show: http or https, and not a local address.
fn parse_page(url: &str) -> Result<tauri::Url, String> {
    let target: tauri::Url = url.parse().map_err(|_| format!("`{url}` is not a URL"))?;
    if !matches!(target.scheme(), "https" | "http") {
        return Err("Only http and https pages can be opened.".into());
    }
    if !pane_may_load(&target) {
        return Err(LOCAL_REFUSED.into());
    }
    Ok(target)
}

/// `parse_page`, and then the name looked up: the check for an address the
/// assistant or the owner asks the pane to open.
pub(crate) async fn checked_page(url: &str) -> Result<tauri::Url, String> {
    let target = parse_page(url)?;
    if resolves_local(&target).await {
        return Err(LOCAL_REFUSED.into());
    }
    Ok(target)
}

/// Whether the URL's name resolves to a local address on this Mac. A public
/// name can point anywhere: wildcard DNS such as `127.0.0.1.nip.io` answers
/// with the address in the name, and `pane_may_load` sees only the name.
///
/// Only the address asked for is looked up. A page's own redirects and links
/// are not, because the navigation check runs on the main thread, where a
/// lookup would stall the app. A name can also answer differently a moment
/// later (DNS rebinding). A box that refuses a `Host` it does not serve stops
/// both; a box without that check does not.
///
/// A name that does not resolve, or not within a few seconds, is let through.
/// The pane is unlikely to load it either, and whoever controls a name's
/// answers can rebind it anyway.
async fn resolves_local(url: &tauri::Url) -> bool {
    let Some(url::Host::Domain(name)) = url.host() else { return false };
    let (name, port) = (name.to_string(), url.port_or_known_default().unwrap_or(443));
    let lookup = tauri::async_runtime::spawn_blocking(move || {
        use std::net::ToSocketAddrs as _;
        (name.as_str(), port).to_socket_addrs().map(|found| found.collect::<Vec<_>>())
    });
    match tokio::time::timeout(Duration::from_secs(5), lookup).await {
        Ok(Ok(Ok(found))) => found.iter().any(|at| is_local_ip(at.ip())),
        _ => false,
    }
}

/// Whether the pane may load `url`, as a page, a frame or a popup.
///
/// Local addresses are refused because the pane is not a sandbox from this
/// Mac. Anything on 127.0.0.1:7117 is spliced to the box under this device's
/// key, so the box answers it as the owner, and the assistant reads whatever
/// the pane shows. The app's own schemes (`virtues:`, `tauri:`) are refused
/// because a page on them is given the app's commands. `about:`, `data:` and
/// `blob:` documents belong to the page that made them.
///
/// A public name that resolves to a local address passes here: see
/// `resolves_local` for what catches it and what does not.
fn pane_may_load(url: &tauri::Url) -> bool {
    match url.scheme() {
        "http" | "https" => url.host().is_some_and(|host| !is_local_host(host)),
        "about" | "data" | "blob" => true,
        _ => false,
    }
}

/// The URL parser has already read every numeric form of an address (decimal
/// `2130706433`, hex `0x7f000001`, short `127.1`, bracketed IPv6) into an IP,
/// so the checks below look at the address itself, never at its spelling.
fn is_local_host(host: url::Host<&str>) -> bool {
    match host {
        url::Host::Ipv4(ip) => is_local_ipv4(ip),
        url::Host::Ipv6(ip) => is_local_ipv6(ip),
        url::Host::Domain(name) => is_local_name(name),
    }
}

fn is_local_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => is_local_ipv4(v4),
        std::net::IpAddr::V6(v6) => is_local_ipv6(v6),
    }
}

fn is_local_ipv4(ip: std::net::Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    a == 0 // 0.0.0.0/8: on macOS a connection to 0.0.0.0 reaches this machine
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || (a == 100 && (b & 0xc0) == 64) // 100.64.0.0/10: carrier NAT and tailnets
        || ip.is_broadcast()
}

fn is_local_ipv6(ip: std::net::Ipv6Addr) -> bool {
    // `::ffff:a.b.c.d` and the older `::a.b.c.d` carry an IPv4 address, and
    // `::1` and `::` fall in the second form.
    if let Some(v4) = ip.to_ipv4() {
        return is_local_ipv4(v4);
    }
    let first = ip.segments()[0];
    (first & 0xfe00) == 0xfc00 // fc00::/7, unique local
        || (first & 0xffc0) == 0xfe80 // fe80::/10, link-local
        || (first & 0xffc0) == 0xfec0 // fec0::/10, the retired site-local block
}

/// Names that resolve only on this machine or its network. A single label
/// resolves through the network's own search domains. `.virtues` is the box's
/// own name on a LAN. `.test` is reserved for testing, and local development
/// servers (Laravel Valet, dnsmasq) point it at this Mac; `.localdomain` is the
/// usual suffix of a machine's own name.
fn is_local_name(name: &str) -> bool {
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    !name.contains('.')
        || ["localhost", "local", "lan", "home", "internal", "home.arpa", "virtues", "test", "localdomain"]
            .iter()
            .any(|suffix| name == *suffix || name.ends_with(&format!(".{suffix}")))
}

/// When the pane last refused a navigation, and what it refused. `open` and
/// the step log read it to tell the assistant why a page went nowhere.
static REFUSED_AT: Mutex<Option<(Instant, Refused)>> = Mutex::new(None);

fn note_refusal(url: &tauri::Url) {
    *REFUSED_AT.lock().unwrap_or_else(|e| e.into_inner()) = Some((Instant::now(), Refused::of(url)));
}

fn refused_since(since: Instant) -> Option<(Instant, Refused)> {
    REFUSED_AT.lock().unwrap_or_else(|e| e.into_inner()).clone().filter(|(at, _)| *at >= since)
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
        // Every navigation in the pane, a frame's included: links, redirects,
        // form posts, history and reloads. A refused one is cancelled and the
        // page stays where it was.
        .on_navigation(|url| {
            let allowed = pane_may_load(url);
            if !allowed {
                note_refusal(url);
                eprintln!("[browser] refused a navigation to a local address or a link that is not a web page");
            }
            allowed
        })
        // Popups (a provider's sign-in window) open in a WebKit window of
        // their own. Only the first address is checked: that window has no
        // navigation check after it. The assistant cannot read it.
        .on_new_window(|url, _| {
            if pane_may_load(&url) {
                tauri::webview::NewWindowResponse::Allow
            } else {
                note_refusal(&url);
                eprintln!("[browser] refused a popup to a local address or a link that is not a web page");
                tauri::webview::NewWindowResponse::Deny
            }
        })
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
        // Round the page to the card the UI draws around it.
        wk.setWantsLayer(true);
        let layer: *mut AnyObject = msg_send![wk, layer];
        if !layer.is_null() {
            let _: () = msg_send![layer, setCornerRadius: PAGE_RADIUS];
            let _: () = msg_send![layer, setMasksToBounds: true];
        }
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
pub async fn set_bounds(app: &AppHandle, x: f64, y: f64, width: f64, height: f64, visible: bool) -> Result<(), String> {
    let Some(view) = app.get_webview(LABEL) else { return Ok(()) };
    if !visible || width < 1.0 || height < 1.0 {
        return view.hide().map_err(|e| e.to_string());
    }
    // The UI measures from the top of its page, and the page runs to the
    // window's top edge (the title bar is an overlay, main.rs), which is
    // where the pane is placed from too: the two origins agree.
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
pub async fn login(
    app: &AppHandle,
    url: &str,
    cookies: &[String],
    domains: &[String],
    timeout: Duration,
) -> Result<Vec<tauri::webview::Cookie<'static>>, String> {
    let page = checked_page(url).await?;
    let view = show(app, &page, true).await?;
    let started = std::time::Instant::now();
    loop {
        tokio::time::sleep(Duration::from_millis(800)).await;
        if started.elapsed() > timeout {
            return Err("timeout".into());
        }
        let Ok(all) = view.cookies() else { continue };
        let jar = crate::browser::site_jar(all, &page, domains);
        let has = |name: &str| jar.iter().any(|c| c.name() == name && !c.value().is_empty());
        if cookies.iter().all(|n| has(n)) {
            return Ok(jar);
        }
    }
}

async fn open(app: &AppHandle, url: &str) -> Result<Value, String> {
    let target = checked_page(url).await?;
    let asked = Instant::now();
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
        // A redirect the pane refused was cancelled: the old document is
        // still there, and waiting longer would show nothing new. A second's
        // grace, since a refused frame inside a page that is still loading
        // looks the same at first.
        let stayed = state.as_ref().is_some_and(|s| s["fresh"] != true || s["url"] == "about:blank");
        if let Some((at, refused)) = refused_since(asked) {
            if !done && stayed && at.elapsed() > Duration::from_secs(1) {
                return Err(refused.redirect());
            }
        }
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
    // The vendored script's own test for a secret field is also what `type`
    // and `screenshot` ask, so the three never disagree about a field.
    let bootstrap = format!(
        "const module = {{}};\n{INJECTED}\nwindow.__virtuesSecretField = virtuesSecretField;\n\
         window.__virtuesAgent = new (module.exports.InjectedScript())(window, \
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

/// The assistant does not type a password, a card or a one-time code: it does
/// not have them, and a page that asks the assistant for one is a page to be
/// wary of. Checks the field `ref` names, or with no ref the element that has
/// focus, followed down through open shadow roots and frames from the same
/// site. A frame from another site is refused as well: the assistant cannot
/// see into it, and payment and code forms are usually built that way.
///
/// So is focus inside a closed shadow root, where the page reports the
/// component itself as focused. A custom element (a name with a hyphen) can
/// only hold focus that way when it has no open shadow root, no `tabindex`
/// and is not editable itself.
async fn refuse_secret_field(win: &Webview, r#ref: Option<&str>) -> Result<(), String> {
    ensure_agent(win).await?;
    let body = format!(
        "const ref = {ref_json};\n\
         const kind = window.__virtuesSecretField;\n\
         if (typeof kind !== 'function') return 'unchecked';\n\
         let e = ref ? window.__virtuesAgent._lastAriaSnapshotForQuery?.info?.get(ref)?.element : document.activeElement;\n\
         for (let hops = 0; e && hops < 32; hops++) {{\n\
           if (e.shadowRoot && e.shadowRoot.activeElement) {{ e = e.shadowRoot.activeElement; continue; }}\n\
           const tag = (e.tagName || '').toUpperCase();\n\
           if (tag !== 'IFRAME' && tag !== 'FRAME') break;\n\
           let doc = null;\n\
           try {{ doc = e.contentDocument; }} catch (_) {{}}\n\
           if (!doc) return 'frame';\n\
           const inner = doc.activeElement;\n\
           if (!inner || inner === doc.body || inner === doc.documentElement) return '';\n\
           e = inner;\n\
         }}\n\
         if (!ref && e && (e.localName || '').includes('-') && !e.shadowRoot && !e.isContentEditable && !e.hasAttribute('tabindex')) return 'closed';\n\
         return e && e.getAttribute ? kind(e) : '';",
        ref_json = serde_json::to_string(&r#ref).unwrap_or_else(|_| "null".into())
    );
    match eval(win, &body, true).await?.as_str() {
        "password" => Err("That is a password field, and you never type passwords. Call browser_handoff and ask the owner to sign in.".into()),
        "code" => Err("That field wants a one-time code, which only the owner has. Call browser_handoff and ask them to enter it.".into()),
        "card" => Err("That is a payment card field. Call browser_handoff and let the owner enter their card.".into()),
        "frame" => Err("That field is inside a frame from another site, such as a payment or sign-in form, which you cannot read. Call browser_handoff and ask the owner to fill it in.".into()),
        "closed" => Err("That field is inside a page component the browser cannot look into, so it cannot tell whether the field wants a password, a card or a code. Call browser_handoff and ask the owner to fill it in.".into()),
        "unchecked" => Err("The browser could not check that field. Take a fresh browser_snapshot and try again.".into()),
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

/// Hides, or shows again, what the model's picture of the page must not have:
/// the click cursor, which is for the owner, and the text of every filled
/// secret field, drawn as dots while the picture is taken. A card number or a
/// code in an ordinary input is otherwise drawn in plain digits. Fields are
/// found in the page, its open shadow roots and its frames from the same site.
/// A frame from another site cannot be looked into, so one whose title, name
/// or address says it is a payment or code form is blanked instead: card forms
/// at checkout are usually built that way. Returns how many frames it
/// blanked, or `unchecked` when the field test is missing.
const SCREENSHOT_SHADE: &str = "const hide = HIDE;\n\
     const s = window.__virtuesCursor; if (s) s.host.style.display = hide ? 'none' : '';\n\
     if (!hide) {\n\
       for (const [e, name, value, priority] of window.__virtuesShaded || []) {\n\
         if (value) e.style.setProperty(name, value, priority); else e.style.removeProperty(name);\n\
       }\n\
       window.__virtuesShaded = null;\n\
       return '';\n\
     }\n\
     const kind = window.__virtuesSecretField;\n\
     if (typeof kind !== 'function') return 'unchecked';\n\
     const payment = /card|payment|checkout|secure|cvc|cvv|stripe|braintree|adyen|hosted[-_ ]?field|one[-_ ]?time|otp/i;\n\
     const fields = [], frames = [];\n\
     const visit = root => { for (const e of root.querySelectorAll('*')) {\n\
       if (e.shadowRoot) visit(e.shadowRoot);\n\
       if (e.tagName === 'IFRAME' || e.tagName === 'FRAME') {\n\
         let doc = null;\n\
         try { doc = e.contentDocument; } catch (_) {}\n\
         if (doc) visit(doc);\n\
         else if (payment.test([e.title, e.name, e.id, e.src, e.getAttribute('aria-label')].join(' '))) frames.push(e);\n\
       } else if (e.value && kind(e)) fields.push(e);\n\
     } };\n\
     visit(document);\n\
     const shade = [...fields.map(e => [e, '-webkit-text-security', 'disc']), ...frames.map(e => [e, 'visibility', 'hidden'])];\n\
     window.__virtuesShaded = shade.map(([e, name]) => [e, name, e.style.getPropertyValue(name), e.style.getPropertyPriority(name)]);\n\
     for (const [e, name, value] of shade) e.style.setProperty(name, value, 'important');\n\
     return String(frames.length);";

async fn screenshot(win: &Webview) -> Result<Value, String> {
    ensure_agent(win).await?;
    let blanked = match eval(win, &SCREENSHOT_SHADE.replace("HIDE", "true"), true).await {
        Ok(frames) if frames != "unchecked" => frames.parse::<u32>().unwrap_or(0),
        failed => {
            let _ = eval(win, &SCREENSHOT_SHADE.replace("HIDE", "false"), true).await;
            return Err(failed.err().unwrap_or_else(|| {
                "The browser could not hide the page's secret fields, so it took no picture. \
                 Try browser_screenshot again."
                    .into()
            }));
        }
    };
    // Points; a Retina screen doubles it. Enough to read, small enough to send
    // with every look.
    let jpeg = jpeg(win, 900.0).await;
    let _ = eval(win, &SCREENSHOT_SHADE.replace("HIDE", "false"), true).await;
    let jpeg = jpeg?;
    let mut out = json!({ "url": current_url(win), "jpeg_base64": jpeg });
    if blanked > 0 {
        out["note"] = json!(
            "Frames from another site that look like payment or code forms are blank in this picture. \
             Their fields are for the owner to fill in."
        );
    }
    Ok(out)
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

#[cfg(test)]
mod tests {
    use super::{checked_page, pane_may_load, parse_page, resolves_local, Refused, INJECTED, LOCAL_REFUSED};

    fn may(url: &str) -> bool {
        pane_may_load(&url.parse().unwrap())
    }

    #[test]
    fn public_pages_load() {
        for url in [
            "https://example.com/",
            "http://example.com:8080/a?b=c",
            "https://8.8.8.8/",
            "https://[2606:4700:4700::1111]/",
            "https://localhost.example.com/",
            "https://www.local.example.com/",
        ] {
            assert!(may(url), "{url} should load");
        }
    }

    #[test]
    fn loopback_is_refused_in_every_spelling() {
        for url in [
            "http://127.0.0.1:7117/api/passwords",
            "http://localhost:7117/",
            "http://LOCALHOST./",
            "http://app.localhost/",
            "http://2130706433:7117/",
            "http://0x7f000001/",
            "http://0177.0.0.1/",
            "http://127.1/",
            "http://127.255.255.254/",
            "http://0.0.0.0:7117/",
            "http://0/",
            "http://[::1]:7117/",
            "http://[0:0:0:0:0:0:0:1]/",
            "http://[::]/",
            "http://[::ffff:127.0.0.1]/",
            "http://[::ffff:7f00:1]/",
            "http://[::127.0.0.1]/",
        ] {
            assert!(!may(url), "{url} should be refused");
        }
    }

    #[test]
    fn private_and_link_local_addresses_are_refused() {
        for url in [
            "http://10.0.0.5/",
            "http://172.16.0.1/",
            "http://172.31.255.255/",
            "http://192.168.1.20:8000/",
            "http://3232235796:8000/",
            "http://169.254.169.254/",
            "http://100.64.0.1/",
            "http://100.127.255.255/",
            "http://255.255.255.255/",
            "http://[fd12:3456::1]/",
            "http://[fe80::1]/",
            "http://[fec0::1]/",
            "http://[::ffff:192.168.1.1]/",
            "http://[::ffff:a00:5]/",
        ] {
            assert!(!may(url), "{url} should be refused");
        }
    }

    #[test]
    fn addresses_just_outside_the_private_blocks_load() {
        for url in ["http://172.32.0.1/", "http://100.128.0.1/", "http://192.169.0.1/", "http://11.0.0.1/", "http://[2001:db8::1]/"] {
            assert!(may(url), "{url} should load");
        }
    }

    #[test]
    fn local_network_names_are_refused() {
        for url in [
            "http://virtues.local:8000/",
            "http://Virtues-Box.LOCAL./",
            "http://box:8000/",
            "http://box.lan/",
            "http://nas.home.arpa/",
            "http://printer.internal/",
            "http://box.virtues:8000/",
            "http://myapp.test/",
            "http://mac.localdomain/",
        ] {
            assert!(!may(url), "{url} should be refused");
        }
    }

    #[test]
    fn the_app_schemes_are_refused() {
        for url in ["virtues://localhost/settings", "tauri://localhost/", "file:///etc/hosts", "javascript:alert(1)"] {
            assert!(!may(url), "{url} should be refused");
        }
    }

    #[test]
    fn documents_a_page_makes_itself_load() {
        for url in ["about:blank", "about:srcdoc", "data:text/html,hi", "blob:https://example.com/0f3e"] {
            assert!(may(url), "{url} should load");
        }
    }

    #[test]
    fn opening_a_local_address_says_why() {
        assert_eq!(parse_page("http://127.0.0.1:7117/api/keys/OPENAI_API_KEY/value").unwrap_err(), LOCAL_REFUSED);
        assert_eq!(parse_page("http://[::ffff:127.0.0.1]:7117/").unwrap_err(), LOCAL_REFUSED);
        assert!(parse_page("virtues://localhost/").unwrap_err().contains("http and https"));
        assert_eq!(parse_page("https://example.com/a").unwrap().as_str(), "https://example.com/a");
    }

    #[test]
    fn a_name_that_resolves_to_this_mac_is_refused() {
        // `localhost` answers from /etc/hosts, so this needs no network. Any
        // public name whose DNS answer is 127.0.0.1 takes the same path.
        use tauri::async_runtime::block_on;
        assert!(block_on(resolves_local(&"http://localhost:7117/".parse().unwrap())));
        // An address is checked as written, not looked up.
        assert!(!block_on(resolves_local(&"https://8.8.8.8/".parse().unwrap())));
        assert_eq!(block_on(checked_page("http://[::1]:7117/")).unwrap_err(), LOCAL_REFUSED);
    }

    /// Needs the network: `cargo test --bin virtues-app -- --ignored wildcard`.
    #[test]
    #[ignore]
    fn a_wildcard_dns_name_for_loopback_is_refused() {
        let page = tauri::async_runtime::block_on(checked_page("http://127.0.0.1.nip.io:7117/api/passwords"));
        assert_eq!(page.unwrap_err(), LOCAL_REFUSED);
    }

    #[test]
    fn a_refusal_says_what_was_refused() {
        let local = Refused::of(&"http://127.0.0.1:7117/".parse().unwrap());
        assert_eq!(local, Refused::Local);
        assert!(local.redirect().contains("local address"));
        let mail = Refused::of(&"mailto:nick@example.com".parse().unwrap());
        assert_eq!(mail, Refused::Scheme("mailto".into()));
        assert!(mail.note().contains("`mailto:` link"));
        assert!(!mail.redirect().contains("local address"));
    }

    /// A re-sync of the vendored script from a newer Playwright drops the one
    /// change made to it, and the outline goes back to carrying passwords.
    #[test]
    fn the_snapshot_script_hides_secret_values() {
        assert!(INJECTED.contains("function virtuesSecretField(element)"));
        assert!(INJECTED.contains("result.children = [virtuesFieldValue(element)]"));
        assert!(!INJECTED.contains("result.children = [element.value]"));
        assert!(!INJECTED.contains("compositeString(element.value,"));
    }
}

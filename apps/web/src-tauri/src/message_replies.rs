//! Replies from the record — the Mac's half.
//!
//! The owner asks for a draft; the box writes one; this opens it. There is no
//! watching here. An earlier build polled every five seconds and raised a
//! notification whenever the box had drafted something on its own — both are
//! gone with the thing that produced them. Asking is the whole trigger.
//!
//! Why this app and not the collector: the collector is a bare LaunchAgent
//! binary, and macOS lets only a bundled app hold the Automation grant
//! Messages requires. This app is already the process the collector routes
//! through (`:7117`), so it is already the one that has to be running.
//!
//! Sending is `osascript` telling Messages to send text to a chat id. The
//! chat id is the GUID the collector reads out of chat.db and the box hands
//! back on the row — one-to-one, group, and SMS threads all take the same
//! call. The text travels as an argv item, never interpolated into the
//! script, so quoting cannot break it or be used against it.

use serde::Deserialize;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_shell::ShellExt;

const CONNECT_TIMEOUT: Duration = Duration::from_millis(1500);
const READ_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_BODY: u64 = 4 * 1024 * 1024;
/// How often to look for the draft the owner just asked for, and for how
/// long. This is a wait on ONE request, not a watch: it starts when they
/// press the button and ends when the draft lands or the box gives up.
const AWAIT_EVERY: Duration = Duration::from_secs(2);
const AWAIT_FOR: Duration = Duration::from_secs(120);
/// The applet the box runs to draft a reply. The id is derived from the
/// applet's directory name (`applet_<dir>`).
const REPLY_APPLET_ID: &str = "applet_message_reply";

/// The wire shape of `app_message_replies`, only the fields this side reads
/// (the send itself is driven from the page, which loads the full row).
#[derive(Debug, Clone, Deserialize)]
pub struct PendingReply {
    pub id: String,
    pub thread_id: String,
}

#[derive(Default)]
pub struct ReplyWatch {
    /// A route the shell wants the SPA to open, held until the SPA asks. An
    /// event emitted before the page's listener exists is simply lost, and
    /// the window may be cold when the menu item is clicked.
    queued_route: Option<String>,
    /// True while a draft has been asked for and not yet arrived, so a second
    /// press does not start a second wait.
    awaiting: bool,
}

pub type ReplyWatchState = Mutex<ReplyWatch>;

// ─── Loopback HTTP ───────────────────────────────────────────────────────────
//
// No HTTP client crate in this binary (it cross-compiles to iOS, and a TLS
// stack is the worst thing to drag along). The loopback is the iroh tunnel,
// so a request here arrives at the box as this device — no token to carry.

fn box_addr() -> String {
    format!("127.0.0.1:{}", tauri_plugin_reach::loopback_port())
}

/// One HTTP/1.1 request over the loopback, returning the status and the body.
/// The status matters to the caller: a 404 is a box too old to know these
/// routes, which is a different thing from a box that is refusing or away.
fn loopback_request(method: &str, path: &str, json: Option<&str>) -> std::io::Result<(u16, Vec<u8>)> {
    let sock = box_addr()
        .parse()
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "bad addr"))?;
    let mut stream = TcpStream::connect_timeout(&sock, CONNECT_TIMEOUT)?;
    stream.set_read_timeout(Some(READ_TIMEOUT))?;

    let mut req = format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n");
    match json {
        Some(body) => {
            req.push_str(&format!(
                "Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            ));
        }
        None => req.push_str("Content-Length: 0\r\n\r\n"),
    }
    stream.write_all(req.as_bytes())?;

    let mut raw = Vec::new();
    stream.take(MAX_BODY).read_to_end(&mut raw)?;

    let head_end = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| std::io::Error::other("no HTTP header terminator"))?;
    let head = String::from_utf8_lossy(&raw[..head_end]);
    let status: u16 = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| std::io::Error::other("no HTTP status"))?;
    Ok((status, raw[head_end + 4..].to_vec()))
}

/// Why a poll did not produce a list. The distinction is the whole point: a
/// box that has never heard of these routes wants an upgrade, not a retry.
enum PollFailure {
    /// 404 — this box predates the feature.
    NoSuchRoute,
    /// Away, restarting, refusing: ordinary while waiting on a draft, and
    /// worth trying again until the deadline.
    Unreachable,
}

/// A non-2xx is a failure here, not "nothing pending": a 401 or 5xx while the
/// box restarts must not clear the seen-set and re-announce everything.
fn fetch_pending() -> Result<Vec<PendingReply>, PollFailure> {
    let (status, body) = loopback_request("GET", "/api/message-replies/pending", None)
        .map_err(|_| PollFailure::Unreachable)?;
    if status == 404 {
        return Err(PollFailure::NoSuchRoute);
    }
    if !(200..300).contains(&status) {
        return Err(PollFailure::Unreachable);
    }
    serde_json::from_slice::<Vec<PendingReply>>(&body)
        .map_err(|_| PollFailure::Unreachable)
}

// ─── Asking for a draft ──────────────────────────────────────────────────────

/// Ask the box to draft a reply, wait for it, and open it.
///
/// Runs on its own thread: the request and the wait both block, and this is
/// called from a menu handler on the main thread. Speaks to the owner only at
/// the ends — when there is a draft, and when there will not be one.
pub fn ask_for_draft(app: &AppHandle, thread_id: Option<String>) {
    {
        let state = app.state::<ReplyWatchState>();
        let mut g = state.lock().unwrap();
        if g.awaiting {
            drop(g);
            notify(app, "Still working on the last one", "");
            return;
        }
        g.awaiting = true;
    }

    let app = app.clone();
    std::thread::spawn(move || {
        let outcome = request_draft(thread_id.as_deref())
            .and_then(|()| await_draft(thread_id.as_deref()));
        app.state::<ReplyWatchState>().lock().unwrap().awaiting = false;

        match outcome {
            Ok(Some(reply)) => open_reply(&app, &reply.id),
            // The drafter ran and had nothing to say. That is an answer, and
            // the owner is owed it — they pressed a button and are waiting.
            Ok(None) => notify(
                &app,
                "Nothing to reply with",
                "Your record doesn't have what that message is asking for.",
            ),
            Err(e) => notify(&app, "Could not draft a reply", &e),
        }
    });
}

/// Poll for the draft this request produced, up to `AWAIT_FOR`.
fn await_draft(thread_id: Option<&str>) -> Result<Option<PendingReply>, String> {
    let deadline = Instant::now() + AWAIT_FOR;
    while Instant::now() < deadline {
        std::thread::sleep(AWAIT_EVERY);
        match fetch_pending() {
            Ok(pending) => {
                let found = pending.into_iter().find(|r| match thread_id {
                    Some(t) => r.thread_id == t,
                    None => true,
                });
                if found.is_some() {
                    return Ok(found);
                }
            }
            // A blip while the box restarts is not a verdict; keep waiting.
            Err(PollFailure::Unreachable) => {}
            Err(PollFailure::NoSuchRoute) => {
                return Err("This server doesn't draft replies yet. Upgrade it first.".into())
            }
        }
    }
    Ok(None)
}

fn notify(app: &AppHandle, title: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;
    let _ = app.notification().builder().title(title).body(body).show();
}

// ─── Opening a draft ─────────────────────────────────────────────────────────

/// Show the window and ask the SPA to open the draft. The route is also
/// queued for `take_pending_route`, which covers a page that is not yet
/// listening when the event goes out.
pub fn open_reply(app: &AppHandle, id: &str) {
    let route = format!("/reply/{id}");
    app.state::<ReplyWatchState>().lock().unwrap().queued_route = Some(route.clone());
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
    let _ = app.emit("virtues://open-route", route);
}

/// Start the drafter on a thread — or, with none, on whichever thread most
/// recently messaged the owner. Returns as soon as the run is queued.
fn request_draft(thread_id: Option<&str>) -> Result<(), String> {
    let payload = match thread_id {
        Some(t) => serde_json::json!({ "payload": { "thread_id": t } }),
        None => serde_json::json!({ "payload": {} }),
    };
    let path = format!("/api/applets/{REPLY_APPLET_ID}/run");
    match loopback_request("POST", &path, Some(&payload.to_string())) {
        Ok((s, _)) if (200..300).contains(&s) => Ok(()),
        Ok((404, _)) => Err("This server doesn't draft replies yet. Upgrade it first.".into()),
        Ok((s, _)) => Err(format!("the box declined to start a draft ({s})")),
        Err(e) => Err(format!("box not reachable: {e}")),
    }
}

// ─── Commands ────────────────────────────────────────────────────────────────

/// The send, as AppleScript. Argv, not interpolation: item 1 is the chat
/// id, item 2 the handle (empty for a group), item 3 the text.
///
/// **Messages must be running before the first attempt.** A `tell` launches
/// it, but the app is not ready to resolve a chat id for a second or so after
/// that, and every lookup in the window throws "Can't get chat id". Measured
/// on a cold Messages: the id path failed outright and only the participant
/// fallback delivered — which a GROUP thread does not have, so the first
/// group reply after a reboot would simply fail. Hence launch, then retry the
/// id for a few seconds before falling back.
///
/// The fallback is the participant, looked up on the iMessage account and
/// then the SMS one — the recipe that predates chat ids. It only applies to a
/// one-to-one thread, where the caller passes a handle.
///
/// chat.db's GUIDs are the `any;-;…` form on modern macOS, and Messages
/// accepts exactly that; rewriting the prefix to `iMessage;`/`SMS;` produces
/// an id that resolves to nothing, so this passes the id through untouched.
const SEND_SCRIPT: &str = r#"
on run argv
    set theId to item 1 of argv
    set theHandle to item 2 of argv
    set theText to item 3 of argv
    tell application "Messages"
        launch
        repeat 8 times
            try
                send theText to chat id theId
                return "sent via chat id"
            end try
            delay 0.5
        end repeat
        if theHandle is not "" then
            repeat with svc in {iMessage, SMS}
                try
                    set acct to first account whose service type = svc
                    send theText to participant theHandle of acct
                    return "sent via participant"
                end try
            end repeat
        end if
        -- Out of options: let this one's error be the one reported, so a real
        -- refusal (Automation denied) still reads as one.
        send theText to chat id theId
    end tell
end run
"#;

/// Send text into a Messages thread. The first use prompts for Automation
/// access to Messages; a refusal comes back here as the error.
#[tauri::command]
pub async fn send_imessage(
    app: AppHandle,
    thread_id: String,
    handle: Option<String>,
    text: String,
) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("nothing to send".into());
    }
    if thread_id.trim().is_empty() {
        return Err("no thread to send to".into());
    }
    let handle = handle.unwrap_or_default();
    // `--` ends option parsing so a message starting with "-" is text.
    let output = app
        .shell()
        .command("osascript")
        .args([
            "-e",
            SEND_SCRIPT,
            "--",
            thread_id.trim(),
            handle.trim(),
            text.as_str(),
        ])
        .output()
        .await
        .map_err(|e| format!("could not run osascript: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if err.is_empty() {
        format!("Messages refused (exit {:?})", output.status.code())
    } else if err.contains("-1743") || err.contains("not allowed") {
        "Virtues isn't allowed to control Messages. Allow it in System Settings → Privacy & Security → Automation.".into()
    } else {
        err
    })
}

/// The zero-permission path: open Messages to the thread with the text in the
/// compose field, and the owner presses return. Works for a handle, not a
/// group — groups have no address to put in the URL.
#[tauri::command]
pub async fn open_messages_thread(app: AppHandle, handle: String, body: String) -> Result<(), String> {
    let handle = handle.trim();
    if handle.is_empty() {
        return Err("this thread has no address to open".into());
    }
    let url = format!("sms:{}&body={}", handle, urlencode(body.trim()));
    app.shell()
        .command("open")
        .args([url.as_str()])
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// The route the shell queued for the page to open, if any — taken once.
#[tauri::command]
pub fn take_pending_route(app: AppHandle) -> Option<String> {
    app.state::<ReplyWatchState>().lock().unwrap().queued_route.take()
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urlencode_keeps_unreserved_and_escapes_the_rest() {
        assert_eq!(urlencode("Sure, it's 12 Example Lane"), "Sure%2C%20it%27s%2012%20Example%20Lane");
        assert_eq!(urlencode("a-b_c.d~e"), "a-b_c.d~e");
    }

}

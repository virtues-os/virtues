//! Replies from the record — the Mac's half.
//!
//! The box drafts a reply to a message thread and parks it in
//! `app_message_replies`. This module is the hand: it polls the box for
//! pending drafts, tells the owner one is waiting (a notification and a tray
//! line), opens the draft in the window, and sends through Messages when the
//! owner says so. It never decides anything; every send is a click.
//!
//! Why this app and not the collector: the collector is a bare LaunchAgent
//! binary, and macOS lets only a bundled app post user notifications or hold
//! the Automation grant Messages requires. This app is already the process the
//! collector routes through (`:7117`), so it is already the one that has to
//! be running.
//!
//! Sending is `osascript` telling Messages to send text to a chat id. The
//! chat id is the GUID the collector reads out of chat.db and the box hands
//! back on the row — one-to-one, group, and SMS threads all take the same
//! call. The text travels as an argv item, never interpolated into the
//! script, so quoting cannot break it or be used against it.

use serde::Deserialize;
use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_shell::ShellExt;

const POLL_EVERY: Duration = Duration::from_secs(5);
const CONNECT_TIMEOUT: Duration = Duration::from_millis(1500);
const READ_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_BODY: u64 = 4 * 1024 * 1024;
/// A notification click activates the app; if the owner then reopens the
/// window within this long, it is safe to assume they came for the draft.
const NOTIFIED_RECENTLY: Duration = Duration::from_secs(90);
/// The applet the box runs for "take care of the latest thread". The id is
/// derived from the applet's directory name (`applet_<dir>`).
const REPLY_APPLET_ID: &str = "applet_message_reply";

/// The wire shape of `app_message_replies`, only the fields this side reads
/// (the send itself is driven from the page, which loads the full row).
#[derive(Debug, Clone, Deserialize)]
pub struct PendingReply {
    pub id: String,
    pub ask_text: String,
    pub from_handle: String,
    pub from_name: Option<String>,
    pub draft: String,
}

impl PendingReply {
    pub fn who(&self) -> &str {
        self.from_name.as_deref().filter(|n| !n.is_empty()).unwrap_or(&self.from_handle)
    }
}

#[derive(Default)]
pub struct ReplyWatch {
    /// Whether a poll has succeeded since launch. The first one seeds `seen`
    /// without notifying: rows up to a day old may be pending, and a burst of
    /// banners at login is noise — the tray line names the newest anyway.
    primed: bool,
    /// Ids already announced, so a row is notified once and not every 5s.
    seen: HashSet<String>,
    /// What is pending as of the last poll, newest first.
    pending: Vec<PendingReply>,
    /// The last draft announced, and when — see `NOTIFIED_RECENTLY`.
    last_notified: Option<(String, Instant)>,
    /// A route the shell wants the SPA to open, held until the SPA asks. An
    /// event emitted before the page's listener exists is simply lost, and
    /// the window may be cold when the tray item is clicked.
    queued_route: Option<String>,
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

/// One HTTP/1.1 request over the loopback. `Some(body)` on 2xx, `None` on any
/// other status; a transport failure is the error.
fn loopback_request(method: &str, path: &str, json: Option<&str>) -> std::io::Result<Option<Vec<u8>>> {
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

    let head_end = match raw.windows(4).position(|w| w == b"\r\n\r\n") {
        Some(i) => i,
        None => return Ok(None),
    };
    let head = String::from_utf8_lossy(&raw[..head_end]);
    let status = head.split_whitespace().nth(1).unwrap_or("");
    if !status.starts_with('2') {
        return Ok(None);
    }
    Ok(Some(raw[head_end + 4..].to_vec()))
}

/// A non-2xx is an error here, not "nothing pending": a 401 or 5xx while the
/// box restarts must not clear the seen-set and re-announce everything.
fn fetch_pending() -> std::io::Result<Vec<PendingReply>> {
    let Some(body) = loopback_request("GET", "/api/message-replies/pending", None)? else {
        return Err(std::io::Error::other("box answered non-2xx"));
    };
    serde_json::from_slice::<Vec<PendingReply>>(&body)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
}

// ─── The poll ────────────────────────────────────────────────────────────────

/// Watch the box for pending drafts. One thread for the life of the app;
/// a poll, because the box has no push channel to this app.
pub fn start_poll(app: AppHandle, tray_line: tauri::menu::MenuItem<tauri::Wry>) {
    std::thread::spawn(move || {
        // Off the launch path: the loopback is not serving for the first
        // seconds, and there is nothing to announce that cannot wait.
        std::thread::sleep(Duration::from_secs(8));
        loop {
            match fetch_pending() {
                Ok(pending) => announce(&app, &tray_line, pending),
                Err(e) => {
                    // The box being away is ordinary (laptop lid, box
                    // rebooting). Say nothing; the tray status line already
                    // reports reachability.
                    eprintln!("[replies] box not reachable: {e}");
                }
            }
            std::thread::sleep(POLL_EVERY);
        }
    });
}

fn announce(app: &AppHandle, tray_line: &tauri::menu::MenuItem<tauri::Wry>, pending: Vec<PendingReply>) {
    use tauri_plugin_notification::NotificationExt;

    let state = app.state::<ReplyWatchState>();
    let mut fresh: Vec<PendingReply> = Vec::new();
    {
        let mut g = state.lock().unwrap();
        let announce_new = g.primed;
        g.primed = true;
        for r in &pending {
            if g.seen.insert(r.id.clone()) && announce_new {
                fresh.push(r.clone());
            }
        }
        // Forget ids the box no longer lists, so a row that is re-drafted
        // later (manual re-request after a dismissal) is announced again.
        let live: HashSet<&str> = pending.iter().map(|r| r.id.as_str()).collect();
        g.seen.retain(|id| live.contains(id.as_str()));
        g.pending = pending.clone();
    }

    // One notification per new draft. The body is the draft itself so the
    // owner can judge it from the banner; the window is one click away.
    for r in &fresh {
        let title = format!("{} asked: {}", r.who(), cap(&r.ask_text, 60));
        let _ = app
            .notification()
            .builder()
            .title(title)
            .body(cap(&r.draft, 200))
            .show();
        state.lock().unwrap().last_notified = Some((r.id.clone(), Instant::now()));
    }

    // The tray line names the newest pending draft, or says there is none.
    let (text, enabled) = match pending.first() {
        Some(r) => (format!("Reply to {}: {}", r.who(), cap(&r.ask_text, 40)), true),
        None => ("No replies waiting".to_string(), false),
    };
    let line = tray_line.clone();
    let _ = app.run_on_main_thread(move || {
        let _ = line.set_text(text);
        let _ = line.set_enabled(enabled);
    });
}

fn cap(s: &str, n: usize) -> String {
    let s = s.trim().replace('\n', " ");
    if s.chars().count() <= n {
        return s;
    }
    let cut: String = s.chars().take(n).collect();
    format!("{cut}…")
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

/// The newest pending draft's id, if any.
pub fn newest_pending(app: &AppHandle) -> Option<String> {
    app.state::<ReplyWatchState>()
        .lock()
        .unwrap()
        .pending
        .first()
        .map(|r| r.id.clone())
}

/// After a notification click: the app was activated with no window, and a
/// draft was announced moments ago. Open it.
pub fn open_if_notified_recently(app: &AppHandle) {
    let recent = {
        let state = app.state::<ReplyWatchState>();
        let g = state.lock().unwrap();
        g.last_notified
            .as_ref()
            .filter(|(_, at)| at.elapsed() < NOTIFIED_RECENTLY)
            .filter(|(id, _)| g.pending.iter().any(|r| &r.id == id))
            .map(|(id, _)| id.clone())
    };
    if let Some(id) = recent {
        open_reply(app, &id);
    }
}

/// Ask the box to draft for the thread that most recently messaged the owner
/// (or a named one). Returns at once; the poll announces the result.
pub fn request_draft(thread_id: Option<&str>) -> Result<(), String> {
    let payload = match thread_id {
        Some(t) => serde_json::json!({ "payload": { "thread_id": t } }),
        None => serde_json::json!({ "payload": {} }),
    };
    let path = format!("/api/applets/{REPLY_APPLET_ID}/run");
    match loopback_request("POST", &path, Some(&payload.to_string())) {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err("the box declined to start a draft".into()),
        Err(e) => Err(format!("box not reachable: {e}")),
    }
}

// ─── Commands ────────────────────────────────────────────────────────────────

/// The send, as AppleScript. Argv, not interpolation: item 1 is the chat
/// id, item 2 the handle (empty for a group), item 3 the text.
///
/// The chain exists because chat.db's GUIDs and Messages' scripting ids do
/// not always agree. Modern macOS stores merged threads as `any;-;+1…`,
/// which the dictionary may refuse; the same thread answers to `iMessage;-;`
/// or `SMS;-;`. For a one-to-one thread the last resort is the participant
/// itself, looked up on the iMessage account and then the SMS one — the
/// well-worn recipe that predates chat ids. The final attempt's error is the
/// one reported, so a real refusal (Automation denied) still reads as one.
const SEND_SCRIPT: &str = r#"
on run argv
    set theId to item 1 of argv
    set theHandle to item 2 of argv
    set theText to item 3 of argv
    set candidates to {theId}
    if theId starts with "any;" then
        set candidates to candidates & {"iMessage;" & (text 5 thru -1 of theId), "SMS;" & (text 5 thru -1 of theId)}
    end if
    tell application "Messages"
        repeat with c in candidates
            try
                send theText to chat id (c as text)
                return "sent via chat id"
            end try
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

    #[test]
    fn cap_trims_and_flattens() {
        assert_eq!(cap("  hi\nthere  ", 10), "hi there");
        assert_eq!(cap("abcdefghijk", 5), "abcde…");
    }
}

//! Sending an iMessage for the assistant on the box (virtues-core
//! `tools/send_imessage.rs`).
//!
//! The box works out the thread and the words; this side only hands them to
//! Messages. The request arrives on the browser's socket (`browser_host.rs`),
//! beside its queue, so a browser handoff waiting on the owner never holds a
//! message back.
//!
//! Sending is `osascript` telling Messages to send text to a chat id. The text
//! travels as an argv item, never interpolated into the script, so no quoting
//! can break it or be turned against it. The first send asks the owner for
//! Automation access to Messages.

use serde_json::{json, Value};
use tauri::AppHandle;
use tauri_plugin_shell::ShellExt;

/// The send, as AppleScript. Item 1 is the chat id (empty when the owner has
/// no thread with them yet), item 2 the handle, item 3 the text.
///
/// **Messages must be running before the first attempt.** A `tell` launches
/// it, but for about a second afterwards it cannot resolve any chat id, and
/// every lookup throws. So launch, retry the id for a few seconds, and only
/// then fall back to the handle, on the iMessage account and then the SMS one.
///
/// chat.db writes its GUIDs as `any;-;…`, and Messages accepts exactly that.
/// Rewriting the prefix to `iMessage;`/`SMS;` makes an id that resolves to
/// nothing, so the id goes through untouched.
const SEND_SCRIPT: &str = r#"
on run argv
    set theId to item 1 of argv
    set theHandle to item 2 of argv
    set theText to item 3 of argv
    tell application "Messages"
        launch
        if theId is not "" then
            repeat 8 times
                try
                    send theText to chat id theId
                    return "chat"
                end try
                delay 0.5
            end repeat
        end if
        if theHandle is not "" then
            repeat with svc in {iMessage, SMS}
                try
                    set acct to first account whose service type = svc
                    send theText to participant theHandle of acct
                    return "handle"
                end try
            end repeat
        end if
        -- Out of options: let this error be the one reported, so a real
        -- refusal (Automation denied) still reads as one.
        if theId is not "" then
            send theText to chat id theId
        else
            send theText to participant theHandle of (first account whose service type = iMessage)
        end if
    end tell
end run
"#;

/// `{chat_id, handle, text}` in; `{via}` out, `chat` or `handle`.
pub async fn send(app: &AppHandle, args: &Value) -> Result<Value, String> {
    let field = |k: &str| args.get(k).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let (chat_id, handle, text) = (field("chat_id"), field("handle"), field("text"));
    if text.is_empty() {
        return Err("There is nothing to send.".into());
    }
    if chat_id.is_empty() && handle.is_empty() {
        return Err("There is no thread or address to send to.".into());
    }
    // `--` ends option parsing, so a message starting with "-" is text.
    let output = app
        .shell()
        .command("osascript")
        .args(["-e", SEND_SCRIPT, "--", chat_id.as_str(), handle.as_str(), text.as_str()])
        .output()
        .await
        .map_err(|e| format!("Could not run osascript: {e}"))?;
    if output.status.success() {
        let via = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Ok(json!({ "via": via }));
    }
    let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if err.contains("-1743") || err.contains("not allowed") {
        "Your Mac blocked Virtues from using Messages. To fix it, turn on Messages under Virtues \
         in System Settings > Privacy & Security > Automation."
            .into()
    } else if err.is_empty() {
        format!("Messages refused the message (exit {:?}).", output.status.code())
    } else {
        format!("Messages refused the message: {err}")
    })
}

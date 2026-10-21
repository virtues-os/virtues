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
/// no thread with them yet), item 2 the handle, item 3 the text or a file's
/// path, item 4 `text` or `file`.
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
    if item 4 of argv is "file" then set theText to POSIX file theText
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

/// Where files wait for Messages to pick them up. Messages copies a file into
/// its own store, so the app's temporary folder works, but it copies after
/// `send` has returned: a file deleted at once fails to send (chat.db error
/// 25). So each send's folder stays, and folders older than this go at the
/// start of a later send.
fn staging_dir() -> std::path::PathBuf {
    std::env::temp_dir().join("virtues-outgoing")
}

const STAGED_FOR: std::time::Duration = std::time::Duration::from_secs(60 * 60);

fn sweep_staging() {
    let Ok(entries) = std::fs::read_dir(staging_dir()) else { return };
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > STAGED_FOR);
        if old {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// `{chat_id, handle, text, files: [{filename, data_base64}]}` in; `{via}`
/// out, `chat` or `handle`. The text goes first, then each file, each its own
/// message in the thread.
pub async fn send(app: &AppHandle, args: &Value) -> Result<Value, String> {
    let field = |k: &str| args.get(k).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let (chat_id, handle, text) = (field("chat_id"), field("handle"), field("text"));
    let files = args.get("files").and_then(|f| f.as_array()).cloned().unwrap_or_default();
    if text.is_empty() && files.is_empty() {
        return Err("There is nothing to send.".into());
    }
    if chat_id.is_empty() && handle.is_empty() {
        return Err("There is no thread or address to send to.".into());
    }

    let mut paths = Vec::new();
    if !files.is_empty() {
        sweep_staging();
        use base64::Engine;
        let dir = staging_dir().join(unique_name());
        std::fs::create_dir_all(&dir).map_err(|e| format!("Could not stage the file: {e}"))?;
        for f in &files {
            let name = f.get("filename").and_then(|v| v.as_str()).unwrap_or("file");
            // A name from the box, used as a path: keep only its last part.
            let name = std::path::Path::new(name).file_name().and_then(|n| n.to_str()).unwrap_or("file");
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(f.get("data_base64").and_then(|v| v.as_str()).unwrap_or(""))
                .map_err(|_| "The file arrived unreadable.".to_string())?;
            let path = dir.join(name);
            std::fs::write(&path, bytes).map_err(|e| format!("Could not stage the file: {e}"))?;
            paths.push(path);
        }
    }

    let via = send_all(app, &chat_id, &handle, &text, &paths).await?;
    Ok(json!({ "via": via }))
}

async fn send_all(
    app: &AppHandle,
    chat_id: &str,
    handle: &str,
    text: &str,
    paths: &[std::path::PathBuf],
) -> Result<String, String> {
    let mut via = String::new();
    if !text.is_empty() {
        via = run(app, chat_id, handle, text, "text").await?;
    }
    for path in paths {
        via = run(app, chat_id, handle, &path.to_string_lossy(), "file").await?;
    }
    Ok(via)
}

fn unique_name() -> String {
    format!("{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0))
}

async fn run(app: &AppHandle, chat_id: &str, handle: &str, payload: &str, kind: &str) -> Result<String, String> {
    // `--` ends option parsing, so a message starting with "-" is text.
    let output = app
        .shell()
        .command("osascript")
        .args(["-e", SEND_SCRIPT, "--", chat_id, handle, payload, kind])
        .output()
        .await
        .map_err(|e| format!("Could not run osascript: {e}"))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
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

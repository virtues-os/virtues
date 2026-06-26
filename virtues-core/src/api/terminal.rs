use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};

use crate::server::webhook::AppState;

/// Bound on the PTY→websocket output queue. When the shell produces output
/// faster than a slow client can drain it, the reader thread blocks on send,
/// the PTY's kernel buffer fills, and the process is flow-controlled — the
/// same backpressure a real terminal applies. Without a bound, a chatty
/// process (`yes`, `cat bigfile`) would grow memory without limit.
const OUTPUT_QUEUE: usize = 256;

// ---------------------------------------------------------------------------
// WebSocket <-> in-process PTY bridge
//
// The terminal tab in the web UI opens a real login shell on *this* machine.
// We spawn a PTY directly (no sshd, no localhost SSH round-trip): the shell
// runs as whatever user the server process runs as — `virtues` on a box, the
// developer in local dev. Same code path everywhere.
//
// portable-pty is thread-based (blocking reader/writer), so we bridge to the
// async websocket through channels: a reader thread pumps PTY output into a
// bounded mpsc the select loop drains, and a writer thread drains input the
// loop feeds it. The select loop keeps the master so it can resize, and kills
// the child on disconnect so no shells leak.
// ---------------------------------------------------------------------------

/// Handler for the terminal WebSocket.
pub async fn terminal_ws_handler(
    ws: WebSocketUpgrade,
    State(_state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    // Defense against Cross-Site WebSocket Hijacking: the session cookie alone
    // would let any page open `ws://<box>/ws/terminal` and ride it to a shell.
    // The web app always connects same-origin (location.host), so a foreign
    // Origin means a foreign page — reject the upgrade before it happens.
    if let Some(rejection) = check_same_origin(&headers) {
        return rejection;
    }
    ws.on_upgrade(handle_socket).into_response()
}

/// Returns `Some(rejection)` if the browser's `Origin` doesn't match the
/// request `Host`. A missing `Origin` (non-browser client) is allowed — auth
/// is still enforced by the route layer, and the CSWSH vector is browser-only:
/// browsers always send `Origin` on a WebSocket handshake.
fn check_same_origin(headers: &HeaderMap) -> Option<Response> {
    let origin = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok())?;
    // Compare authority (host[:port]); strip the scheme from the Origin.
    let origin_authority = origin.split_once("://").map(|(_, a)| a).unwrap_or(origin);
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok());
    if host == Some(origin_authority) {
        None
    } else {
        tracing::warn!(
            "Terminal WS rejected: cross-origin (origin={:?}, host={:?})",
            origin,
            host
        );
        Some((StatusCode::FORBIDDEN, "cross-origin websocket rejected").into_response())
    }
}

/// Handle the established WebSocket connection.
async fn handle_socket(mut socket: WebSocket) {
    if let Err(e) = pty_bridge(&mut socket).await {
        tracing::error!("Terminal PTY bridge error: {}", e);
        let _ = socket
            .send(Message::Text(format!(
                "\r\n\x1b[31mTerminal error: {}\x1b[0m\r\n",
                e
            )))
            .await;
    }
}

async fn pty_bridge(socket: &mut WebSocket) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Default size; the frontend sends a resize immediately after connecting.
    let pty = native_pty_system().openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    // Spawn the user's login shell so they get their normal environment.
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
    let mut cmd = CommandBuilder::new(&shell);
    cmd.arg("-l");
    cmd.env("TERM", "xterm-256color");
    if let Ok(home) = std::env::var("HOME") {
        // CLIs the user installs into their home (the Claude Code installer →
        // ~/.local/bin, `npm -g`, cargo, our own helpers → ~/.virtues/bin) must
        // survive across sessions. The shell is `-l`, but on the appliance the
        // `virtues` HOME has no profile to source, so each session would
        // otherwise start with the bare service PATH and lose whatever was
        // installed last session — the binary persists on disk, but nothing
        // points PATH at it. Prepend the conventional per-user bin dirs so a
        // freshly-installed tool is on PATH regardless of dotfiles.
        let mut path = [".local/bin", ".virtues/bin", "bin", ".npm-global/bin", ".cargo/bin"]
            .iter()
            .map(|d| format!("{home}/{d}"))
            .collect::<Vec<_>>()
            .join(":");
        if let Ok(existing) = std::env::var("PATH") {
            path.push(':');
            path.push_str(&existing);
        }
        cmd.env("PATH", path);
        cmd.cwd(home);
    }

    let mut child = pty.slave.spawn_command(cmd)?;
    // Close our handle to the slave; only the child should hold it, so the
    // master sees EOF when the shell exits.
    drop(pty.slave);

    let mut reader = pty.master.try_clone_reader()?;
    let mut writer = pty.master.take_writer()?;
    let master = pty.master;
    let mut killer = child.clone_killer();

    // PTY output -> async loop (blocking reads on a dedicated thread).
    // Bounded channel: `blocking_send` parks the thread when the consumer is
    // behind, which backpressures the shell through the kernel PTY buffer.
    let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(OUTPUT_QUEUE);
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if out_tx.blocking_send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    // async loop -> PTY input (blocking writes on a dedicated thread).
    let (in_tx, in_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        while let Ok(data) = in_rx.recv() {
            if writer.write_all(&data).is_err() {
                break;
            }
            let _ = writer.flush();
        }
    });

    // Child exit -> async loop (blocking wait on a dedicated thread).
    let (exit_tx, exit_rx) = tokio::sync::oneshot::channel::<u32>();
    std::thread::spawn(move || {
        let code = child.wait().map(|s| s.exit_code()).unwrap_or(1);
        let _ = exit_tx.send(code);
    });

    // The loop breaks on either the client disconnecting or the PTY closing.
    // We do NOT break on the child-exit signal: that can fire before the reader
    // thread has drained the shell's final bytes. Reader-EOF (`out_rx` yields
    // `None`) is the authoritative "all output flushed" signal, so we key off
    // it and only then report the exit code.
    let mut shell_closed = false;
    loop {
        tokio::select! {
            // WebSocket -> PTY: user input and resize events.
            ws_msg = socket.recv() => {
                match ws_msg {
                    Some(Ok(Message::Text(t))) => {
                        if let Ok(cmd) = serde_json::from_str::<serde_json::Value>(&t) {
                            let msg_type = cmd.get("type").and_then(|v| v.as_str()).unwrap_or("");
                            match msg_type {
                                "input" => {
                                    if let Some(data) = cmd.get("data").and_then(|v| v.as_str()) {
                                        if in_tx.send(data.as_bytes().to_vec()).is_err() {
                                            break;
                                        }
                                    }
                                }
                                "resize" => {
                                    let cols = cmd.get("cols").and_then(|v| v.as_u64()).unwrap_or(80) as u16;
                                    let rows = cmd.get("rows").and_then(|v| v.as_u64()).unwrap_or(24) as u16;
                                    let _ = master.resize(PtySize {
                                        rows,
                                        cols,
                                        pixel_width: 0,
                                        pixel_height: 0,
                                    });
                                }
                                _ => {}
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(e)) => {
                        tracing::debug!("Terminal WebSocket error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }

            // PTY -> WebSocket: shell output. Sent as Binary so xterm.js decodes
            // UTF-8 itself — a lossy decode here would mangle any multibyte glyph
            // straddling a read boundary.
            out = out_rx.recv() => {
                match out {
                    Some(bytes) => {
                        if socket.send(Message::Binary(bytes)).await.is_err() {
                            break;
                        }
                    }
                    None => {
                        shell_closed = true; // PTY closed and fully drained
                        break;
                    }
                }
            }
        }
    }

    // If the shell ended on its own (vs. the client disconnecting), all its
    // output is now drained, so report the exit code.
    if shell_closed {
        if let Ok(code) = exit_rx.await {
            let _ = socket
                .send(Message::Text(format!(
                    "\r\n\x1b[90m[process exited with code {}]\x1b[0m\r\n",
                    code
                )))
                .await;
        }
    }

    // Kill the shell on disconnect so it doesn't outlive the websocket
    // (no-op if it already exited). Dropping `master` next sends SIGHUP to the
    // foreground process group, cleaning up children too.
    let _ = killer.kill();
    Ok(())
}

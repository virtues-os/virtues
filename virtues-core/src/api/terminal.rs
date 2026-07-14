use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::Deserialize;
use std::io::{Read, Write};
use std::path::PathBuf;

use crate::server::webhook::AppState;

/// Bound on the PTY→websocket output queue. When the shell produces output
/// faster than a slow client can drain it, the reader thread blocks on send,
/// the PTY's kernel buffer fills, and the process is flow-controlled — the
/// same backpressure a real terminal applies. Without a bound, a chatty
/// process (`yes`, `cat bigfile`) would grow memory without limit.
const OUTPUT_QUEUE: usize = 256;

/// The tmux session every terminal tab attaches to. One shared session is the
/// point: two tabs see the same screen.
const TMUX_SESSION: &str = "virtues";

/// Our tmux server runs on its own socket rather than the user's default one.
/// Two reasons: the global options we set below would otherwise be applied to
/// whatever tmux server the user already has (in local dev that's the
/// developer's personal one, and clobbering their `default-terminal` is a nasty
/// surprise), and a `virtues` session appearing in their `tmux ls` is noise.
/// To reach this session from a shell: `tmux -L virtues attach`.
const TMUX_SOCKET: &str = "virtues";

// ---------------------------------------------------------------------------
// WebSocket <-> in-process PTY bridge
//
// The terminal tab in the web UI opens a real login shell on *this* machine.
// We spawn a PTY directly (no sshd, no localhost SSH round-trip): the shell
// runs as whatever user the server process runs as — `virtues` on a box, the
// developer in local dev. Same code path everywhere.
//
// The PTY runs a tmux *client* attached to a long-lived session, not the shell
// itself. The shell (and anything long-running in it — `claude`, a build, a
// migration) is a child of the tmux *server*, which outlives this websocket.
// So a closed tab, a slept laptop, or a dropped iroh path detaches instead of
// killing the work, and the next connection reattaches to the same screen. If
// tmux isn't installed we fall back to a plain login shell, which behaves as
// before: disconnect kills it.
//
// portable-pty is thread-based (blocking reader/writer), so we bridge to the
// async websocket through channels: a reader thread pumps PTY output into a
// bounded mpsc the select loop drains, and a writer thread drains input the
// loop feeds it. The select loop keeps the master so it can resize, and kills
// the child on disconnect so no clients leak.
// ---------------------------------------------------------------------------

/// Terminal size, sent by the client on connect. The PTY has to be born at the
/// right size: a full-screen TUI reads the winsize at startup and paints once,
/// so a PTY that opens at 80x24 and resizes a beat later gets a garbled first
/// frame that only a manual redraw clears.
#[derive(Debug, Deserialize)]
pub struct TerminalParams {
    cols: Option<u16>,
    rows: Option<u16>,
}

/// Clamp a client-supplied dimension: it reaches an ioctl, and 0 rows would
/// make curses apps divide by zero.
fn clamp_dim(v: Option<u16>, default: u16) -> u16 {
    v.unwrap_or(default).clamp(2, 1000)
}

/// Handler for the terminal WebSocket.
pub async fn terminal_ws_handler(
    ws: WebSocketUpgrade,
    State(_state): State<AppState>,
    Query(params): Query<TerminalParams>,
    headers: HeaderMap,
) -> Response {
    // Defense against Cross-Site WebSocket Hijacking: the session cookie alone
    // would let any page open `ws://<box>/ws/terminal` and ride it to a shell.
    // The web app always connects same-origin (location.host), so a foreign
    // Origin means a foreign page — reject the upgrade before it happens.
    if let Some(rejection) = check_same_origin(&headers) {
        return rejection;
    }
    let size = PtySize {
        cols: clamp_dim(params.cols, 80),
        rows: clamp_dim(params.rows, 24),
        pixel_width: 0,
        pixel_height: 0,
    };
    ws.on_upgrade(move |socket| handle_socket(socket, size))
        .into_response()
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
async fn handle_socket(mut socket: WebSocket, size: PtySize) {
    if let Err(e) = pty_bridge(&mut socket, size).await {
        tracing::error!("Terminal PTY bridge error: {}", e);
        let _ = socket
            .send(Message::Text(format!(
                "\r\n\x1b[31mTerminal error: {}\x1b[0m\r\n",
                e
            )))
            .await;
    }
}

/// The PATH terminal sessions run with: the conventional per-user bin dirs
/// ahead of the service PATH.
///
/// CLIs the user installs into their home (the Claude Code installer →
/// ~/.local/bin, `npm -g`, cargo, our own helpers → ~/.virtues/bin) must
/// survive across sessions. The shell is a login shell, but on the appliance
/// the `virtues` HOME has no profile to source, so each session would otherwise
/// start with the bare service PATH and lose whatever was installed last
/// session — the binary persists on disk, but nothing points PATH at it.
fn session_path(home: Option<&str>) -> String {
    let service_path = std::env::var("PATH").unwrap_or_default();
    let Some(home) = home else {
        return service_path;
    };
    let mut path = [
        ".local/bin",
        ".virtues/bin",
        "bin",
        ".npm-global/bin",
        ".cargo/bin",
    ]
    .iter()
    .map(|d| format!("{home}/{d}"))
    .collect::<Vec<_>>()
    .join(":");
    if !service_path.is_empty() {
        path.push(':');
        path.push_str(&service_path);
    }
    path
}

/// Find `tmux` on the session PATH. We can't shell out to `which` (there's no
/// shell in this path) and we don't take a dependency for one lookup.
fn find_tmux(path: &str) -> Option<PathBuf> {
    path.split(':')
        .filter(|dir| !dir.is_empty())
        .map(|dir| PathBuf::from(dir).join("tmux"))
        .find(|candidate| candidate.is_file())
}

/// Build the command the PTY runs: a tmux client attached to the shared
/// session, or a plain login shell if tmux isn't installed.
fn session_command(path: &str, home: Option<&str>) -> CommandBuilder {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());

    let mut cmd = match find_tmux(path) {
        Some(tmux) => {
            let mut cmd = CommandBuilder::new(tmux);
            // `new-session -A` is attach-or-create: the first tab starts the
            // session, every later one reattaches to it.
            //
            // The two `set-option`s run before it (tmux executes a `;`-separated
            // command list in order, starting the server on demand) and exist
            // because tmux otherwise advertises a colour-poor terminal to the
            // programs inside it: `default-terminal` is what they see in $TERM,
            // and `Tc` is what tells tmux the outer terminal — xterm.js — takes
            // 24-bit colour, which it does. Neither is inferable: verified on
            // tmux 3.4 that COLORTERM=truecolor alone does *not* get RGB into
            // the client's terminal features. `screen-256color` over
            // `tmux-256color` because the latter's terminfo entry is missing on
            // minimal images.
            //
            // Both are plain `-g` sets, not `-ga` appends: this command list
            // runs on *every* connect, and appending is not idempotent — three
            // connects leave three copies of `*:Tc` in the option. Overwriting
            // is safe because the server on this socket is ours alone.
            cmd.args([
                "-L",
                TMUX_SOCKET,
                "set-option",
                "-g",
                "default-terminal",
                "screen-256color",
                ";",
                "set-option",
                "-g",
                "terminal-overrides",
                ",*:Tc",
                ";",
                "new-session",
                "-A",
                "-s",
                TMUX_SESSION,
            ]);
            cmd
        }
        None => {
            tracing::warn!(
                "tmux not found on PATH — terminal sessions will not survive disconnects"
            );
            let mut cmd = CommandBuilder::new(&shell);
            cmd.arg("-l");
            cmd
        }
    };

    // What the tmux *client* (or the bare shell) talks to: xterm.js, which is a
    // truecolor xterm.
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("PATH", path);
    // tmux runs the login shell for us; tell it which one.
    cmd.env("SHELL", &shell);
    if let Some(home) = home {
        cmd.cwd(home);
    }
    cmd
}

async fn pty_bridge(
    socket: &mut WebSocket,
    size: PtySize,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Born at the client's real size — see `TerminalParams`.
    let pty = native_pty_system().openpty(size)?;

    let home = std::env::var("HOME").ok();
    let path = session_path(home.as_deref());
    let cmd = session_command(&path, home.as_deref());

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

    // If the session ended on its own (vs. the client disconnecting), all its
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

    // Reap the PTY's child on disconnect so no clients leak (no-op if it already
    // exited). Under tmux that child is the *client*: killing it detaches, and
    // the tmux server keeps the shell and whatever it's running alive for the
    // next connection to reattach to. Without tmux it's the shell itself, and
    // this kills it — the pre-tmux behaviour.
    let _ = killer.kill();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn clamps_client_supplied_dimensions() {
        assert_eq!(clamp_dim(Some(120), 80), 120);
        assert_eq!(clamp_dim(None, 80), 80);
        // 0 rows reaches an ioctl and makes curses apps divide by zero.
        assert_eq!(clamp_dim(Some(0), 24), 2);
        assert_eq!(clamp_dim(Some(60000), 80), 1000);
    }

    #[test]
    fn session_path_puts_user_bins_ahead_of_the_service_path() {
        let path = session_path(Some("/home/virtues"));
        let dirs: Vec<&str> = path.split(':').collect();
        assert_eq!(dirs[0], "/home/virtues/.local/bin");
        assert!(dirs.contains(&"/home/virtues/.virtues/bin"));

        // No HOME: fall back to the service PATH untouched, rather than
        // synthesising nonsense like "/.local/bin" from an empty home.
        assert_eq!(session_path(None), std::env::var("PATH").unwrap_or_default());
    }

    #[test]
    fn finds_tmux_only_when_it_is_there() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_str().unwrap().to_string();
        assert!(find_tmux(&path).is_none());

        std::fs::write(dir.path().join("tmux"), b"#!/bin/sh\n").unwrap();
        assert_eq!(find_tmux(&path), Some(dir.path().join("tmux")));

        // Empty segments in PATH must not resolve to a relative "tmux".
        assert!(find_tmux("::").is_none());
    }

    #[test]
    fn attaches_to_the_shared_tmux_session_when_tmux_exists() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("tmux"), b"#!/bin/sh\n").unwrap();
        let path = dir.path().to_str().unwrap();

        let cmd = session_command(path, Some("/home/virtues"));
        let argv: Vec<String> = cmd
            .get_argv()
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();

        assert!(argv[0].ends_with("/tmux"));
        let tail = argv.join(" ");
        // attach-or-create against the one shared session — this is what makes a
        // closed tab a detach instead of a kill.
        assert!(tail.ends_with(&format!("new-session -A -s {TMUX_SESSION}")));
        // ...on our own socket, so we don't mutate the user's tmux server.
        assert!(tail.starts_with(&format!("{} -L {TMUX_SOCKET} ", argv[0])));
        // ...and tmux must be told the outer terminal takes 24-bit colour.
        assert!(tail.contains("terminal-overrides ,*:Tc"));
        // The option sets must be idempotent: this runs on every connect, and
        // `-ga` would leave a copy of `*:Tc` behind each time.
        assert!(!tail.contains("-ga"));

        assert_eq!(cmd.get_env("COLORTERM").unwrap(), "truecolor");
        assert_eq!(cmd.get_env("TERM").unwrap(), "xterm-256color");
    }

    #[test]
    fn falls_back_to_a_login_shell_without_tmux() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_str().unwrap();

        let cmd = session_command(path, None);
        let argv: Vec<String> = cmd
            .get_argv()
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();

        assert_eq!(argv.len(), 2, "shell + -l, no tmux args: {argv:?}");
        assert_eq!(argv[1], "-l");
        assert_eq!(cmd.get_env("COLORTERM").unwrap(), "truecolor");
    }

    /// The PTY must be born at the client's size: a full-screen TUI reads the
    /// winsize once at startup, so one opened at 80x24 and resized a beat later
    /// paints its first frame into the wrong box.
    #[test]
    fn pty_opens_at_the_size_the_client_asked_for() {
        let size = PtySize {
            cols: 137,
            rows: 42,
            pixel_width: 0,
            pixel_height: 0,
        };
        let pty = native_pty_system().openpty(size).unwrap();

        let mut cmd = CommandBuilder::new("stty");
        cmd.arg("size");
        let mut child = pty.slave.spawn_command(cmd).unwrap();
        drop(pty.slave);

        let mut out = String::new();
        pty.master
            .try_clone_reader()
            .unwrap()
            .read_to_string(&mut out)
            .unwrap();
        child.wait().unwrap();

        // `stty size` prints "rows cols".
        assert_eq!(out.trim(), "42 137");
    }
}

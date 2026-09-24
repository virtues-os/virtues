//! `shell` — a command on the machine this server runs on, for sudo mode.
//!
//! This is the owner's admin shell handed to the model: the same account, the
//! same PATH and the same passwordless sudo the web terminal (`/ws/terminal`)
//! gives a person. Nothing here narrows it. The boundaries are elsewhere and
//! are about *who* may call it, never *what* it may run:
//!
//! - only a chat turn in `sudo` mode lists the tool (`get_tools_for_agent_mode`)
//! - the executor refuses it unless the turn's context says sudo, so a model
//!   naming it in any other mode — or an applet run, or a subagent, none of
//!   which can carry that flag — gets a refusal, not a shell
//!
//! Non-interactive by construction: stdin is closed, so a command that waits
//! for input reads EOF instead of hanging until the timeout.
//!
//! The command runs in its own process group, and the whole group is killed on
//! timeout or when the turn is stopped. Killing only `bash` would leave
//! whatever it started running with nobody attached to it. A command that
//! exits on its own keeps its group, so a deliberate `nohup … &` survives.

use std::collections::VecDeque;
use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::io::{AsyncRead, AsyncReadExt};

use super::executor::{ToolError, ToolResult};

const DEFAULT_TIMEOUT_SECS: u64 = 120;
/// An hour: builds, restores and package upgrades run long. The agent loop's
/// ceiling for this tool sits just past it (`agent::executor`).
pub const MAX_TIMEOUT_SECS: u64 = 3600;
/// Kept from the start and from the end of each stream. The end is where the
/// error usually is; the start is where the header that explains it usually is.
const HEAD_BYTES: usize = 12 * 1024;
const TAIL_BYTES: usize = 12 * 1024;

pub async fn execute(arguments: serde_json::Value) -> Result<ToolResult, ToolError> {
    let command = arguments
        .get("command")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ToolError::InvalidParameters("command is required".into()))?
        .to_string();
    let timeout_secs = arguments
        .get("timeout_seconds")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_TIMEOUT_SECS)
        .clamp(1, MAX_TIMEOUT_SECS);

    let home = std::env::var("HOME").ok();
    let cwd = arguments
        .get("cwd")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| home.clone())
        .unwrap_or_else(|| "/".to_string());

    let bash = if std::path::Path::new("/bin/bash").exists() { "/bin/bash" } else { "/bin/sh" };
    let mut cmd = tokio::process::Command::new(bash);
    cmd.arg("-c")
        .arg(&command)
        .current_dir(&cwd)
        .env("PATH", crate::api::terminal::session_path(home.as_deref()))
        // Pagers and prompts would wait on a terminal that is not there.
        .env("PAGER", "cat")
        .env("SYSTEMD_PAGER", "")
        .env("GIT_PAGER", "cat")
        .env("DEBIAN_FRONTEND", "noninteractive")
        .env("TERM", "dumb")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .kill_on_drop(true);

    let started = Instant::now();
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return Ok(ToolResult::error(format!("could not start the command in {cwd}: {e}"))),
    };
    let mut group = GroupKill(child.id().map(|p| p as i32));

    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");
    let run = async {
        let (out, err, status) =
            tokio::join!(read_capped(stdout), read_capped(stderr), child.wait());
        (out, err, status)
    };

    let (out, err, status, timed_out) =
        match tokio::time::timeout(Duration::from_secs(timeout_secs), run).await {
            Ok((out, err, status)) => (out, err, status.ok(), false),
            Err(_) => {
                group.kill();
                (Captured::default(), Captured::default(), None, true)
            }
        };
    // It finished on its own: leave its group alone, so something it started
    // in the background on purpose (`nohup … &`) keeps running.
    group.disarm();

    let mut data = serde_json::json!({
        "exit_code": status.and_then(|s| s.code()),
        "stdout": out.render(),
        "stderr": err.render(),
        "duration_ms": started.elapsed().as_millis() as u64,
    });
    if timed_out {
        data["timed_out"] = serde_json::json!(true);
        data["hint"] = serde_json::json!(format!(
            "killed after {timeout_secs}s. Raise timeout_seconds (max {MAX_TIMEOUT_SECS}), \
             or start it in the background with its output sent to a file \
             (nohup … >/tmp/job.log 2>&1 &) and check on it later."
        ));
    }
    if out.dropped > 0 || err.dropped > 0 {
        data["truncated"] = serde_json::json!(true);
    }
    Ok(ToolResult::success(data))
}

/// Kills the command's whole process group when dropped — which covers a
/// stopped turn, where this future is dropped mid-await and nothing after the
/// await ever runs.
struct GroupKill(Option<i32>);

impl GroupKill {
    fn disarm(&mut self) {
        self.0 = None;
    }

    fn kill(&self) {
        // kill(1) with a negative pid signals the whole group. `libc` is a
        // Linux-only dependency here and this has to work on a dev Mac too;
        // the path is rare (timeout, stop), so a process spawn costs nothing.
        // A group that has already exited is the outcome we wanted anyway.
        if let Some(pgid) = self.0 {
            let _ = std::process::Command::new("kill")
                .args(["-KILL", "--", &format!("-{pgid}")])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}

impl Drop for GroupKill {
    fn drop(&mut self) {
        self.kill();
    }
}

#[derive(Default)]
struct Captured {
    head: Vec<u8>,
    tail: VecDeque<u8>,
    dropped: usize,
}

impl Captured {
    fn render(&self) -> String {
        let mut s = String::from_utf8_lossy(&self.head).into_owned();
        if self.dropped > 0 {
            s.push_str(&format!("\n… {} bytes omitted …\n", self.dropped));
        }
        let tail: Vec<u8> = self.tail.iter().copied().collect();
        s.push_str(&String::from_utf8_lossy(&tail));
        s
    }
}

/// Read a stream to its end, keeping its first and last bytes. Reading to the
/// end even past the cap matters: a full pipe blocks the writer, and a command
/// blocked on its own output never exits.
async fn read_capped(mut r: impl AsyncRead + Unpin) -> Captured {
    let mut c = Captured::default();
    let mut buf = [0u8; 8192];
    loop {
        let n = match r.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let mut chunk = &buf[..n];
        if c.head.len() < HEAD_BYTES {
            let take = chunk.len().min(HEAD_BYTES - c.head.len());
            c.head.extend_from_slice(&chunk[..take]);
            chunk = &chunk[take..];
        }
        for &b in chunk {
            if c.tail.len() == TAIL_BYTES {
                c.tail.pop_front();
                c.dropped += 1;
            }
            c.tail.push_back(b);
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn runs_a_command_and_reports_its_exit() {
        let r = execute(json!({ "command": "echo hi; echo oops >&2; exit 3" })).await.unwrap();
        assert!(r.success);
        assert_eq!(r.data["exit_code"], 3);
        assert_eq!(r.data["stdout"], "hi\n");
        assert_eq!(r.data["stderr"], "oops\n");
    }

    #[tokio::test]
    async fn a_timeout_kills_the_command_and_says_so() {
        let r = execute(json!({ "command": "sleep 30", "timeout_seconds": 1 })).await.unwrap();
        assert_eq!(r.data["timed_out"], true);
        assert!(r.data["duration_ms"].as_u64().unwrap() < 5_000);
    }

    #[tokio::test]
    async fn stdin_is_closed_so_a_prompt_does_not_hang() {
        let r = execute(json!({ "command": "read x; echo got:$x", "timeout_seconds": 5 }))
            .await
            .unwrap();
        assert_eq!(r.data["stdout"], "got:\n");
        assert!(r.data.get("timed_out").is_none());
    }

    #[tokio::test]
    async fn long_output_keeps_both_ends() {
        let r = execute(json!({ "command": "echo START; head -c 100000 /dev/zero | tr '\\0' x; echo; echo END" }))
            .await
            .unwrap();
        let out = r.data["stdout"].as_str().unwrap();
        assert!(out.starts_with("START"));
        assert!(out.trim_end().ends_with("END"));
        assert!(out.contains("bytes omitted"));
        assert_eq!(r.data["truncated"], true);
    }
}

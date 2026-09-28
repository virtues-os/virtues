//! Code execution API for the AI `code_interpreter` tool.
//!
//! This runs Python the LLM wrote at runtime, so it is the path most exposed to
//! prompt injection: the same turn can have read a third party's email. On the
//! appliance we isolate it with **systemd-run** — a transient unit per
//! execution, which gives us, declaratively:
//!
//! - `PrivateNetwork=yes` — no network, the exfil channel. Only `AF_UNIX` is
//!   left as an address family, because asyncio and multiprocessing build
//!   socketpairs; the filesystem sockets it could otherwise reach (Postgres,
//!   D-Bus) are hidden below.
//! - `InaccessiblePaths` over the box's data — the lake, the state root,
//!   secrets, backups — and those sockets. `ProtectSystem=strict` alone makes
//!   them read-only, not unreadable: world-readable lake files were readable.
//! - `ProtectProc=invisible` + `ProcSubset=pid` — no other process's command
//!   line.
//! - `MemoryMax` / `MemorySwapMax=0`, `TasksMax`, `CPUQuota`, `LimitFSIZE` —
//!   cgroup- and rlimit-enforced; a runaway kills the exec, not the box.
//! - `RuntimeMaxSec` — hard timeout enforced by systemd.
//! - `DynamicUser=yes` — a throwaway uid. This is what keeps the code off
//!   root: the unit is created through `sudo`, and a system unit with no user
//!   runs as uid 0. `sandbox_properties_pin_the_boundary` holds it in place.
//! - `NoNewPrivileges`, `RestrictNamespaces`, `SystemCallFilter` — seccomp.
//!
//! The code is fed on stdin (`python3 -`), so no file needs to be readable by
//! the ephemeral user.
//!
//! ## Refusal vs. dev fallback
//!
//! In a release build (the appliance) we **refuse to run** if systemd-run is
//! unavailable rather than silently dropping the sandbox. In a debug build
//! (dev/CI, incl. macOS which has no systemd) we run the code directly — that's
//! the developer's own trusted machine.
//!
//! ## Deployment requirements
//!
//! - The box user needs passwordless `sudo` (the installer grants it) so that
//!   `sudo -n systemd-run` can create a system unit without a polkit agent.
//! - Only the standard library is guaranteed; the tool description says so.

use serde::{Deserialize, Serialize};
use std::process::Stdio;
use std::time::Instant;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

use crate::tools::shell::{read_capped, Captured};

/// Request to execute Python code
#[derive(Debug, Deserialize)]
pub struct ExecuteCodeRequest {
    /// Python code to execute
    pub code: String,
    /// Execution timeout in seconds (default: 60, max: 120)
    #[serde(default = "default_timeout")]
    pub timeout: u32,
}

fn default_timeout() -> u32 {
    60
}

/// Response from code execution
#[derive(Debug, Serialize)]
pub struct ExecuteCodeResponse {
    /// Whether the code ran and exited 0
    pub success: bool,
    /// Standard output, head and tail kept past the cap
    pub stdout: String,
    /// Standard error, head and tail kept past the cap
    pub stderr: String,
    /// What went wrong, in one line, when `success` is false
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The process's exit code, when it exited rather than being killed
    pub exit_code: Option<i32>,
    /// Killed for running past the timeout
    pub timed_out: bool,
    /// Either stream was longer than the cap and lost its middle
    pub truncated: bool,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
}

/// Per-exec memory ceiling. Enough for a large standard-library job, small
/// enough to leave the ML sidecars their share of the box.
const MEMORY_MAX: &str = "1G";

/// Execute Python code for the `code_interpreter` tool.
///
/// Appliance (Linux): isolated in a transient `systemd-run` unit. Dev/CI (any
/// debug build, incl. macOS): run directly on the trusted developer machine.
pub async fn execute_code(request: ExecuteCodeRequest) -> ExecuteCodeResponse {
    let start = Instant::now();
    let timeout_secs = request.timeout.clamp(5, 120);

    let run = if cfg!(target_os = "linux") {
        match sandboxed_command(timeout_secs) {
            Some(cmd) => run(cmd, &request.code, timeout_secs).await,
            // systemd-run missing: refuse in release (appliance) so we never run
            // LLM code unsandboxed; allow a direct run only in debug (dev/CI).
            None if cfg!(debug_assertions) => {
                tracing::warn!("systemd-run unavailable; running directly (debug build only)");
                run(direct_command(), &request.code, timeout_secs).await
            }
            None => Err(
                "code execution sandbox (systemd-run) is unavailable; refusing to run unsandboxed"
                    .to_string(),
            ),
        }
    } else {
        // No systemd off Linux — this is only ever a developer machine.
        run(direct_command(), &request.code, timeout_secs).await
    };

    let elapsed_ms = start.elapsed().as_millis() as u64;
    match run {
        Ok(r) => {
            // systemd's RuntimeMaxSec kills the unit and systemd-run just
            // exits non-zero, so a timeout is recognised by the clock.
            let timed_out = r.backstop_fired
                || (!r.success && elapsed_ms + 500 >= timeout_secs as u64 * 1000);
            let stderr = r.stderr.render();
            // The last line of stderr is the exception — the one line of a
            // traceback that says what went wrong. It rides in `error` so a
            // reader of the failure alone (the thinking block, the guard's
            // refusal) sees it; the full traceback is in `stderr` beside it.
            let error = if timed_out {
                Some(format!("Code execution timed out after {timeout_secs}s"))
            } else {
                (!r.success).then(|| {
                    stderr
                        .lines()
                        .rev()
                        .map(str::trim)
                        .find(|l| !l.is_empty())
                        .map(|l| format!("Code execution failed: {l}"))
                        .unwrap_or_else(|| "Code execution failed".to_string())
                })
            };
            ExecuteCodeResponse {
                success: r.success && !timed_out,
                stdout: r.stdout.render(),
                stderr,
                error,
                exit_code: r.exit_code,
                timed_out,
                truncated: r.stdout.dropped > 0 || r.stderr.dropped > 0,
                execution_time_ms: elapsed_ms,
            }
        }
        Err(e) => ExecuteCodeResponse {
            success: false,
            stdout: String::new(),
            stderr: String::new(),
            error: Some(e),
            exit_code: None,
            timed_out: false,
            truncated: false,
            execution_time_ms: elapsed_ms,
        },
    }
}

/// The unit properties. A function of its own so the test can hold the
/// boundary in place: dropping `DynamicUser` here would run model-written code
/// as root.
fn sandbox_properties(timeout_secs: u32) -> Vec<String> {
    let state_root = crate::applet_templates::state_root();
    let mut props = vec![
        "PrivateNetwork=yes".to_string(),
        "RestrictAddressFamilies=AF_UNIX".to_string(),
        format!("MemoryMax={MEMORY_MAX}"),
        "MemorySwapMax=0".to_string(),
        "TasksMax=64".to_string(),
        "CPUQuota=200%".to_string(),
        "LimitFSIZE=256M".to_string(),
        format!("RuntimeMaxSec={timeout_secs}"),
        "DynamicUser=yes".to_string(),
        "ProtectSystem=strict".to_string(),
        "ProtectHome=yes".to_string(),
        "PrivateTmp=yes".to_string(),
        "PrivateDevices=yes".to_string(),
        "ProtectProc=invisible".to_string(),
        "ProcSubset=pid".to_string(),
        "NoNewPrivileges=yes".to_string(),
        "RestrictNamespaces=yes".to_string(),
        "RestrictRealtime=yes".to_string(),
        "LockPersonality=yes".to_string(),
        "SystemCallArchitectures=native".to_string(),
        "SystemCallFilter=@system-service".to_string(),
        "SystemCallErrorNumber=EPERM".to_string(),
    ];
    // `-` ignores a path that does not exist, so a dev machine or a box laid
    // out differently still starts the unit.
    for path in [
        "/var/lib/virtues",
        "/etc/virtues",
        "/var/lib/postgresql",
        "/run/postgresql",
        "/run/dbus",
        "/run/user",
    ] {
        props.push(format!("InaccessiblePaths=-{path}"));
    }
    props.push(format!("InaccessiblePaths=-{}", state_root.display()));
    props
}

/// `sudo -n systemd-run …`, or None when systemd-run is not installed.
///
/// `sudo`, matching applet_runner and api/updates.rs: the box runs as
/// `User=virtues`, and a non-root user creating a system transient unit goes
/// through polkit, which on a headless box denies with "Interactive
/// authentication required". Root here does not reach the code —
/// `DynamicUser=yes` runs it as a throwaway uid.
fn sandboxed_command(timeout_secs: u32) -> Option<Command> {
    // Behind sudo, systemd-run's absence is a plain non-zero exit, so check
    // for it first to keep the release-build refusal meaningful.
    crate::applet_runner::which_systemd_run()?;
    let mut cmd = Command::new("sudo");
    cmd.args(["-n", "systemd-run"]);
    cmd.args([
        "--pipe",    // wire the unit's stdio to ours
        "--wait",    // block and propagate the exit status
        "--collect", // garbage-collect the transient unit when done
        "--quiet",   // keep systemd-run's own chatter off our stderr
    ]);
    for p in sandbox_properties(timeout_secs) {
        cmd.args(["-p", &p]);
    }
    cmd.args([
        // Give libs a writable home inside the private /tmp.
        "-E",
        "HOME=/tmp",
        "-E",
        "MPLCONFIGDIR=/tmp",
        "--",
        "python3",
        "-I", // isolated mode: ignore env + user site-packages
        "-",  // read the program from stdin
    ]);
    Some(cmd)
}

/// Run code directly, unsandboxed. Developer machines only.
fn direct_command() -> Command {
    let mut cmd = Command::new("python3");
    cmd.args(["-I", "-"]);
    cmd
}

struct Run {
    stdout: Captured,
    stderr: Captured,
    success: bool,
    exit_code: Option<i32>,
    backstop_fired: bool,
}

/// Spawn, feed the code on stdin, and read both streams to their end with
/// their head and tail kept — reading past the cap so a chatty program never
/// blocks on a full pipe.
async fn run(mut cmd: Command, code: &str, timeout_secs: u32) -> Result<Run, String> {
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.kill_on_drop(true); // the backstop drops the future → kills the child

    let mut child = cmd.spawn().map_err(|e| format!("failed to start code execution: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(code.as_bytes())
            .await
            .map_err(|e| format!("failed to write code to stdin: {e}"))?;
        drop(stdin); // EOF
    }

    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let collect = async {
        let (out, err) = tokio::join!(read_capped(stdout), read_capped(stderr));
        let status = child.wait().await;
        (out, err, status)
    };

    // systemd enforces RuntimeMaxSec in the sandbox; this is the timeout for a
    // direct run and a backstop for a unit that hangs.
    let backstop = Duration::from_secs(timeout_secs as u64 + 10);
    match timeout(backstop, collect).await {
        Ok((out, err, Ok(status))) => Ok(Run {
            stdout: out,
            stderr: err,
            success: status.success(),
            exit_code: status.code(),
            backstop_fired: false,
        }),
        Ok((_, _, Err(e))) => Err(format!("process error: {e}")),
        Err(_) => Ok(Run {
            stdout: Captured::default(),
            stderr: Captured::default(),
            success: false,
            exit_code: None,
            backstop_fired: true,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(code: &str, timeout: u32) -> ExecuteCodeRequest {
        ExecuteCodeRequest { code: code.to_string(), timeout }
    }

    #[test]
    fn sandbox_properties_pin_the_boundary() {
        let props = sandbox_properties(30);
        for must in [
            "DynamicUser=yes",
            "PrivateNetwork=yes",
            "RestrictAddressFamilies=AF_UNIX",
            "NoNewPrivileges=yes",
            "ProtectSystem=strict",
            "ProtectProc=invisible",
            "MemoryMax=1G",
            "RuntimeMaxSec=30",
            "InaccessiblePaths=-/var/lib/virtues",
            "InaccessiblePaths=-/run/postgresql",
        ] {
            assert!(props.iter().any(|p| p == must), "missing {must}");
        }
        assert!(
            !props.iter().any(|p| p.starts_with("User=")),
            "a named user would replace the throwaway uid"
        );
    }

    #[tokio::test]
    async fn runs_code_and_returns_stdout() {
        let r = execute_code(request("x = 2 + 2\nprint(f'Result: {x}')", 10)).await;
        assert!(r.success, "{r:?}");
        assert_eq!(r.stdout, "Result: 4\n");
        assert_eq!(r.exit_code, Some(0));
    }

    #[tokio::test]
    async fn an_exception_is_the_error_and_the_traceback_is_kept() {
        let r = execute_code(request("1/0", 10)).await;
        assert!(!r.success);
        assert_eq!(
            r.error.as_deref(),
            Some("Code execution failed: ZeroDivisionError: division by zero")
        );
        assert!(r.stderr.contains("Traceback"));
        assert!(!r.timed_out);
    }

    #[tokio::test]
    async fn long_output_keeps_its_ends() {
        let r = execute_code(request(
            "print('first')\nfor i in range(200000): print(i)\nprint('last')",
            30,
        ))
        .await;
        assert!(r.success);
        assert!(r.truncated);
        assert!(r.stdout.starts_with("first\n"));
        assert!(r.stdout.ends_with("last\n"));
        assert!(r.stdout.len() < 64 * 1024);
    }

    #[tokio::test]
    async fn a_timeout_says_so() {
        let r = execute_code(request("import time; time.sleep(30)", 5)).await;
        assert!(!r.success);
        assert!(r.timed_out);
        assert_eq!(r.error.as_deref(), Some("Code execution timed out after 5s"));
    }
}

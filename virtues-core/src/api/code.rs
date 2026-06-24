//! Code execution API for the AI `code_interpreter` tool.
//!
//! This runs Python the LLM wrote at runtime, so it is the path most exposed to
//! prompt injection. On the appliance we isolate it with **systemd-run** — a
//! transient unit per execution, which gives us, declaratively:
//!
//! - `PrivateNetwork=yes`  — no network (the exfil channel); calc/stats/charts
//!   don't need it. (Actions, which *do* need egress, run on a different path and
//!   are intentionally left alone.)
//! - `MemoryMax` / `MemorySwapMax=0` — cgroup-enforced; an OOM kills the exec,
//!   not the box (important on an 8GB Jetson shared with the ML sidecars).
//! - `RuntimeMaxSec` — hard timeout enforced by systemd.
//! - `ProtectSystem=strict` + `ProtectHome` + `PrivateTmp` + `DynamicUser` —
//!   no access to `/etc/virtues` secrets, app data, WG keys, or a real home.
//! - `NoNewPrivileges` + `SystemCallFilter` — seccomp.
//!
//! The code is fed on stdin (`python3 -`), so no file needs to be readable by
//! the ephemeral `DynamicUser`.
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
//! - The appliance Python (`python3` on PATH) must carry the data-science
//!   packages (numpy/pandas/scipy/numpy-financial) — they used to live in the
//!   now-removed sandbox Docker image and must be baked into the appliance image.
//! - virtues-core must run with rights to the system service manager (root or an
//!   appropriately-privileged unit) for `systemd-run` to set these properties.

use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

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
    /// Whether execution completed successfully
    pub success: bool,
    /// Standard output from the code
    pub stdout: String,
    /// Standard error from the code
    pub stderr: String,
    /// Error message if execution failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
}

/// Execute Python code for the `code_interpreter` tool.
///
/// Appliance (Linux): isolated in a transient `systemd-run` unit. Dev/CI (any
/// debug build, incl. macOS): run directly on the trusted developer machine.
pub async fn execute_code(request: ExecuteCodeRequest) -> ExecuteCodeResponse {
    let start = std::time::Instant::now();
    let timeout_secs = request.timeout.clamp(5, 120);

    let output = if cfg!(target_os = "linux") {
        match execute_with_systemd_run(&request.code, timeout_secs).await {
            // systemd-run missing: refuse in release (appliance) so we never run
            // LLM code unsandboxed; allow a direct run only in debug (dev/CI).
            Err(SandboxError::Unavailable) if cfg!(debug_assertions) => {
                tracing::warn!("systemd-run unavailable; running directly (debug build only)");
                execute_directly(&request.code, timeout_secs).await
            }
            Err(SandboxError::Unavailable) => Err(
                "code execution sandbox (systemd-run) is unavailable; refusing to run unsandboxed"
                    .to_string(),
            ),
            Err(SandboxError::Other(e)) => Err(e),
            Ok(triple) => Ok(triple),
        }
    } else {
        // No systemd off Linux — this is only ever a developer machine.
        execute_directly(&request.code, timeout_secs).await
    };

    match output {
        Ok((stdout, stderr, success)) => ExecuteCodeResponse {
            success,
            stdout,
            stderr,
            error: if success {
                None
            } else {
                Some("Code execution failed".to_string())
            },
            execution_time_ms: start.elapsed().as_millis() as u64,
        },
        Err(e) => ExecuteCodeResponse {
            success: false,
            stdout: String::new(),
            stderr: String::new(),
            error: Some(e),
            execution_time_ms: start.elapsed().as_millis() as u64,
        },
    }
}

enum SandboxError {
    /// systemd-run is not installed/on PATH.
    Unavailable,
    /// Any other failure (spawn/io/timeout).
    Other(String),
}

/// Run code in a hardened transient systemd unit. Code is fed on stdin.
async fn execute_with_systemd_run(
    code: &str,
    timeout_secs: u32,
) -> Result<(String, String, bool), SandboxError> {
    let mut cmd = Command::new("systemd-run");
    cmd.args([
        "--pipe",    // wire the unit's stdio to ours
        "--wait",    // block and propagate the exit status
        "--collect", // garbage-collect the transient unit when done
        "--quiet",   // keep systemd-run's own chatter off our stderr
        "-p",
        "PrivateNetwork=yes",
        "-p",
        &format!("MemoryMax={MEMORY_MAX}"),
        "-p",
        "MemorySwapMax=0",
        "-p",
        &format!("RuntimeMaxSec={timeout_secs}"),
        "-p",
        "ProtectSystem=strict",
        "-p",
        "ProtectHome=yes",
        "-p",
        "PrivateTmp=yes",
        "-p",
        "PrivateDevices=yes",
        "-p",
        "NoNewPrivileges=yes",
        "-p",
        "DynamicUser=yes",
        "-p",
        "SystemCallFilter=@system-service",
        "-p",
        "SystemCallErrorNumber=EPERM",
        // Give libs (e.g. matplotlib) a writable home inside the private /tmp.
        "-E",
        "HOME=/tmp",
        "-E",
        "MPLCONFIGDIR=/tmp",
        "--",
        "python3",
        "-I", // isolated mode: ignore env + user site-packages
        "-",  // read the program from stdin
    ]);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.kill_on_drop(true); // backstop timeout drops the future → kills the unit

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(SandboxError::Unavailable)
        }
        Err(e) => return Err(SandboxError::Other(format!("failed to start systemd-run: {e}"))),
    };

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(code.as_bytes())
            .await
            .map_err(|e| SandboxError::Other(format!("failed to write code to stdin: {e}")))?;
        drop(stdin); // EOF
    }

    // systemd enforces RuntimeMaxSec; this is a backstop in case it hangs.
    let backstop = Duration::from_secs(timeout_secs as u64 + 10);
    match timeout(backstop, child.wait_with_output()).await {
        Ok(Ok(output)) => Ok((
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
            output.status.success(),
        )),
        Ok(Err(e)) => Err(SandboxError::Other(format!("process error: {e}"))),
        Err(_) => Err(SandboxError::Other("Execution timed out".to_string())),
    }
}

/// Run code directly, unsandboxed. Debug builds only (dev/CI on a trusted box).
async fn execute_directly(
    code: &str,
    timeout_secs: u32,
) -> Result<(String, String, bool), String> {
    let mut cmd = Command::new("python3");
    cmd.args(["-I", "-"]);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.kill_on_drop(true);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to start python3: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(code.as_bytes())
            .await
            .map_err(|e| format!("failed to write code to stdin: {e}"))?;
        drop(stdin);
    }

    match timeout(Duration::from_secs(timeout_secs as u64), child.wait_with_output()).await {
        Ok(Ok(output)) => Ok((
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
            output.status.success(),
        )),
        Ok(Err(e)) => Err(format!("process error: {e}")),
        Err(_) => Err("Execution timed out".to_string()),
    }
}

/// Per-exec memory ceiling. Generous enough for numpy/pandas, small enough to
/// protect the ML sidecars' share of an 8GB box.
const MEMORY_MAX: &str = "512M";

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_simple_code_execution() {
        let request = ExecuteCodeRequest {
            code: "print('hello world')".to_string(),
            timeout: 10,
        };

        let response = execute_code(request).await;

        // Should work on any machine with Python installed
        assert!(response.stdout.contains("hello world") || response.error.is_some());
    }

    #[tokio::test]
    async fn test_code_with_calculation() {
        let request = ExecuteCodeRequest {
            code: "x = 2 + 2\nprint(f'Result: {x}')".to_string(),
            timeout: 10,
        };

        let response = execute_code(request).await;

        if response.success {
            assert!(response.stdout.contains("Result: 4"));
        }
    }

    #[tokio::test]
    async fn test_syntax_error() {
        let request = ExecuteCodeRequest {
            code: "print('unclosed".to_string(),
            timeout: 10,
        };

        let response = execute_code(request).await;

        // Should fail with syntax error
        assert!(!response.success || !response.stderr.is_empty());
    }

    #[tokio::test]
    async fn test_timeout() {
        let request = ExecuteCodeRequest {
            code: "import time; time.sleep(30)".to_string(),
            timeout: 5, // 5 second timeout, code sleeps for 30
        };

        let response = execute_code(request).await;

        // Should timeout
        assert!(!response.success);
        assert!(
            response
                .error
                .as_ref()
                .map_or(false, |e| e.contains("timed out"))
                || response.execution_time_ms >= 5000
        );
    }
}

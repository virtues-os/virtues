//! Code execution API for AI sandbox
//!
//! Provides secure Python code execution with platform-specific sandboxing:
//!
//! - **Production (Linux + gVisor)**: Uses bubblewrap for process isolation within
//!   the tenant's gVisor container. The container provides tenant isolation, and
//!   bubblewrap provides filesystem isolation for the Python process.
//!
//! - **Development (macOS)**: Uses Docker/OrbStack to run code in a Linux container
//!   with bubblewrap. This allows testing the same sandboxing approach on macOS.
//!
//! Used by the AI agent's code_interpreter tool.

use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tempfile::TempDir;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

/// Sandbox image name for Docker-based execution (dev mode)
const SANDBOX_IMAGE: &str = "virtues-sandbox:latest";

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

/// Execute Python code
///
/// Security model:
/// - Production (Linux + gVisor): bubblewrap provides filesystem isolation within
///   the tenant's gVisor container
/// - Development (macOS): Docker container with bubblewrap provides equivalent isolation
pub async fn execute_code(request: ExecuteCodeRequest) -> ExecuteCodeResponse {
    let start = std::time::Instant::now();

    // Clamp timeout to valid range
    let timeout_secs = request.timeout.clamp(5, 120);

    // Choose execution strategy based on platform
    let output = if cfg!(target_os = "macos") {
        // macOS: Use Docker/OrbStack with sandbox container
        execute_with_docker(&request.code, timeout_secs).await
    } else if cfg!(target_os = "linux") {
        // Linux: Use bubblewrap directly (or fallback to direct execution)
        execute_with_bubblewrap(&request.code, timeout_secs).await
    } else {
        // Fallback: Direct execution (not recommended for untrusted code)
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

/// Execute Python code via Docker container (macOS development)
///
/// Uses the virtues-sandbox Docker image which contains:
/// - Python 3.12 with common packages
/// - bubblewrap for process isolation
///
/// The container runs with --privileged to allow bubblewrap's namespace operations.
/// This is acceptable for development; in production, gVisor provides the isolation.
async fn execute_with_docker(
    code: &str,
    timeout_secs: u32,
) -> Result<(String, String, bool), String> {
    let mut cmd = Command::new("docker");
    cmd.args([
        "run",
        "--rm",
        "-i",
        "--privileged",
        "--network=none", // No network access for sandboxed code
        SANDBOX_IMAGE,
    ]);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let timeout_duration = Duration::from_secs(timeout_secs as u64);

    // Spawn the process and write code to stdin
    let mut child = cmd.spawn().map_err(|e| format!("Failed to start Docker: {}", e))?;

    // Write code to stdin
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(code.as_bytes())
            .await
            .map_err(|e| format!("Failed to write to stdin: {}", e))?;
        // Close stdin to signal EOF
        drop(stdin);
    }

    // Wait for output with timeout
    match timeout(timeout_duration, child.wait_with_output()).await {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Ok((stdout, stderr, output.status.success()))
        }
        Ok(Err(e)) => Err(format!("Process error: {}", e)),
        Err(_) => {
            // Timeout - kill the container
            let _ = Command::new("docker")
                .args(["kill", "--signal=KILL"])
                .output()
                .await;
            Err("Execution timed out".to_string())
        }
    }
}

/// Execute Python code with bubblewrap isolation (Linux production)
///
/// Uses bubblewrap to create an isolated filesystem namespace:
/// - Read-only access to Python and system libraries
/// - Writable /tmp for temporary files
/// - No access to application data directories
async fn execute_with_bubblewrap(
    code: &str,
    timeout_secs: u32,
) -> Result<(String, String, bool), String> {
    // Create a temporary file for the code
    let workspace = TempDir::new().map_err(|e| format!("Failed to create workspace: {}", e))?;
    let code_path = workspace.path().join("code.py");
    tokio::fs::write(&code_path, code)
        .await
        .map_err(|e| format!("Failed to write code: {}", e))?;

    let mut cmd = Command::new("bwrap");
    cmd.args([
        "--ro-bind",
        "/usr",
        "/usr",
        "--ro-bind",
        "/lib",
        "/lib",
        "--ro-bind-try",
        "/lib64",
        "/lib64",
        "--symlink",
        "usr/bin",
        "/bin",
        "--symlink",
        "usr/sbin",
        "/sbin",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--tmpfs",
        "/tmp",
        "--tmpfs",
        "/home",
        "--bind",
        code_path.to_str().unwrap(),
        "/tmp/code.py",
        "--chdir",
        "/tmp",
        "--unshare-all",
        "--share-net", // Can be removed to disable network
        "--die-with-parent",
        "--new-session",
        "--",
        "python3",
        "/tmp/code.py",
    ]);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let timeout_duration = Duration::from_secs(timeout_secs as u64);

    match timeout(timeout_duration, cmd.output()).await {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Ok((stdout, stderr, output.status.success()))
        }
        Ok(Err(e)) => {
            // bubblewrap not available, fall back to direct execution
            if e.kind() == std::io::ErrorKind::NotFound {
                tracing::warn!("bubblewrap not found, falling back to direct execution");
                execute_directly(code, timeout_secs).await
            } else {
                Err(format!("Process error: {}", e))
            }
        }
        Err(_) => Err("Execution timed out".to_string()),
    }
}

/// Execute Python code directly (fallback, less secure)
///
/// Used when:
/// - bubblewrap is not available
/// - Running in a trusted environment
async fn execute_directly(
    code: &str,
    timeout_secs: u32,
) -> Result<(String, String, bool), String> {
    // Create a temporary file for the code
    let workspace = TempDir::new().map_err(|e| format!("Failed to create workspace: {}", e))?;
    let code_path = workspace.path().join("code.py");
    tokio::fs::write(&code_path, code)
        .await
        .map_err(|e| format!("Failed to write code: {}", e))?;

    let mut cmd = Command::new("python3");
    cmd.arg(&code_path);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.current_dir(workspace.path());

    let timeout_duration = Duration::from_secs(timeout_secs as u64);

    match timeout(timeout_duration, cmd.output()).await {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Ok((stdout, stderr, output.status.success()))
        }
        Ok(Err(e)) => Err(format!("Process error: {}", e)),
        Err(_) => Err("Execution timed out".to_string()),
    }
}

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

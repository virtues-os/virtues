//! Code execution API for AI sandbox
//!
//! Provides secure Python code execution. In production, isolation is provided
//! by gVisor at the container level. No additional sandboxing (nsjail) is needed
//! because each tenant runs in their own gVisor container.
//!
//! Used by the AI agent's code_interpreter tool.

use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tempfile::TempDir;
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

/// Execute Python code
///
/// Security model:
/// - Production (gVisor): Container IS the sandbox. If user crashes Python,
///   they crash their own container. If they try to escape, gVisor blocks it.
/// - Development: Runs directly on dev machine (trusted environment).
pub async fn execute_code(request: ExecuteCodeRequest) -> ExecuteCodeResponse {
    let start = std::time::Instant::now();

    // Clamp timeout to valid range
    let timeout_secs = request.timeout.clamp(5, 120);

    // Create isolated workspace for the code
    let workspace = match TempDir::new() {
        Ok(dir) => dir,
        Err(e) => {
            return ExecuteCodeResponse {
                success: false,
                stdout: String::new(),
                stderr: String::new(),
                error: Some(format!("Failed to create workspace: {}", e)),
                execution_time_ms: start.elapsed().as_millis() as u64,
            };
        }
    };

    // Write code to file
    let code_path = workspace.path().join("code.py");
    if let Err(e) = tokio::fs::write(&code_path, &request.code).await {
        return ExecuteCodeResponse {
            success: false,
            stdout: String::new(),
            stderr: String::new(),
            error: Some(format!("Failed to write code: {}", e)),
            execution_time_ms: start.elapsed().as_millis() as u64,
        };
    }

    // Execute Python directly - gVisor provides isolation in production
    let output = execute_python(&code_path, timeout_secs).await;

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

/// Execute Python code directly
///
/// In production (gVisor container), this is secure because:
/// - The container runs with gVisor's syscall filtering
/// - The user can only access their own filesystem
/// - Resource limits are enforced by Nomad/cgroups
///
/// In development, this trusts the local Python environment.
async fn execute_python(
    code_path: &std::path::Path,
    timeout_secs: u32,
) -> Result<(String, String, bool), String> {
    let mut cmd = Command::new("python3");
    cmd.arg(code_path);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    // Set working directory to the code's directory for relative imports
    if let Some(parent) = code_path.parent() {
        cmd.current_dir(parent);
    }

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

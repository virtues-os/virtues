//! The `shell` tool: a bash command in the sandbox.
//!
//! This runs commands the model wrote at runtime, so it is the path most
//! exposed to prompt injection: the same turn can have read a third party's
//! email. On the appliance it runs in the sandbox ([`crate::sandbox`]): a
//! transient systemd unit with no network, a throwaway user, the box's data
//! hidden, and memory, CPU, process, file-size and time limits.
//!
//! The command runs with `bash -c` and no stdin. Its working directory is
//! `/work`, the run's workspace bind-mounted in, and the pinned Python
//! environment is bind-mounted read-only at `/opt/py`, first on `PATH` (both
//! in `api/code_env.rs`). An applet's run also gets its folder read-only at
//! `/skill`, and an egress socket when its owner allowed hosts. Everything
//! else of the box's stays hidden, other chats' workspaces included.
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
//! - The package environment builds itself on first use and needs the network
//!   once; until then a run gets the standard library and is told so.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Instant;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

use crate::api::code_env::{self, Workspace};
use crate::sandbox::{Bind, Unit};
use crate::tools::sudo_shell::{read_capped, Captured};

/// One `shell` call.
#[derive(Debug, Default)]
pub struct ShellRequest {
    /// Run with `bash -c`.
    pub command: String,
    /// Seconds, clamped to 5..=120.
    pub timeout: u32,
    /// An applet's folder, mounted read-only at `/skill`.
    pub folder: Option<PathBuf>,
    /// The run's egress proxy socket, when its owner allowed hosts.
    pub egress: Option<PathBuf>,
}

/// What a `shell` call returns.
#[derive(Debug, Serialize)]
pub struct ShellResponse {
    /// Whether the command ran and exited 0
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
    /// Images the run wrote to `out/`, as workspace-relative paths
    pub images: Vec<String>,
    /// The pinned packages were there; false means standard library only
    pub packages_ready: bool,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
}

/// At most this many images come back from one run.
const MAX_IMAGES: usize = 8;

/// Per-exec memory ceiling. Enough for a large standard-library job, small
/// enough to leave the ML sidecars their share of the box.
const MEMORY_MAX: &str = "1G";

/// Run a `shell` command.
///
/// Appliance (Linux): isolated in a transient `systemd-run` unit. Dev/CI (any
/// debug build, incl. macOS): run directly on the trusted developer machine.
pub async fn run_shell(request: ShellRequest, ws: &Workspace) -> ShellResponse {
    let start = Instant::now();
    let timeout_secs = request.timeout.clamp(5, 120);
    let env = code_env::ready_env();
    let packages_ready = env.is_some();

    let held = ws.bytes();
    if held > code_env::WORKSPACE_MAX_BYTES {
        return ShellResponse {
            error: Some(format!(
                "The workspace holds {} MB, over its {} MB limit. Delete files you no longer need (rm) in a small run first.",
                held / 1_000_000,
                code_env::WORKSPACE_MAX_BYTES / 1_000_000
            )),
            ..ShellResponse::failed(packages_ready, 0)
        };
    }

    let out_before = snapshot_out(ws);
    let run = if cfg!(target_os = "linux") {
        match sandboxed_command(timeout_secs, ws, env.as_deref(), &request) {
            Some(cmd) => run(cmd, timeout_secs).await,
            // systemd-run missing: refuse in release (appliance) so we never run
            // LLM code unsandboxed; allow a direct run only in debug (dev/CI).
            None if cfg!(debug_assertions) => {
                tracing::warn!("systemd-run unavailable; running directly (debug build only)");
                run(direct_command(ws, env.as_deref(), &request.command), timeout_secs).await
            }
            None => Err(
                "the shell sandbox (systemd-run) is unavailable; refusing to run unsandboxed"
                    .to_string(),
            ),
        }
    } else {
        // No systemd off Linux — this is only ever a developer machine.
        run(direct_command(ws, env.as_deref(), &request.command), timeout_secs).await
    };

    let elapsed_ms = start.elapsed().as_millis() as u64;
    match run {
        Ok(r) => {
            // systemd's RuntimeMaxSec kills the unit and systemd-run just
            // exits non-zero, so a timeout is recognised by the clock.
            let timed_out = r.backstop_fired
                || (!r.success && elapsed_ms + 500 >= timeout_secs as u64 * 1000);
            let stderr = r.stderr.render();
            // The last line of stderr is usually what went wrong: the
            // exception that ends a Python traceback, or a command's own
            // complaint. It rides in `error` so a
            // reader of the failure alone (the thinking block, the guard's
            // refusal) sees it; the full traceback is in `stderr` beside it.
            let error = if timed_out {
                Some(format!("The command timed out after {timeout_secs}s"))
            } else {
                (!r.success).then(|| {
                    stderr
                        .lines()
                        .rev()
                        .map(str::trim)
                        .find(|l| !l.is_empty())
                        .map(|l| format!("The command failed: {l}"))
                        .unwrap_or_else(|| match r.exit_code {
                            Some(code) => format!("The command exited with status {code}"),
                            None => "The command failed".to_string(),
                        })
                })
            };
            ShellResponse {
                success: r.success && !timed_out,
                stdout: r.stdout.render(),
                stderr,
                error,
                exit_code: r.exit_code,
                timed_out,
                truncated: r.stdout.dropped > 0 || r.stderr.dropped > 0,
                images: new_images(ws, &out_before),
                packages_ready,
                execution_time_ms: elapsed_ms,
            }
        }
        Err(e) => ShellResponse {
            error: Some(e),
            ..ShellResponse::failed(packages_ready, elapsed_ms)
        },
    }
}

impl ShellResponse {
    fn failed(packages_ready: bool, execution_time_ms: u64) -> Self {
        Self {
            success: false,
            stdout: String::new(),
            stderr: String::new(),
            error: None,
            exit_code: None,
            timed_out: false,
            truncated: false,
            images: Vec::new(),
            packages_ready,
            execution_time_ms,
        }
    }
}

/// What `out/` holds before a run: each file's modified time and size.
type OutSnapshot = std::collections::HashMap<String, (Option<std::time::SystemTime>, u64)>;

fn snapshot_out(ws: &Workspace) -> OutSnapshot {
    let Ok(entries) = std::fs::read_dir(ws.out_dir()) else { return OutSnapshot::new() };
    entries
        .flatten()
        .filter_map(|e| {
            let m = e.metadata().ok().filter(|m| m.is_file())?;
            Some((e.file_name().into_string().ok()?, (m.modified().ok(), m.len())))
        })
        .collect()
}

/// Images in `out/` this run wrote, as `out/<name>`: new, or changed since
/// the snapshot. One left from an earlier run is not this run's result.
fn new_images(ws: &Workspace, before: &OutSnapshot) -> Vec<String> {
    let mut names: Vec<String> = snapshot_out(ws)
        .into_iter()
        .filter(|(name, state)| image_type(name).is_some() && before.get(name) != Some(state))
        .map(|(name, _)| name)
        .collect();
    names.sort();
    names.truncate(MAX_IMAGES);
    names.into_iter().map(|n| format!("out/{n}")).collect()
}

/// The media type of an image file the chat can show, by extension.
pub fn image_type(name: &str) -> Option<&'static str> {
    let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => return None,
    })
}

/// `PATH` inside the unit: the pinned Python first, when it is ready.
const SANDBOX_PATH: &str = "/opt/py/bin:/usr/local/bin:/usr/bin:/bin";
const SANDBOX_PATH_WITHOUT_PACKAGES: &str = "/usr/local/bin:/usr/bin:/bin";

/// The sandboxed unit for one run: the workspace at `/work`, the package
/// environment read-only at `/opt/py`, and for an applet its folder at
/// `/skill` and its egress socket.
fn sandbox_unit(timeout_secs: u32, workspace: &Path, env: Option<&Path>, request: &ShellRequest) -> Unit {
    let mut binds = vec![Bind { host: workspace.to_path_buf(), inside: "/work".into(), writable: true }];
    if let Some(env) = env {
        binds.push(Bind { host: env.to_path_buf(), inside: "/opt/py".into(), writable: false });
    }
    if let Some(folder) = &request.folder {
        binds.push(Bind { host: folder.clone(), inside: "/skill".into(), writable: false });
    }
    Unit {
        timeout_secs,
        memory_max: MEMORY_MAX,
        working_directory: "/work",
        binds,
        egress: request.egress.clone(),
        postgres: false,
        extra: Vec::new(),
    }
}

/// `sudo -n systemd-run …` running the command, or None when systemd-run is
/// not installed.
fn sandboxed_command(timeout_secs: u32, ws: &Workspace, env: Option<&Path>, request: &ShellRequest) -> Option<Command> {
    let argv = ["/bin/bash".to_string(), "-c".to_string(), request.command.clone()];
    let path = if env.is_some() { SANDBOX_PATH } else { SANDBOX_PATH_WITHOUT_PACKAGES };
    sandbox_unit(timeout_secs, &ws.dir, env, request).command(
        &argv,
        &[
            ("PATH", path),
            // Give libs a writable home inside the private /tmp.
            ("HOME", "/tmp"),
            ("MPLCONFIGDIR", "/tmp"),
            // No display: charts render to files.
            ("MPLBACKEND", "Agg"),
            // The pinned packages, not whatever a home directory would add.
            ("PYTHONNOUSERSITE", "1"),
        ],
    )
}

/// Run the command directly, unsandboxed. Developer machines only.
fn direct_command(ws: &Workspace, env: Option<&Path>, command: &str) -> Command {
    let mut cmd = Command::new("bash");
    cmd.args(["-c", command]);
    cmd.current_dir(&ws.dir);
    cmd.env("MPLBACKEND", "Agg");
    if let Some(env) = env {
        let path = std::env::var("PATH").unwrap_or_default();
        cmd.env("PATH", format!("{}:{path}", env.join("bin").display()));
    }
    cmd
}

struct Run {
    stdout: Captured,
    stderr: Captured,
    success: bool,
    exit_code: Option<i32>,
    backstop_fired: bool,
}

/// Spawn with no stdin, and read both streams to their end with their head
/// and tail kept — reading past the cap so a chatty program never blocks on a
/// full pipe.
async fn run(mut cmd: Command, timeout_secs: u32) -> Result<Run, String> {
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.kill_on_drop(true); // the backstop drops the future → kills the child

    let mut child = cmd.spawn().map_err(|e| format!("failed to start the command: {e}"))?;

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

    fn request(command: &str, timeout: u32) -> ShellRequest {
        ShellRequest { command: command.to_string(), timeout, ..Default::default() }
    }

    async fn run_shell(request: ShellRequest) -> ShellResponse {
        let ws = Workspace::temporary().unwrap();
        super::run_shell(request, &ws).await
    }

    /// The boundary itself is pinned in `sandbox`; this pins what a shell
    /// run adds to it.
    #[test]
    fn a_shell_run_mounts_its_workspace_and_packages_and_has_no_way_out() {
        let props = sandbox_unit(30, Path::new("/ws"), Some(Path::new("/env")), &request("true", 30)).properties();
        for must in ["BindPaths=/ws:/work", "BindReadOnlyPaths=/env:/opt/py", "WorkingDirectory=/work", "MemoryMax=1G"] {
            assert!(props.iter().any(|p| p == must), "missing {must}");
        }
        assert!(props.iter().any(|p| p == "RestrictAddressFamilies=AF_UNIX"));
        assert!(!props.iter().any(|p| p.contains("/skill")), "only an applet's run has a folder");
    }

    /// An applet's run sees its folder at /skill and, when allowed, its
    /// egress socket.
    #[test]
    fn an_applets_shell_sees_its_folder_and_its_way_out() {
        let req = ShellRequest {
            command: "true".into(),
            timeout: 30,
            folder: Some("/applets/user/tides".into()),
            egress: Some("/run/virtues/egress/r1.sock".into()),
        };
        let props = sandbox_unit(30, Path::new("/ws"), None, &req).properties();
        assert!(props.iter().any(|p| p == "BindReadOnlyPaths=/applets/user/tides:/skill"));
        assert!(props.iter().any(|p| p.ends_with(":/run/egress.sock")));
    }

    #[tokio::test]
    async fn runs_a_command_and_python_and_returns_stdout() {
        let r = run_shell(request("x=$((2 + 2)); echo \"Result: $x\"", 10)).await;
        assert!(r.success, "{r:?}");
        assert_eq!(r.stdout, "Result: 4\n");
        let r = run_shell(request("python3 - <<'EOF'\nx = 2 + 2\nprint(f'Result: {x}')\nEOF", 10)).await;
        assert!(r.success, "{r:?}");
        assert_eq!(r.stdout, "Result: 4\n");
        assert_eq!(r.exit_code, Some(0));
    }

    #[tokio::test]
    async fn an_exception_is_the_error_and_the_traceback_is_kept() {
        let r = run_shell(request("python3 -c '1/0'", 10)).await;
        assert!(!r.success);
        assert_eq!(r.error.as_deref(), Some("The command failed: ZeroDivisionError: division by zero"));
        assert!(r.stderr.contains("Traceback"));
        assert!(!r.timed_out);
        let r = run_shell(request("exit 3", 10)).await;
        assert_eq!(r.error.as_deref(), Some("The command exited with status 3"));
    }

    #[tokio::test]
    async fn long_output_keeps_its_ends() {
        let r = run_shell(request("echo first; seq 1 200000; echo last", 30)).await;
        assert!(r.success);
        assert!(r.truncated);
        assert!(r.stdout.starts_with("first\n"));
        assert!(r.stdout.ends_with("last\n"));
        assert!(r.stdout.len() < 64 * 1024);
    }

    #[tokio::test]
    async fn a_timeout_says_so() {
        let r = run_shell(request("sleep 30", 5)).await;
        assert!(!r.success);
        assert!(r.timed_out);
        assert_eq!(r.error.as_deref(), Some("The command timed out after 5s"));
    }

    #[tokio::test]
    async fn the_workspace_carries_files_between_runs_and_images_come_back() {
        let ws = Workspace::temporary().unwrap();
        let first = super::run_shell(request("echo 41 > n.txt", 10), &ws).await;
        assert!(first.success, "{first:?}");
        let second =
            super::run_shell(request("echo $(( $(cat n.txt) + 1 )); echo '<svg/>' > out/plot.svg", 10), &ws).await;
        assert_eq!(second.stdout, "42\n");
        assert_eq!(second.images, vec!["out/plot.svg".to_string()]);
        let third = super::run_shell(request("echo again", 10), &ws).await;
        assert!(third.images.is_empty(), "an earlier run's image is not this run's");
    }
}

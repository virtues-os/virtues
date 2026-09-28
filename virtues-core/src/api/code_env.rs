//! Where `code_interpreter`'s Python and its files live: the pinned package
//! environment, and a workspace per chat.
//!
//! **The environment** is a venv built from `python/requirements.lock` — every
//! file hash-pinned, installed with `pip --require-hashes`, wheels only. The
//! box builds it itself, in the background, the first time it is wanted: the
//! installer does not run on upgrade, so anything it laid down would never
//! reach a box that already exists. Its directory is named after the lock's
//! digest, so a release that changes the lock builds the new set beside the
//! old one, swaps by rename, and deletes the old. Until it is ready, code runs
//! on the system `python3` with the standard library, and the result says so.
//!
//! pip itself comes from a pinned wheel run directly (`python pip.whl/pip`),
//! not from `ensurepip`: stock Ubuntu ships `python3` without it unless
//! `python3-venv` is installed, and a venv made `--without-pip` needs neither.
//!
//! **A workspace** is a directory the sandbox sees as `/work`, its working
//! directory. A saved chat's persists until the chat is purged, so files and
//! query results carry between calls; a ghost chat, an applet run or a
//! research worker gets one that is deleted when the call ends. The sandbox's
//! throwaway uid differs every run, so the workspace is mode 0777 and the unit
//! runs with `UMask=0000`: everything written there stays readable and
//! deletable by the next run and by the server. Its parents are 0700, which
//! the sandbox never needs to cross — the directory is bind-mounted in.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use tokio::process::Command;

const LOCK: &str = include_str!("../../python/requirements.lock");

const PIP_WHEEL_URL: &str = "https://files.pythonhosted.org/packages/f3/6e/1736e5b4ae2b778ef2f81c47d797de9f891d4d8acb047a24ca37a60294dd/pip-26.2.1-py3-none-any.whl";
const PIP_WHEEL_SHA256: &str = "71138adf1f4ca900cdb7d289c21b7494329f2332b6d85f0e1c42108c0384ed3e";

/// A failed build (offline box, PyPI down) is retried no sooner than this.
const RETRY_AFTER: Duration = Duration::from_secs(3600);

/// Past this, a workspace takes no more runs until files are deleted.
pub const WORKSPACE_MAX_BYTES: u64 = 1024 * 1024 * 1024;

const WELL_KNOWN_CODE_DIR: &str = "/var/lib/virtues/code";
/// Dev-only, relative to virtues-core, like the lake's (`data/` is gitignored).
const DEV_CODE_DIR_FROM_CORE: &str = "../data/code";

/// `VIRTUES_CODE_DIR`, else the box path when `/var/lib/virtues` exists, else
/// the dev path. Same precedence as `storage::lake::lake_root`.
pub fn code_root() -> PathBuf {
    if let Ok(dir) = std::env::var("VIRTUES_CODE_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    let installed = PathBuf::from(WELL_KNOWN_CODE_DIR);
    if installed.parent().is_some_and(|p| p.is_dir()) {
        return installed;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DEV_CODE_DIR_FROM_CORE)
}

// ─── The environment ────────────────────────────────────────────────────────

fn env_dir() -> PathBuf {
    let digest = hex::encode(Sha256::digest(LOCK.as_bytes()));
    code_root().join(format!("python-{}", &digest[..12]))
}

/// The built environment, or None while it is missing or still building.
pub fn ready_env() -> Option<PathBuf> {
    let dir = env_dir();
    dir.join(".complete").is_file().then_some(dir)
}

struct BuildState {
    building: bool,
    failed_at: Option<Instant>,
}

static BUILD: Mutex<BuildState> = Mutex::new(BuildState { building: false, failed_at: None });

/// Start building the environment if it is missing and not already underway.
/// Returns at once; a caller that needs it now checks [`ready_env`].
pub fn ensure_env() {
    let Ok(runtime) = tokio::runtime::Handle::try_current() else { return };
    if ready_env().is_some() {
        return;
    }
    {
        let mut state = BUILD.lock().unwrap_or_else(|e| e.into_inner());
        if state.building || state.failed_at.is_some_and(|t| t.elapsed() < RETRY_AFTER) {
            return;
        }
        state.building = true;
    }
    runtime.spawn(async {
        let started = Instant::now();
        let result = build_env().await;
        let mut state = BUILD.lock().unwrap_or_else(|e| e.into_inner());
        state.building = false;
        match result {
            Ok(()) => {
                state.failed_at = None;
                tracing::info!(secs = started.elapsed().as_secs(), "code_interpreter packages installed");
            }
            Err(e) => {
                state.failed_at = Some(Instant::now());
                tracing::warn!("code_interpreter packages could not be installed: {e}");
            }
        }
    });
}

async fn build_env() -> Result<(), String> {
    let root = code_root();
    make_dir(&root, 0o700)?;
    let dest = env_dir();
    let name = dest.file_name().and_then(|n| n.to_str()).unwrap_or("python");
    let partial = root.join(format!("{name}.partial"));
    if partial.exists() {
        std::fs::remove_dir_all(&partial).map_err(|e| format!("clearing {}: {e}", partial.display()))?;
    }

    run_step(
        Command::new("python3").args(["-m", "venv", "--without-pip"]).arg(&partial),
        Duration::from_secs(120),
        "python3 -m venv",
    )
    .await?;

    let wheel = partial.join("pip.whl");
    let bytes = crate::http_client::base_builder()
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| format!("http client: {e}"))?
        .get(PIP_WHEEL_URL)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("downloading pip: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("downloading pip: {e}"))?;
    let got = hex::encode(Sha256::digest(&bytes));
    if got != PIP_WHEEL_SHA256 {
        return Err(format!("pip wheel hash mismatch: got {got}"));
    }
    std::fs::write(&wheel, &bytes).map_err(|e| format!("writing pip wheel: {e}"))?;

    let lock = partial.join("requirements.lock");
    std::fs::write(&lock, LOCK).map_err(|e| format!("writing lock: {e}"))?;

    run_step(
        Command::new(partial.join("bin").join("python3"))
            .arg(wheel.join("pip"))
            .args([
                "install",
                "--quiet",
                "--no-deps",
                "--require-hashes",
                "--only-binary=:all:",
                "--no-cache-dir",
                "--disable-pip-version-check",
                "--no-input",
                "-r",
            ])
            .arg(&lock),
        Duration::from_secs(1800),
        "pip install",
    )
    .await?;

    let _ = std::fs::remove_file(&wheel);
    std::fs::write(partial.join(".complete"), "").map_err(|e| format!("marking complete: {e}"))?;
    std::fs::rename(&partial, &dest).map_err(|e| format!("moving into place: {e}"))?;

    // The environments of earlier locks.
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            let path = entry.path();
            let stale = path != dest
                && path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("python-"));
            if stale {
                let _ = std::fs::remove_dir_all(&path);
            }
        }
    }
    Ok(())
}

async fn run_step(cmd: &mut Command, limit: Duration, what: &str) -> Result<(), String> {
    cmd.kill_on_drop(true);
    let out = tokio::time::timeout(limit, cmd.output())
        .await
        .map_err(|_| format!("{what} timed out"))?
        .map_err(|e| format!("{what}: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let tail: String = stderr.lines().rev().take(5).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" | ");
    Err(format!("{what} exited {}: {tail}", out.status))
}

// ─── Workspaces ─────────────────────────────────────────────────────────────

/// Where a chat's workspace lives, or None for an id that is not a plain
/// identifier — it becomes a path, so nothing that could climb out of it.
pub fn chat_workspace_dir(chat_id: &str) -> Option<PathBuf> {
    let plain = !chat_id.is_empty()
        && chat_id.len() <= 128
        && chat_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    plain.then(|| code_root().join("chats").join(chat_id))
}

/// A file in a chat's workspace by its workspace-relative path, or None for
/// one that does not exist or would resolve outside it (`..`, a symlink the
/// sandbox left pointing elsewhere).
pub fn workspace_file(chat_id: &str, rel: &str) -> Option<PathBuf> {
    let root = chat_workspace_dir(chat_id)?.canonicalize().ok()?;
    let plain = rel.split('/').all(|c| !c.is_empty() && c != "." && c != "..");
    if !plain {
        return None;
    }
    let file = root.join(rel).canonicalize().ok()?;
    (file.starts_with(&root) && file.is_file()).then_some(file)
}

/// Delete a chat's workspace. Best-effort: the chat is already gone.
pub fn remove_chat_workspace(chat_id: &str) {
    if let Some(dir) = chat_workspace_dir(chat_id) {
        if dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(&dir) {
                tracing::warn!(chat_id, "could not delete the chat's code workspace: {e}");
            }
        }
    }
}

/// The directory one run sees as `/work`.
pub struct Workspace {
    pub dir: PathBuf,
    /// The chat whose workspace this is; None for a throwaway one.
    pub chat_id: Option<String>,
}

impl Workspace {
    /// The saved chat's workspace, created on first use.
    pub fn for_chat(chat_id: &str) -> Result<Self, String> {
        let dir = chat_workspace_dir(chat_id).ok_or_else(|| format!("not a chat id: {chat_id}"))?;
        prepare(&dir)?;
        Ok(Self { dir, chat_id: Some(chat_id.to_string()) })
    }

    /// A workspace deleted when this value is dropped.
    pub fn temporary() -> Result<Self, String> {
        let dir = code_root().join("tmp").join(uuid::Uuid::new_v4().to_string());
        prepare(&dir)?;
        Ok(Self { dir, chat_id: None })
    }

    pub fn out_dir(&self) -> PathBuf {
        self.dir.join("out")
    }

    /// Bytes held, counted by walking it.
    pub fn bytes(&self) -> u64 {
        fn walk(p: &Path) -> u64 {
            let Ok(entries) = std::fs::read_dir(p) else { return 0 };
            entries
                .flatten()
                .map(|e| match e.metadata() {
                    Ok(m) if m.is_dir() => walk(&e.path()),
                    Ok(m) => m.len(),
                    Err(_) => 0,
                })
                .sum()
        }
        walk(&self.dir)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        if self.chat_id.is_none() {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

fn prepare(dir: &Path) -> Result<(), String> {
    let root = code_root();
    make_dir(&root, 0o700)?;
    if let Some(parent) = dir.parent() {
        make_dir(parent, 0o700)?;
    }
    make_dir(dir, 0o777)?;
    make_dir(&dir.join("out"), 0o777)
}

fn make_dir(dir: &Path, mode: u32) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode))
            .map_err(|e| format!("chmod {}: {e}", dir.display()))?;
    }
    #[cfg(not(unix))]
    let _ = mode;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chat_id_that_could_climb_out_gets_no_workspace() {
        assert!(chat_workspace_dir("../etc").is_none());
        assert!(chat_workspace_dir("a/b").is_none());
        assert!(chat_workspace_dir("").is_none());
        assert!(chat_workspace_dir("chat_01H-xyz").is_some());
    }

    #[test]
    fn a_workspace_file_never_resolves_outside_the_workspace() {
        let id = format!("test-{}", uuid::Uuid::new_v4());
        let ws = Workspace::for_chat(&id).unwrap();
        std::fs::write(ws.out_dir().join("a.png"), b"x").unwrap();
        assert!(workspace_file(&id, "out/a.png").is_some());
        assert!(workspace_file(&id, "out/../out/a.png").is_none());
        assert!(workspace_file(&id, "/etc/passwd").is_none());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/etc/hosts", ws.dir.join("escape")).unwrap();
            assert!(workspace_file(&id, "escape").is_none());
        }
        remove_chat_workspace(&id);
        assert!(!ws.dir.exists());
    }

    #[test]
    fn the_lock_is_hash_pinned_throughout() {
        // Every requirement line is followed by hashes; pip refuses a mixed
        // file under --require-hashes, so this is what keeps the build honest.
        let reqs = LOCK.lines().filter(|l| !l.starts_with(' ') && !l.starts_with('#') && !l.is_empty());
        for line in reqs {
            assert!(line.contains("=="), "unpinned: {line}");
        }
        assert!(LOCK.contains("--hash=sha256:"));
    }

    /// Builds the real environment: downloads pip and every locked package.
    /// Run by hand after changing the lock or the build steps:
    /// `VIRTUES_CODE_DIR=$(mktemp -d) cargo test -p virtues --lib code_env -- --ignored`
    #[tokio::test]
    #[ignore = "downloads ~400 MB from PyPI"]
    async fn builds_the_environment() {
        build_env().await.unwrap();
        let env = ready_env().expect("the build marks the environment complete");
        let out = std::process::Command::new(env.join("bin").join("python3"))
            .args(["-I", "-c", "import numpy, pandas, scipy, matplotlib, numpy_financial"])
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    }

    #[test]
    fn a_temporary_workspace_goes_when_dropped() {
        let ws = Workspace::temporary().unwrap();
        let dir = ws.dir.clone();
        assert!(dir.join("out").is_dir());
        drop(ws);
        assert!(!dir.exists());
    }
}

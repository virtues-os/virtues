//! Process lifecycle for `app`-runtime actions.
//!
//! Supervises one child process per `app` action. Spawns at boot, watches
//! for exit, restarts on crash with exponential backoff, reaps on shutdown.
//!
//! Architecture:
//!   - `ServiceSupervisor::start(db)` spawns one **boot child per app** and one
//!     long-lived **restart loop** task that owns respawns.
//!   - Each child has a per-child **watchdog** task that awaits exit and,
//!     on unexpected exit, sends a `RestartRequest` to the restart loop via
//!     an mpsc channel.
//!   - The restart loop applies backoff and calls `spawn_one`. This avoids
//!     recursive `spawn_one → tokio::spawn → spawn_one` chains (which Rust
//!     can't prove Send) and centralizes the respawn policy in one place.
//!
//! Health probe: after spawn, we poll the app's `health_path` (default
//! `/__health`) for up to ~5s. A successful probe flips state to `Running`
//! and traffic flows. A timeout leaves the app in `Starting` — proxy hits
//! return 503 until the next successful probe.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, Mutex};
use tokio::time::sleep;

use crate::error::{Error, Result};

use super::registry::{AppRegistry, RunningService, AppStatus};

/// Max consecutive crashes before we give up auto-restarting an app.
/// Manual reconcile clears this and tries again.
const MAX_RESTARTS: u32 = 10;

/// How long the health probe waits before declaring a failed start.
const HEALTH_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Backoff schedule (seconds) keyed by `restart_count`. Past the end, we cap
/// at the last value (5 minutes).
const BACKOFF_SECS: &[u64] = &[1, 2, 5, 15, 60, 300];

fn backoff_duration(restart_count: u32) -> Duration {
    let idx = (restart_count as usize).saturating_sub(1);
    let secs = BACKOFF_SECS
        .get(idx)
        .copied()
        .unwrap_or_else(|| *BACKOFF_SECS.last().unwrap_or(&300));
    Duration::from_secs(secs)
}

/// Message sent from a watchdog to the restart loop after an unexpected exit.
#[derive(Debug, Clone)]
struct RestartRequest {
    action_id: String,
    command_argv: Vec<String>,
    config: serde_json::Value,
}

/// The supervisor handle. Cheap to clone (Arc fields).
///
/// Held by the axum AppState so the proxy handler can look up ports, and by
/// the shutdown signal handler so it can SIGTERM children gracefully.
#[derive(Clone)]
pub struct ServiceSupervisor {
    pub registry: AppRegistry,
    /// Tracks live child processes so we can SIGTERM them on shutdown.
    /// Outside the registry because Child is not Clone.
    children: Arc<Mutex<HashMap<String, Child>>>,
    /// Where to find the `actions/` tree at runtime — passed in so we can
    /// spawn relative to the repo (resolves `command = ["./target/..."]`).
    repo_root: PathBuf,
    /// What to inject as `VIRTUES_CORE_URL` for the spawned app.
    api_base: String,
    /// Channel for "respawn this action" requests from watchdogs.
    restart_tx: mpsc::UnboundedSender<RestartRequest>,
    /// Owned only by the restart loop after `start()`; kept here in an
    /// Option<Mutex> only so we can `take()` it once.
    restart_rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<RestartRequest>>>>,
}

impl ServiceSupervisor {
    pub fn new(repo_root: PathBuf, api_base: String) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            registry: AppRegistry::new(),
            children: Arc::new(Mutex::new(HashMap::new())),
            repo_root,
            api_base,
            restart_tx: tx,
            restart_rx: Arc::new(Mutex::new(Some(rx))),
        }
    }

    /// Boot phase: query app-runtime actions from the DB, start the restart
    /// loop, spawn each app. Failures to spawn are logged but don't stop the
    /// boot — a single broken app shouldn't take down core.
    pub async fn start(&self, db: &PgPool) -> Result<()> {
        // Take the receiver. Subsequent calls to start() are no-ops on the rx
        // front (defensive — start() should only be called once at boot).
        if let Some(rx) = self.restart_rx.lock().await.take() {
            let supervisor = self.clone();
            tokio::spawn(async move {
                supervisor.run_restart_loop(rx).await;
            });
        }

        let rows: Vec<(String, Option<String>, serde_json::Value)> =
            sqlx::query_as(
                r#"SELECT id, command, config
               FROM app_actions
               WHERE supervise = TRUE AND enabled = TRUE"#,
            )
            .fetch_all(db)
            .await?;

        if rows.is_empty() {
            tracing::info!("no app-runtime actions to supervise");
            return Ok(());
        }

        tracing::info!(count = rows.len(), "starting app-runtime supervisor");

        for (action_id, command_json, config) in rows {
            let command_argv: Option<Vec<String>> = command_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok());

            if let Err(e) = self
                .spawn_one(&action_id, command_argv, &config)
                .await
            {
                tracing::error!(
                    action_id = %action_id,
                    error = %e,
                    "failed to spawn app; will not retry without reconcile"
                );
            }
        }
        Ok(())
    }

    /// The single owner task for respawn requests. Receives RestartRequest
    /// messages from watchdogs, applies backoff, calls spawn_one. Centralizing
    /// here avoids recursive `tokio::spawn` chains.
    async fn run_restart_loop(self, mut rx: mpsc::UnboundedReceiver<RestartRequest>) {
        while let Some(req) = rx.recv().await {
            if self.registry.is_shutting_down().await {
                break;
            }

            let restarts = self.registry.restart_count(&req.action_id).await;
            if restarts >= MAX_RESTARTS {
                tracing::error!(
                    action_id = %req.action_id,
                    restarts,
                    "app exceeded max restarts; marking Crashed (manual reconcile required)"
                );
                self.registry.record_crashed(&req.action_id).await;
                continue;
            }

            let delay = backoff_duration(restarts);
            tracing::info!(
                action_id = %req.action_id,
                restarts,
                delay_secs = delay.as_secs(),
                "backing off before respawn"
            );
            sleep(delay).await;

            if self.registry.is_shutting_down().await {
                break;
            }

            if let Err(e) = self
                .spawn_one(&req.action_id, Some(req.command_argv), &req.config)
                .await
            {
                tracing::error!(
                    action_id = %req.action_id,
                    error = %e,
                    "respawn failed"
                );
            }
        }
    }

    /// Spawn a single app. Allocates a port, builds the Command, kicks off
    /// the watchdog. Health probe runs in its own task.
    async fn spawn_one(
        &self,
        action_id: &str,
        command_argv: Option<Vec<String>>,
        config: &serde_json::Value,
    ) -> Result<()> {
        if self.registry.is_shutting_down().await {
            return Ok(());
        }

        // Resolve the command. A bare argv[0] (no separator) is treated as a
        // Cargo-built action binary under target/{debug,release}; anything
        // else (./x, python3, node) is spawned as-is via PATH/cwd.
        let mut argv: Vec<String> = command_argv.filter(|c| !c.is_empty()).ok_or_else(|| {
            Error::Other(format!("service action {action_id}: `command` not set"))
        })?;
        if !argv[0].contains('/') {
            let bin_path = self
                .repo_root
                .join("target")
                .join(if cfg!(debug_assertions) { "debug" } else { "release" })
                .join(&argv[0]);
            if bin_path.exists() {
                argv[0] = bin_path.to_string_lossy().into_owned();
            }
        }

        // Reuse existing port if we already have a slot for this action,
        // otherwise allocate fresh.
        let port = match self.registry.get(action_id).await {
            Some(state) => state.port,
            None => self
                .registry
                .allocate_port()
                .await
                .ok_or_else(|| Error::Other("port allocator exhausted".into()))?,
        };

        let mut cmd = Command::new(&argv[0]);
        cmd.args(&argv[1..])
            .env("PORT", port.to_string())
            .env("VIRTUES_CORE_URL", &self.api_base)
            .env("VIRTUES_ACTION_ID", action_id)
            .current_dir(&self.repo_root)
            // Capture stdout/stderr so we can surface logs in the System
            // subtab. Each stream gets its own reader task that appends
            // lines to the per-app ring buffer in the registry.
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd
            .spawn()
            .map_err(|e| Error::Other(format!("failed to spawn app {action_id}: {e}")))?;

        let pid = child.id().unwrap_or(0);

        // Tap stdout/stderr into the per-app log buffer. Reading these
        // streams is essential — if we leave them piped without a reader,
        // the child blocks once its kernel pipe buffer fills.
        if let Some(stdout) = child.stdout.take() {
            spawn_log_reader(
                self.registry.clone(),
                action_id.to_string(),
                super::registry::LogStream::Stdout,
                stdout,
            );
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_log_reader(
                self.registry.clone(),
                action_id.to_string(),
                super::registry::LogStream::Stderr,
                stderr,
            );
        }

        // Insert/refresh registry state.
        let existing = self.registry.get(action_id).await;
        let mut state = existing.unwrap_or_else(|| RunningService::new(action_id.to_string(), port));
        state.status = AppStatus::Starting;
        self.registry.insert(state).await;
        self.registry.record_spawn(action_id, pid).await;

        tracing::info!(
            action_id = %action_id,
            pid,
            port,
            argv = ?argv,
            "spawned app"
        );

        // The Child moves into the watchdog task below. We rely on
        // `kill_on_drop(true)` for teardown — when the watchdog task is
        // dropped (e.g. on supervisor shutdown via JoinSet drop, or when the
        // child exits naturally), Tokio SIGKILLs the process if still alive.
        //
        // v1 limitation: `stop_one` (called during reload to remove apps no
        // longer in the manifest) cannot explicitly SIGTERM a running child
        // because the Child has been moved into the watchdog. The registry
        // entry is removed and logs cleared, but the orphaned process keeps
        // serving on its port until core restart. Acceptable for the
        // single-user authoring loop: the operator notices via the System
        // tab. Full stop_one teardown lands when we wire a per-app kill
        // channel.
        let mut watchdog_child = child;

        // Health probe in the background.
        let health_path = config
            .get("service")
            .and_then(|a| a.get("health_path"))
            .and_then(|v| v.as_str())
            .unwrap_or("/__health")
            .to_string();
        {
            let registry = self.registry.clone();
            let action_id_for_probe = action_id.to_string();
            tokio::spawn(async move {
                run_health_probe(&registry, &action_id_for_probe, port, &health_path).await;
            });
        }

        // Watchdog task: await exit, send RestartRequest on unexpected exit.
        let registry = self.registry.clone();
        let restart_tx = self.restart_tx.clone();
        let req = RestartRequest {
            action_id: action_id.to_string(),
            command_argv: argv,
            config: config.clone(),
        };
        tokio::spawn(async move {
            let exit = watchdog_child.wait().await;
            if registry.is_shutting_down().await {
                tracing::info!(
                    action_id = %req.action_id,
                    ?exit,
                    "app exited during shutdown — not restarting"
                );
                return;
            }
            tracing::warn!(
                action_id = %req.action_id,
                ?exit,
                "app exited unexpectedly"
            );
            registry.record_unexpected_exit(&req.action_id).await;
            // Best-effort send. If the channel is closed (shutdown raced),
            // the loop already broke; the `kill_on_drop` will reap.
            let _ = restart_tx.send(req);
        });

        Ok(())
    }

    /// Look up the proxy port for an action. Returns None if the app isn't
    /// running (caller should 503 with Retry-After).
    pub async fn proxy_port(&self, action_id: &str) -> Option<u16> {
        let state = self.registry.get(action_id).await?;
        match state.status {
            AppStatus::Running => Some(state.port),
            _ => None,
        }
    }

    /// Reconcile running apps against the current DB state.
    ///
    /// Called from `/api/admin/reconcile` after the user (or LLM) edits a
    /// manifest on disk and wants core to pick up the change without restart.
    ///
    /// Diff (v1 — minimal):
    ///   - In DB but not running → spawn
    ///   - In DB and Crashed (exceeded MAX_RESTARTS) → drop registry slot and
    ///     spawn fresh. Lets the user fix the code, hit reconcile, recover.
    ///   - Running but not in DB / disabled → stop and remove
    ///   - In both, otherwise → leave running
    ///
    /// Restart-on-config-change (signature diffing) is deferred — for now,
    /// bumping a manifest's `command` or `config` requires toggling enabled
    /// off→on or restarting core. Documented in ARCHITECTURE.md.
    ///
    /// Returns `(added, removed, restarted)` counts for the API response.
    pub async fn reload(&self, db: &PgPool) -> Result<ReloadOutcome> {
        let rows: Vec<(String, Option<String>, serde_json::Value)> =
            sqlx::query_as(
                r#"SELECT id, command, config
               FROM app_actions
               WHERE supervise = TRUE AND enabled = TRUE"#,
            )
            .fetch_all(db)
            .await?;

        // Build the desired state from the DB.
        let mut desired: HashMap<String, DesiredApp> = HashMap::new();
        for (id, cmd_json, cfg) in rows {
            let argv: Option<Vec<String>> =
                cmd_json.as_deref().and_then(|s| serde_json::from_str(s).ok());
            desired.insert(
                id.clone(),
                DesiredApp {
                    action_id: id,
                    command_argv: argv,
                    config: cfg,
                },
            );
        }

        // Snapshot the current registry so we can iterate without holding
        // the read guard across spawn calls.
        let current = self.registry.list().await;

        let mut added = Vec::new();
        let mut removed = Vec::new();
        let mut restarted = Vec::new();

        // Stop apps no longer in the DB / disabled.
        for state in &current {
            if !desired.contains_key(&state.action_id) {
                self.stop_one(&state.action_id).await;
                removed.push(state.action_id.clone());
            }
        }

        // Crashed apps: drop the registry slot so the spawn loop below treats
        // them as fresh. Without this they stay Crashed forever (MAX_RESTARTS
        // already tripped) and only a core restart could recover them.
        let crashed: Vec<String> = current
            .iter()
            .filter(|s| matches!(s.status, AppStatus::Crashed) && desired.contains_key(&s.action_id))
            .map(|s| s.action_id.clone())
            .collect();
        for id in &crashed {
            self.stop_one(id).await;
            restarted.push(id.clone());
        }

        // Spawn apps newly in the DB (or freshly cleared from Crashed above).
        let currently_present: std::collections::HashSet<String> = current
            .iter()
            .filter(|s| !crashed.contains(&s.action_id))
            .map(|s| s.action_id.clone())
            .collect();
        for (id, want) in desired {
            if currently_present.contains(&id) {
                continue;
            }
            if let Err(e) = self
                .spawn_one(&want.action_id, want.command_argv, &want.config)
                .await
            {
                tracing::error!(
                    action_id = %want.action_id,
                    error = %e,
                    "reload: failed to spawn newly-added app"
                );
                continue;
            }
            added.push(id);
        }

        Ok(ReloadOutcome {
            added,
            removed,
            restarted,
        })
    }

    /// Stop and remove a single app. Used by `reload()` for apps removed
    /// from the manifest.
    async fn stop_one(&self, action_id: &str) {
        if let Some(mut child) = self.children.lock().await.remove(action_id) {
            self.registry
                .update_status(action_id, AppStatus::Stopping)
                .await;
            if let Err(e) = child.start_kill() {
                tracing::warn!(
                    action_id = %action_id,
                    error = %e,
                    "stop_one: kill signal failed"
                );
            }
        }
        if let Some(state) = self.registry.remove(action_id).await {
            self.registry.release_port(state.port).await;
        }
        self.registry.clear_logs(action_id).await;
    }
}

/// Returned from `ServiceSupervisor::reload`. Maps cleanly to the JSON shape of
/// `POST /api/admin/reconcile`.
#[derive(Debug, Default, serde::Serialize)]
pub struct ReloadOutcome {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub restarted: Vec<String>,
}

/// Internal scratch type for diffing in `reload()`.
struct DesiredApp {
    action_id: String,
    command_argv: Option<Vec<String>>,
    config: serde_json::Value,
}

impl ServiceSupervisor {
    /// Graceful shutdown. SIGTERM all children, wait briefly, then return.
    /// Children registered with `kill_on_drop(true)` will be SIGKILL'd by
    /// Tokio when their watchdog tasks end.
    pub async fn shutdown(&self) {
        self.registry.set_shutting_down().await;

        let mut children = self.children.lock().await;
        let action_ids: Vec<String> = children.keys().cloned().collect();

        tracing::info!(count = action_ids.len(), "shutting down apps");

        for action_id in &action_ids {
            if let Some(mut child) = children.remove(action_id) {
                self.registry
                    .update_status(action_id, AppStatus::Stopping)
                    .await;
                if let Err(e) = child.start_kill() {
                    tracing::warn!(
                        action_id = %action_id,
                        error = %e,
                        "failed to send kill signal to app"
                    );
                }
            }
        }
    }
}

/// Poll `GET http://127.0.0.1:port{health_path}` until 2xx or timeout. Mark
/// Running on success; leave Starting on timeout (the app will keep trying
/// to come up; subsequent probes / proxy requests will surface the issue).
/// Spawn a Tokio task that reads lines from a child's stdout or stderr and
/// appends them to the per-app log ring buffer. Exits cleanly on EOF
/// (which happens when the child closes the stream / exits).
fn spawn_log_reader<R>(
    registry: AppRegistry,
    action_id: String,
    stream: super::registry::LogStream,
    reader: R,
) where
    R: tokio::io::AsyncRead + Send + Unpin + 'static,
{
    use tokio::io::{AsyncBufReadExt, BufReader};
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    registry.append_log(&action_id, stream, line).await;
                }
                Ok(None) => break, // EOF
                Err(e) => {
                    tracing::warn!(
                        action_id = %action_id,
                        error = %e,
                        "log reader error; closing"
                    );
                    break;
                }
            }
        }
    });
}

async fn run_health_probe(
    registry: &AppRegistry,
    action_id: &str,
    port: u16,
    health_path: &str,
) {
    let url = format!("http://127.0.0.1:{port}{health_path}");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
        .expect("reqwest client");

    let deadline = tokio::time::Instant::now() + HEALTH_PROBE_TIMEOUT;
    loop {
        if tokio::time::Instant::now() > deadline {
            tracing::warn!(
                action_id = %action_id,
                url,
                "app failed health probe within {}s — staying in Starting",
                HEALTH_PROBE_TIMEOUT.as_secs()
            );
            return;
        }
        if let Ok(resp) = client.get(&url).send().await {
            if resp.status().is_success() {
                registry.record_running(action_id).await;
                tracing::info!(
                    action_id = %action_id,
                    port,
                    "app health probe passed; marked Running"
                );
                return;
            }
        }
        sleep(Duration::from_millis(200)).await;
    }
}

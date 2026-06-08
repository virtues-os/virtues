//! In-memory registry of running `app`-runtime children.
//!
//! Keyed by `action_id`. Reads and writes go through a `tokio::sync::RwLock`
//! so reconcile (Phase 4) and the proxy handler can both read concurrently
//! without contention.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::Arc;

use tokio::sync::RwLock;

/// How many recent log lines to retain per app (in-memory ring buffer).
/// Older lines are dropped when the buffer fills. v1.1 will persist to disk;
/// for now, restart wipes history.
const LOG_RING_SIZE: usize = 1000;

/// A single captured stdout/stderr line.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LogLine {
    pub stream: LogStream,
    pub line: String,
    /// Wall-clock time the line was captured. ISO-8601.
    pub at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogStream {
    Stdout,
    Stderr,
}

/// Lifecycle state of a single supervised app.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum AppStatus {
    /// Process spawned, port assigned; awaiting health-probe readiness.
    Starting,
    /// Health probe passed; proxy will forward traffic.
    Running,
    /// Process exited unexpectedly; awaiting backoff before respawn.
    Backoff,
    /// Process exited too many times in a row; manual reconcile required.
    Crashed,
    /// Supervisor is shutting down (SIGTERM sent, awaiting reap).
    Stopping,
}

/// One row in the in-memory registry.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RunningService {
    pub action_id: String,
    pub port: u16,
    pub pid: Option<u32>,
    pub status: AppStatus,
    /// ISO-8601 timestamp of the most recent successful spawn.
    pub started_at: Option<String>,
    /// Number of unexpected exits since boot. Resets when status flips to
    /// `Running` after a successful health probe.
    pub restart_count: u32,
}

impl RunningService {
    pub fn new(action_id: String, port: u16) -> Self {
        Self {
            action_id,
            port,
            pid: None,
            status: AppStatus::Starting,
            started_at: None,
            restart_count: 0,
        }
    }
}

/// Cloneable handle to the supervisor's state. Cheap to clone (Arc).
#[derive(Clone, Default)]
pub struct AppRegistry {
    inner: Arc<RwLock<RegistryInner>>,
}

#[derive(Default)]
struct RegistryInner {
    /// `action_id → RunningService`.
    apps: HashMap<String, RunningService>,
    /// Allocated ports — for cheap "next free port" allocation.
    used_ports: BTreeSet<u16>,
    /// Hard global flag: reconcile + supervisor task short-circuit when set.
    shutting_down: bool,
    /// Per-app ring buffer of captured stdout/stderr lines. Keyed by
    /// action_id; bounded by `LOG_RING_SIZE`. Cleared when the app row is
    /// removed from the registry (manual reconcile / disable).
    logs: HashMap<String, VecDeque<LogLine>>,
}

const PORT_BASE: u16 = 3100;
const PORT_MAX_OFFSET: u16 = 200;

impl AppRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate the next free port starting at 3100. Returns None if the
    /// 200-port window is exhausted (we'd be running 200 apps; pathological).
    ///
    /// We also probe `127.0.0.1:<port>` with a non-blocking bind+drop to skip
    /// ports that are LISTENing for some other process. Without this, an
    /// orphaned app binary from a previous `cargo run` (the watchdog wasn't
    /// reaped on Ctrl-C, kill_on_drop didn't propagate) deadlocks the
    /// supervisor in a respawn loop trying to bind a port a zombie still
    /// owns.
    pub async fn allocate_port(&self) -> Option<u16> {
        let mut inner = self.inner.write().await;
        for offset in 0..PORT_MAX_OFFSET {
            let port = PORT_BASE + offset;
            if inner.used_ports.contains(&port) {
                continue;
            }
            if !port_is_free(port) {
                tracing::warn!(port, "port held by an external process; skipping");
                continue;
            }
            inner.used_ports.insert(port);
            return Some(port);
        }
        None
    }

    pub async fn release_port(&self, port: u16) {
        let mut inner = self.inner.write().await;
        inner.used_ports.remove(&port);
    }

    pub async fn insert(&self, state: RunningService) {
        let mut inner = self.inner.write().await;
        inner.apps.insert(state.action_id.clone(), state);
    }

    pub async fn remove(&self, action_id: &str) -> Option<RunningService> {
        let mut inner = self.inner.write().await;
        let removed = inner.apps.remove(action_id);
        if let Some(s) = &removed {
            inner.used_ports.remove(&s.port);
        }
        removed
    }

    pub async fn update_status(&self, action_id: &str, status: AppStatus) {
        let mut inner = self.inner.write().await;
        if let Some(state) = inner.apps.get_mut(action_id) {
            state.status = status;
        }
    }

    pub async fn record_spawn(&self, action_id: &str, pid: u32) {
        let mut inner = self.inner.write().await;
        if let Some(state) = inner.apps.get_mut(action_id) {
            state.pid = Some(pid);
            state.started_at = Some(chrono::Utc::now().to_rfc3339());
        }
    }

    pub async fn record_running(&self, action_id: &str) {
        let mut inner = self.inner.write().await;
        if let Some(state) = inner.apps.get_mut(action_id) {
            state.status = AppStatus::Running;
            state.restart_count = 0;
        }
    }

    pub async fn record_unexpected_exit(&self, action_id: &str) {
        let mut inner = self.inner.write().await;
        if let Some(state) = inner.apps.get_mut(action_id) {
            state.pid = None;
            state.restart_count += 1;
            state.status = AppStatus::Backoff;
        }
    }

    pub async fn record_crashed(&self, action_id: &str) {
        let mut inner = self.inner.write().await;
        if let Some(state) = inner.apps.get_mut(action_id) {
            state.status = AppStatus::Crashed;
        }
    }

    pub async fn get(&self, action_id: &str) -> Option<RunningService> {
        let inner = self.inner.read().await;
        inner.apps.get(action_id).cloned()
    }

    pub async fn list(&self) -> Vec<RunningService> {
        let inner = self.inner.read().await;
        let mut out: Vec<RunningService> = inner.apps.values().cloned().collect();
        out.sort_by(|a, b| a.action_id.cmp(&b.action_id));
        out
    }

    pub async fn restart_count(&self, action_id: &str) -> u32 {
        let inner = self.inner.read().await;
        inner
            .apps
            .get(action_id)
            .map(|s| s.restart_count)
            .unwrap_or(0)
    }

    pub async fn set_shutting_down(&self) {
        let mut inner = self.inner.write().await;
        inner.shutting_down = true;
    }

    pub async fn is_shutting_down(&self) -> bool {
        let inner = self.inner.read().await;
        inner.shutting_down
    }

    /// Append a captured log line for an app. Bounded ring buffer:
    /// drops the oldest line when full.
    pub async fn append_log(&self, action_id: &str, stream: LogStream, line: String) {
        let mut inner = self.inner.write().await;
        let buf = inner.logs.entry(action_id.to_string()).or_default();
        if buf.len() >= LOG_RING_SIZE {
            buf.pop_front();
        }
        buf.push_back(LogLine {
            stream,
            line,
            at: chrono::Utc::now().to_rfc3339(),
        });
    }

    /// Snapshot the current ring buffer for an app (oldest → newest).
    pub async fn logs(&self, action_id: &str) -> Vec<LogLine> {
        let inner = self.inner.read().await;
        inner
            .logs
            .get(action_id)
            .map(|buf| buf.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Drop log buffer for an app (called when the app is removed via
    /// reload).
    pub async fn clear_logs(&self, action_id: &str) {
        let mut inner = self.inner.write().await;
        inner.logs.remove(action_id);
    }
}

/// Probe whether `127.0.0.1:<port>` is free by attempting a synchronous bind
/// with `SO_REUSEADDR`. The bind is immediately dropped so the OS releases
/// the socket. Returns `true` if the port is available, `false` if anything
/// else (typically another process LISTENing) holds it.
fn port_is_free(port: u16) -> bool {
    use std::net::{Ipv4Addr, SocketAddr, TcpListener};

    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    match TcpListener::bind(addr) {
        Ok(listener) => {
            // Explicit drop to release immediately; let the OS reclaim before
            // the supervisor's child tries to bind.
            drop(listener);
            true
        }
        Err(_) => false,
    }
}

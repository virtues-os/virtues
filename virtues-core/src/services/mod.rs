//! `app`-runtime supervisor and HTTP proxy.
//!
//! An `app`-runtime action is a long-running HTTP server. Core spawns one
//! child process per `app`-runtime action at boot, watches it, restarts on
//! crash with backoff, and exposes its HTTP at `/service/<action_id>/*` via an
//! axum reverse-proxy handler.
//!
//! ## Why not Docker
//!
//! For self-hosted single-user the supervision needs are: spawn, restart,
//! shutdown, route HTTP. All that comes for free from `tokio::process` +
//! axum + a small port allocator. Docker brings a Linux-VM dependency on
//! macOS, image build pipelines, and registry concerns we don't have.
//! See ARCHITECTURE.md for the full rationale.
//!
//! ## What lives where
//!
//! - [`supervisor::ServiceSupervisor`] — the orchestrator. Holds the registry
//!   handle, spawns boot-time, restarts on crash, shuts down on signal.
//! - [`registry::AppRegistry`] — the in-memory map `action_id → AppState`.
//!   Cloneable handle; backed by an `Arc<RwLock>`.
//! - [`proxy::handle_service_proxy`] — the axum handler for
//!   `/service/:action_id/*path`. Looks up the port, forwards via reqwest.
//!
//! ## App contract
//!
//! Each spawned app receives the following env vars:
//!
//! - `PORT`               — bind here
//! - `VIRTUES_CORE_URL`   — e.g., `http://127.0.0.1:8000`
//! - `VIRTUES_ACTION_ID`  — for log correlation
//!
//! Apps are encouraged (not required) to expose:
//!
//! - `GET  /__health`     — returns 2xx when ready
//! - `POST /__trigger`    — accepts an `ActionInput` JSON body when the
//!                          action is fired by cron / webhook / manual
//!
//! No app token in v1. Apps trust localhost; the API can be called without
//! auth when reached via 127.0.0.1.

pub mod proxy;
pub mod registry;
pub mod supervisor;

pub use registry::{AppRegistry, AppStatus, LogLine, LogStream, RunningService};
pub use supervisor::{ServiceSupervisor, ReloadOutcome};

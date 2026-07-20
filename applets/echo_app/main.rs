//! `echo_app` — example `app`-runtime action.
//!
//! Demonstrates the long-running HTTP server pattern. Core's `AppSupervisor`
//! spawns this binary at boot, allocates a port (passed via `$PORT`), and
//! proxies external HTTP at `/app/<action_id>/*` to it.
//!
//! Endpoints:
//!   GET  /__health   — liveness probe; supervisor calls this at startup.
//!   GET  /hello      — returns a greeting; the canonical "did the proxy work?"
//!   POST /__trigger  — accepts an ActionInput JSON body when the action is
//!                      fired by cron / webhook / manual; echoes it back.
//!
//! ~80 LOC. Pure axum, no virtues helpers. A real app would call
//! `$VIRTUES_CORE_URL` to read/write data_* tables.

use axum::{
    extract::Json,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use serde_json::Value;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    virtues_applets::init_tracing();

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3100);
    let action_id = std::env::var("VIRTUES_ACTION_ID").unwrap_or_default();
    let api_base = std::env::var("VIRTUES_CORE_URL").unwrap_or_default();

    tracing::info!(
        action_id = %action_id,
        api_base = %api_base,
        port,
        "echo_app starting"
    );

    let app = Router::new()
        .route("/__health", get(health))
        .route("/hello", get(hello))
        .route("/__trigger", post(trigger));

    let addr = format!("127.0.0.1:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("echo_app listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> StatusCode {
    StatusCode::OK
}

async fn hello() -> impl IntoResponse {
    let action_id = std::env::var("VIRTUES_ACTION_ID").unwrap_or_default();
    Json(serde_json::json!({
        "message": "hello from echo_app",
        "action_id": action_id,
    }))
}

/// Cron / webhook / manual triggers proxied here by the action runner.
/// Body is the standard `ActionInput`; we just echo it back for the demo.
async fn trigger(Json(input): Json<Value>) -> impl IntoResponse {
    tracing::info!(?input, "echo_app /__trigger received");
    Json(serde_json::json!({
        "result": "ok",
        "echoed": input,
    }))
}

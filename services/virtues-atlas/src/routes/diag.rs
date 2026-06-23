//! Diagnostic beacons from Virtues boxes.
//!
//! Two endpoints, both unauthenticated. Box-side runs are best-effort and
//! never have a session to authenticate with — a crashed box can't read
//! its DB to fetch a credential. Instead we rate-limit per `box_id` (the
//! anonymous per-install hash sent in every payload) so a runaway box
//! can't drown the table. Everything is logged at the application level
//! for SRE follow-up; we never proxy this to anything else.

use std::net::SocketAddr;

use axum::{
    extract::{ConnectInfo, State},
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::AppState;

const MAX_BOX_ID_LEN: usize = 40;
const MAX_TAIL_BYTES: usize = 16 * 1024; // 16 KB — last 50 journal lines fit easily

/// Maximum events per `box_id` per hour. A healthy box will send ~0–1
/// install beacons and 0 crash beacons; this cap exists to keep a
/// pathological restart-loop from filling the table. 60/hr ≈ one per
/// minute, plenty of headroom while still bounded.
const RATE_LIMIT_PER_HOUR: i64 = 60;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/diag/install", post(install))
        .route("/diag/crash", post(crash))
}

#[derive(Debug, Deserialize)]
struct InstallBeacon {
    box_id: String,
    distro: Option<String>,
    version: Option<String>,
    arch: Option<String>,
    /// "ok" | "failed"
    outcome: Option<String>,
    /// Set when outcome == failed; one of the install.sh step names.
    failed_step: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CrashBeacon {
    box_id: String,
    version: Option<String>,
    service_result: Option<String>,
    exit_code: Option<String>,
    exit_status: Option<String>,
    journal_tail: Option<String>,
    ts: Option<String>,
}

async fn install(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(beacon): Json<InstallBeacon>,
) -> impl IntoResponse {
    if let Err(resp) = validate_box_id(&beacon.box_id) {
        return resp;
    }
    if let Err(resp) = check_rate_limit(&state, &beacon.box_id).await {
        return resp;
    }
    let payload = json!({
        "distro": beacon.distro,
        "version": beacon.version,
        "arch": beacon.arch,
        "outcome": beacon.outcome,
        "failed_step": beacon.failed_step,
    });
    insert(&state, &beacon.box_id, "install", payload, addr).await
}

async fn crash(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(mut beacon): Json<CrashBeacon>,
) -> impl IntoResponse {
    if let Err(resp) = validate_box_id(&beacon.box_id) {
        return resp;
    }
    if let Err(resp) = check_rate_limit(&state, &beacon.box_id).await {
        return resp;
    }
    if let Some(t) = beacon.journal_tail.as_mut() {
        if t.len() > MAX_TAIL_BYTES {
            // Keep the END of the log — that's where the crash actually
            // happened. Truncating from the front loses what caused it.
            let cut = t.len() - MAX_TAIL_BYTES;
            *t = format!("[…{cut} bytes truncated…]\n{}", &t[cut..]);
        }
    }
    let payload = json!({
        "version": beacon.version,
        "service_result": beacon.service_result,
        "exit_code": beacon.exit_code,
        "exit_status": beacon.exit_status,
        "journal_tail": beacon.journal_tail,
        "ts": beacon.ts,
    });
    insert(&state, &beacon.box_id, "crash", payload, addr).await
}

async fn insert(
    state: &AppState,
    box_id: &str,
    event_type: &'static str,
    payload: Value,
    addr: SocketAddr,
) -> axum::response::Response {
    let res = sqlx::query(
        "INSERT INTO diag_events (box_id, event_type, payload, src_ip) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(box_id)
    .bind(event_type)
    .bind(&payload)
    .bind(addr.ip().to_string())
    .execute(&state.pool)
    .await;
    match res {
        Ok(_) => (StatusCode::ACCEPTED, Json(json!({ "ok": true }))).into_response(),
        Err(e) => {
            tracing::warn!(error = %e, %event_type, %box_id, "diag insert failed");
            // Don't surface internal-error detail to the caller — the box
            // doesn't care and there's nothing it can do.
            (
                StatusCode::ACCEPTED,
                Json(json!({ "ok": true, "stored": false })),
            )
                .into_response()
        }
    }
}

fn validate_box_id(box_id: &str) -> Result<(), axum::response::Response> {
    if box_id.is_empty() || box_id.len() > MAX_BOX_ID_LEN {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid_box_id" })),
        )
            .into_response());
    }
    // box_id is either `i:<hex>` (install) or `h:<hex>` (host fallback)
    // — any other shape is a client we don't recognize.
    if !(box_id.starts_with("i:") || box_id.starts_with("h:")) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid_box_id_prefix" })),
        )
            .into_response());
    }
    Ok(())
}

async fn check_rate_limit(state: &AppState, box_id: &str) -> Result<(), axum::response::Response> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT COUNT(*) FROM diag_events \
         WHERE box_id = $1 AND received_at > now() - interval '1 hour'",
    )
    .bind(box_id)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();
    let n = row.map(|(n,)| n).unwrap_or(0);
    if n >= RATE_LIMIT_PER_HOUR {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({ "error": "rate_limited" })),
        )
            .into_response());
    }
    Ok(())
}

//! Link-a-device rendezvous (fully-remote enrollment).
//!
//! atlas is a **blind coordinator**: it only relays a code HASH, the box's public
//! reach, and the new device's public EndpointId (+ a MAC the box verifies). It
//! never sees the linking code plaintext or the bearer — the bearer flows
//! box→device over iroh at redeem. See docs/reach-enrollment.md.
//!
//! - `POST /link/session`  (box, api_key)  — open a session for code_hash.
//! - `POST /link/lookup`   (new device)    — submit endpoint_id+mac, get box reach.
//! - `POST /link/status`   (box, api_key)  — poll for the device's endpoint_id+mac.
//! - `POST /link/approve`  (box, api_key)  — mark approved (device may now redeem).
//! - `POST /link/result`   (new device)    — poll status until 'approved'.

use axum::{extract::State, response::{IntoResponse, Json}, routing::post, Router};
use serde::Deserialize;
use serde_json::json;

use crate::routes::{credits::resolve_active_customer, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/link/session", post(link_session))
        .route("/link/lookup", post(link_lookup))
        .route("/link/status", post(link_status))
        .route("/link/approve", post(link_approve))
        .route("/link/result", post(link_result))
}

#[derive(Debug, Deserialize)]
struct SessionBody {
    api_key: String,
    code_hash: String,
    box_node_id: String,
    relay_url: String,
    ttl_secs: Option<i32>,
}

/// The box opens a link session. Gated on an active subscription.
async fn link_session(State(state): State<AppState>, Json(b): Json<SessionBody>) -> axum::response::Response {
    let (_customer_id, account_id) = match resolve_active_customer(&state, &b.api_key).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if b.code_hash.trim().is_empty() || b.box_node_id.trim().is_empty() || b.relay_url.trim().is_empty() {
        return bad("missing_fields");
    }
    let ttl = b.ttl_secs.unwrap_or(600).clamp(60, 1800);
    // Opportunistic sweep.
    let _ = sqlx::query("DELETE FROM link_session WHERE expires_at < now()")
        .execute(&state.pool)
        .await;
    if let Err(e) = sqlx::query(
        "INSERT INTO link_session (code_hash, account_id, box_node_id, relay_url, expires_at) \
         VALUES ($1, $2, $3, $4, now() + make_interval(secs => $5)) \
         ON CONFLICT (code_hash) DO UPDATE SET account_id = EXCLUDED.account_id, \
             box_node_id = EXCLUDED.box_node_id, relay_url = EXCLUDED.relay_url, \
             expires_at = EXCLUDED.expires_at, status = 'pending', \
             device_endpoint_id = NULL, mac = NULL \
         WHERE link_session.account_id = EXCLUDED.account_id",
    )
    .bind(b.code_hash.trim())
    .bind(&account_id)
    .bind(b.box_node_id.trim())
    .bind(b.relay_url.trim())
    .bind(ttl)
    .execute(&state.pool)
    .await
    {
        tracing::error!(error = %e, "link_session insert failed");
        return internal();
    }
    (axum::http::StatusCode::OK, Json(json!({"ok": true, "ttl_secs": ttl}))).into_response()
}

#[derive(Debug, Deserialize)]
struct LookupBody {
    code_hash: String,
    endpoint_id: String,
    mac: String,
}

/// The new device submits its (proven-later) EndpointId + MAC and learns the
/// box's public reach so it can dial once approved. Device-blind (code_hash only).
async fn link_lookup(State(state): State<AppState>, Json(b): Json<LookupBody>) -> axum::response::Response {
    if b.endpoint_id.trim().is_empty() || b.mac.trim().is_empty() {
        return bad("missing_fields");
    }
    let row: Option<(String, String)> = sqlx::query_as(
        "UPDATE link_session SET device_endpoint_id = $2, mac = $3, status = 'requested' \
         WHERE code_hash = $1 AND status IN ('pending', 'requested') AND expires_at > now() \
         RETURNING box_node_id, relay_url",
    )
    .bind(b.code_hash.trim())
    .bind(b.endpoint_id.trim())
    .bind(b.mac.trim())
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);
    match row {
        Some((box_node_id, relay_url)) => (
            axum::http::StatusCode::OK,
            Json(json!({"box_node_id": box_node_id, "relay_url": relay_url})),
        )
            .into_response(),
        None => (
            axum::http::StatusCode::NOT_FOUND,
            Json(json!({"error": "not_found_or_expired"})),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct BoxPollBody {
    api_key: String,
    code_hash: String,
}

/// The box polls for the device's submitted EndpointId + MAC.
async fn link_status(State(state): State<AppState>, Json(b): Json<BoxPollBody>) -> axum::response::Response {
    let (_c, account_id) = match resolve_active_customer(&state, &b.api_key).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let row: Option<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT status, device_endpoint_id, mac FROM link_session \
         WHERE code_hash = $1 AND account_id = $2 AND expires_at > now()",
    )
    .bind(b.code_hash.trim())
    .bind(&account_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);
    match row {
        Some((status, endpoint_id, mac)) => (
            axum::http::StatusCode::OK,
            Json(json!({"status": status, "device_endpoint_id": endpoint_id, "mac": mac})),
        )
            .into_response(),
        None => (axum::http::StatusCode::OK, Json(json!({"status": "expired"}))).into_response(),
    }
}

/// The box marks the session approved (after it enrolled the device). The new
/// device's `/link/result` poll then flips to 'approved' and it redeems over iroh.
async fn link_approve(State(state): State<AppState>, Json(b): Json<BoxPollBody>) -> axum::response::Response {
    let (_c, account_id) = match resolve_active_customer(&state, &b.api_key).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let res = sqlx::query(
        "UPDATE link_session SET status = 'approved' \
         WHERE code_hash = $1 AND account_id = $2 AND status = 'requested' AND expires_at > now()",
    )
    .bind(b.code_hash.trim())
    .bind(&account_id)
    .execute(&state.pool)
    .await;
    match res {
        Ok(r) if r.rows_affected() == 1 => (axum::http::StatusCode::OK, Json(json!({"ok": true}))).into_response(),
        Ok(_) => (axum::http::StatusCode::CONFLICT, Json(json!({"error": "not_requested_or_expired"}))).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "link_approve failed");
            internal()
        }
    }
}

#[derive(Debug, Deserialize)]
struct ResultBody {
    code_hash: String,
}

/// The new device polls until the session is 'approved', then redeems over iroh.
async fn link_result(State(state): State<AppState>, Json(b): Json<ResultBody>) -> axum::response::Response {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT status FROM link_session WHERE code_hash = $1 AND expires_at > now()",
    )
    .bind(b.code_hash.trim())
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);
    let status = row.map(|(s,)| s).unwrap_or_else(|| "expired".to_string());
    (axum::http::StatusCode::OK, Json(json!({"status": status}))).into_response()
}

fn bad(code: &str) -> axum::response::Response {
    (axum::http::StatusCode::BAD_REQUEST, Json(json!({"error": code}))).into_response()
}
fn internal() -> axum::response::Response {
    (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal"}))).into_response()
}

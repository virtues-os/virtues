//! Internal routes (internal-secret gated). The entire atlas → virtues-api
//! surface.
//!
//! `POST /internal/device` — atlas registers (or rotates) a device api key
//! for an account. Carries the key hash, the opaque `account_id`, and the
//! per-account daily cap. This is the link/recovery primitive: re-pointing an
//! account to a new key never touches its balance.
//!
//! `POST /internal/credit` — atlas credits an account. `set` overwrites the
//! balance to the monthly allotment (subscription renewal); `add` increments
//! it (top-up). Each lands a `ledger` row so `balance == SUM(ledger)`.
//!
//! `POST /internal/block` / `/internal/unblock` / `GET /internal/blocklist` —
//! ops-only behavioral blocklist control, keyed by an api-key hash from
//! virtues-api's own abuse logs.

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::Duration;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::auth::AuthenticatedRequest;
use crate::entitlement::{self, CreditMode};
use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/internal/device", post(register_device))
        .route("/internal/credit", post(credit_account))
        .route("/internal/block", post(block_key))
        .route("/internal/unblock", post(unblock_key))
        .route("/internal/blocklist", get(blocklist_state))
}

#[derive(Debug, Deserialize)]
struct RegisterDeviceBody {
    /// Lowercase hex of SHA-256(api_key).
    api_key_hash: String,
    /// Opaque per-customer account id (assigned by atlas).
    account_id: String,
    /// The box's iroh EndpointId, when the caller knows it. Scopes rotation to
    /// this box so a sibling box on the same account keeps its key. Absent =
    /// legacy whole-account replacement.
    #[serde(default)]
    box_id: Option<String>,
}

async fn register_device(
    State(state): State<Arc<AppState>>,
    _auth: AuthenticatedRequest,
    Json(body): Json<RegisterDeviceBody>,
) -> impl IntoResponse {
    let Ok(hash) = hex_decode(&body.api_key_hash) else {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_hash",
            "api_key_hash must be lowercase hex",
        );
    };
    match entitlement::register_device(&state.db, &hash, &body.account_id, body.box_id.as_deref())
        .await {
        Ok(()) => (StatusCode::CREATED, Json(json!({ "ok": true }))).into_response(),
        Err(e) => {
            tracing::warn!("register device failed: {e:#}");
            error(StatusCode::INTERNAL_SERVER_ERROR, "register_failed", &e.to_string())
        }
    }
}

#[derive(Debug, Deserialize)]
struct CreditBody {
    account_id: String,
    amount_micros: i64,
    /// "set" = subscription renewal (overwrite balance to amount, fresh
    /// monthly cohort). "add" = top-up (increment).
    mode: String,
    /// Optional reference for the ledger row (e.g. a Stripe invoice/PI id).
    #[serde(default)]
    reference: Option<String>,
}

async fn credit_account(
    State(state): State<Arc<AppState>>,
    _auth: AuthenticatedRequest,
    Json(body): Json<CreditBody>,
) -> impl IntoResponse {
    let mode = match body.mode.as_str() {
        "set" => CreditMode::Set,
        "add" => CreditMode::Add,
        other => {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_mode",
                &format!("mode must be 'set' or 'add', got '{other}'"),
            )
        }
    };
    if body.amount_micros < 0 {
        return error(StatusCode::BAD_REQUEST, "invalid_amount", "amount_micros must be >= 0");
    }
    match entitlement::credit(
        &state.db,
        &body.account_id,
        body.amount_micros,
        mode,
        body.reference.as_deref(),
    )
    .await
    {
        Ok(balance) => {
            (StatusCode::OK, Json(json!({ "ok": true, "balance_micros": balance }))).into_response()
        }
        Err(e) => {
            tracing::warn!("credit account failed: {e:#}");
            error(StatusCode::INTERNAL_SERVER_ERROR, "credit_failed", &e.to_string())
        }
    }
}


/// Introspection: current blocks + the rate "watchlist". Internal-secret gated.
async fn blocklist_state(
    State(state): State<Arc<AppState>>,
    _auth: AuthenticatedRequest,
) -> axum::response::Response {
    (StatusCode::OK, Json(state.blocklist.snapshot())).into_response()
}

#[derive(Debug, Deserialize)]
struct BlockBody {
    /// Lowercase hex of SHA-256(api_key).
    key_hash: String,
    /// Optional cooldown override (seconds). Defaults to the rate-block TTL.
    ttl_seconds: Option<i64>,
}

async fn block_key(
    State(state): State<Arc<AppState>>,
    _auth: AuthenticatedRequest,
    Json(body): Json<BlockBody>,
) -> axum::response::Response {
    let Ok(hash) = hex_decode(&body.key_hash) else {
        return error(StatusCode::BAD_REQUEST, "invalid_hash", "key_hash must be lowercase hex");
    };
    let ttl = body.ttl_seconds.map(Duration::seconds);
    state
        .blocklist
        .block(&state.db, &hash, crate::blocklist::REASON_MANUAL, ttl)
        .await;
    (StatusCode::OK, Json(json!({ "ok": true }))).into_response()
}

#[derive(Debug, Deserialize)]
struct UnblockBody {
    key_hash: String,
}

async fn unblock_key(
    State(state): State<Arc<AppState>>,
    _auth: AuthenticatedRequest,
    Json(body): Json<UnblockBody>,
) -> axum::response::Response {
    let Ok(hash) = hex_decode(&body.key_hash) else {
        return error(StatusCode::BAD_REQUEST, "invalid_hash", "key_hash must be lowercase hex");
    };
    state.blocklist.unblock(&state.db, &hash).await;
    (StatusCode::OK, Json(json!({ "ok": true }))).into_response()
}

fn error(status: StatusCode, code: &str, message: &str) -> axum::response::Response {
    (
        status,
        Json(json!({ "error": { "code": code, "message": message } })),
    )
        .into_response()
}

fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 2 != 0 {
        return Err("odd hex length".into());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

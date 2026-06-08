//! Internal routes (internal-secret gated).
//!
//! `POST /internal/voucher` — Atlas registers a freshly minted voucher.
//! The payload carries ONLY the voucher's value (budget, days, expiry) and
//! its code hash. No customer, no bearer. This is the entire Atlas →
//! virtues-api surface: Atlas tells us "a voucher worth X exists"; the
//! device later redeems it. Atlas never sees a bearer; we never see a
//! customer.
//!
//! `POST /internal/block` / `POST /internal/unblock` — ops-only behavioral
//! blocklist control. The caller supplies a `bearer_hash` it learned from
//! *virtues-api's own* abuse logs (never from Atlas — Atlas has no bearer to
//! send, by construction). Gated by the same internal secret.

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::auth::AuthenticatedRequest;
use crate::voucher::{self, RegisterVoucher};
use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/internal/voucher", post(register_voucher))
        .route("/internal/block", post(block_bearer))
        .route("/internal/unblock", post(unblock_bearer))
        .route("/internal/blocklist", get(blocklist_state))
}

/// Introspection: current blocks + the rate "watchlist" (bearers that have
/// exceeded the ceiling). Lets us watch the would-block signal while
/// enforcement is off. Internal-secret gated.
async fn blocklist_state(
    State(state): State<Arc<AppState>>,
    _auth: AuthenticatedRequest,
) -> axum::response::Response {
    (StatusCode::OK, Json(state.blocklist.snapshot())).into_response()
}

#[derive(Debug, Deserialize)]
struct BlockBody {
    /// Lowercase hex of SHA-256(bearer).
    bearer_hash: String,
    /// Optional cooldown override (seconds). Defaults to the rate-block TTL.
    ttl_seconds: Option<i64>,
}

async fn block_bearer(
    State(state): State<Arc<AppState>>,
    _auth: AuthenticatedRequest,
    Json(body): Json<BlockBody>,
) -> axum::response::Response {
    let Some(pool) = state.db.as_ref() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "db_unavailable",
            "VIRTUES_API_DATABASE_URL not configured",
        );
    };
    let Ok(hash) = hex_decode(&body.bearer_hash) else {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_hash",
            "bearer_hash must be lowercase hex",
        );
    };
    let ttl = body.ttl_seconds.map(Duration::seconds);
    state
        .blocklist
        .block(pool, &hash, crate::blocklist::REASON_MANUAL, ttl)
        .await;
    (StatusCode::OK, Json(json!({ "ok": true }))).into_response()
}

#[derive(Debug, Deserialize)]
struct UnblockBody {
    bearer_hash: String,
}

async fn unblock_bearer(
    State(state): State<Arc<AppState>>,
    _auth: AuthenticatedRequest,
    Json(body): Json<UnblockBody>,
) -> axum::response::Response {
    let Some(pool) = state.db.as_ref() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "db_unavailable",
            "VIRTUES_API_DATABASE_URL not configured",
        );
    };
    let Ok(hash) = hex_decode(&body.bearer_hash) else {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_hash",
            "bearer_hash must be lowercase hex",
        );
    };
    state.blocklist.unblock(pool, &hash).await;
    (StatusCode::OK, Json(json!({ "ok": true }))).into_response()
}

#[derive(Debug, Deserialize)]
struct RegisterVoucherBody {
    /// Lowercase hex of SHA-256(voucher_code).
    voucher_code_hash: String,
    /// Single voucher amount in micros USD.
    amount_micros: i64,
    /// `true` = sub renewal (overwrite wallet to amount). `false` = top-up (add).
    #[serde(default)]
    is_renewal: bool,
    voucher_expires_at: DateTime<Utc>,
}

async fn register_voucher(
    State(state): State<Arc<AppState>>,
    _auth: AuthenticatedRequest,
    Json(body): Json<RegisterVoucherBody>,
) -> impl IntoResponse {
    let Some(pool) = state.db.as_ref() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "db_unavailable",
            "VIRTUES_API_DATABASE_URL not configured",
        );
    };

    let Ok(hash) = hex_decode(&body.voucher_code_hash) else {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_hash",
            "voucher_code_hash must be lowercase hex",
        );
    };

    match voucher::register(
        pool,
        RegisterVoucher {
            voucher_code_hash: hash,
            amount_micros: body.amount_micros,
            is_renewal: body.is_renewal,
            voucher_expires_at: body.voucher_expires_at,
        },
    )
    .await
    {
        Ok(()) => (StatusCode::CREATED, Json(json!({ "ok": true }))).into_response(),
        Err(e) => {
            tracing::warn!("register voucher failed: {e:#}");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "register_failed",
                &e.to_string(),
            )
        }
    }
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

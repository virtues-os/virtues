//! Voucher redemption (device-facing).
//!
//! `POST /v1/redeem`
//!   Authorization: Bearer <the device's bearer for this month>
//!   Body: { "voucher_code": "..." }
//!
//! The device generates a fresh bearer locally each month, then redeems
//! the voucher it got from Atlas onto that bearer. virtues-api hashes the
//! bearer (the raw value only ever lives on the device), applies the
//! voucher's budget + expiry, and discards which bearer redeemed which
//! voucher.
//!
//! Note: we do NOT use the `BearerAuth` extractor here — the bearer is
//! typically brand-new (no entitlement row yet) or expired (renewal), and
//! BearerAuth would reject it. We extract the raw bearer ourselves and
//! hash it.

use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Json},
    routing::post,
    Router,
};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::Arc;

use crate::voucher::{self, RedeemError};
use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/v1/redeem", post(redeem))
}

#[derive(Debug, Deserialize)]
struct RedeemBody {
    voucher_code: String,
}

async fn redeem(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<RedeemBody>,
) -> axum::response::Response {
    let Some(pool) = state.db.as_ref() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "db_unavailable",
            "entitlement DB not configured",
        );
    };

    let Some(bearer) = extract_bearer(&headers) else {
        return error(
            StatusCode::UNAUTHORIZED,
            "missing_bearer",
            "Authorization: Bearer <token> required",
        );
    };
    let bearer_hash = sha256(bearer.as_bytes());

    match voucher::redeem(pool, &body.voucher_code, &bearer_hash).await {
        Ok(res) => (
            StatusCode::OK,
            Json(json!({
                "redeemed": true,
                "expires_at": res.expires_at,
                "wallet_micros": res.wallet_micros,
            })),
        )
            .into_response(),
        Err(RedeemError::NotFound) => {
            error(StatusCode::NOT_FOUND, "voucher_not_found", "no such voucher")
        }
        Err(RedeemError::AlreadyRedeemed) => error(
            StatusCode::CONFLICT,
            "already_redeemed",
            "voucher already redeemed",
        ),
        Err(RedeemError::Expired) => {
            error(StatusCode::GONE, "voucher_expired", "voucher expired")
        }
        Err(RedeemError::Db(e)) => {
            tracing::warn!("redeem db error: {e:#}");
            error(StatusCode::INTERNAL_SERVER_ERROR, "internal", "redeem failed")
        }
    }
}

fn extract_bearer(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let bearer = raw.strip_prefix("Bearer ")?.trim();
    if bearer.is_empty() {
        None
    } else {
        Some(bearer.to_string())
    }
}

fn sha256(data: &[u8]) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().to_vec()
}

fn error(status: StatusCode, code: &str, message: &str) -> axum::response::Response {
    (
        status,
        Json(json!({ "error": { "code": code, "message": message } })),
    )
        .into_response()
}

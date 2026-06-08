//! Smoke-test endpoints for the bearer-auth + entitlement path.
//!
//! These exercise the full new flow (header → hash → entitlement
//! lookup → optional decrement) without touching any real upstream
//! provider. Permanent canary for verifying the gate is alive.

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::bearer_auth::BearerAuth;
use crate::entitlement::{self, ChargeError};
use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/whoami", get(whoami))
        .route("/v1/charge-test", post(charge_test))
}

/// Returns a non-sensitive summary of the resolved entitlement.
/// Useful for verifying bearer auth + entitlement lookup work end-to-end.
///
/// Returns BOTH pools for debugging; iOS should only surface
/// `wallet_chat_micros` to the user (see project_economic_model memory).
async fn whoami(BearerAuth(ent): BearerAuth) -> impl IntoResponse {
    Json(json!({
        "wallet_micros": ent.wallet_micros,
        "today_spent_micros": ent.today_spent_micros,
        "today_reset_at": ent.today_reset_at,
        "expires_at": ent.expires_at,
    }))
}

#[derive(Deserialize)]
struct ChargeParams {
    /// Cost in micros to deduct (1_000_000 = $1.00). Defaults to 1000 ($0.001).
    cost_micros: Option<i64>,
}

/// Hits the routing + atomic-decrement path. Purpose comes from the
/// `X-Virtues-Purpose` header (default: `user`). Doesn't touch any
/// upstream provider — just exercises the gate's bookkeeping.
async fn charge_test(
    State(state): State<Arc<AppState>>,
    BearerAuth(ent): BearerAuth,
    headers: HeaderMap,
    Query(params): Query<ChargeParams>,
) -> impl IntoResponse {
    let pool = match state.db.as_ref() {
        Some(p) => p,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "error": { "code": "db_unavailable" } })),
            )
                .into_response()
        }
    };

    let cost = params.cost_micros.unwrap_or(1_000); // default $0.001
    if cost <= 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": { "code": "invalid_cost", "message": "cost_micros must be > 0" } })),
        )
            .into_response();
    }

    let _ = &headers; // X-Virtues-Purpose accepted, no-op in v3

    match entitlement::charge(pool, &ent.bearer_hash, cost).await {
        Ok(ok) => (
            StatusCode::OK,
            Json(json!({
                "real_cost_micros": ok.real_micros,
                "billed_micros": ok.billed_micros,
                "wallet_micros": ok.wallet_micros,
            })),
        )
            .into_response(),
        Err(ChargeError::Expired) => (
            StatusCode::PAYMENT_REQUIRED,
            Json(json!({ "error": { "code": "bearer_expired" } })),
        )
            .into_response(),
        Err(ChargeError::InsufficientBudget) => (
            StatusCode::PAYMENT_REQUIRED,
            Json(json!({ "error": { "code": "insufficient_budget" } })),
        )
            .into_response(),
        Err(ChargeError::NotFound) => (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": { "code": "unknown_bearer" } })),
        )
            .into_response(),
        Err(ChargeError::InvalidCost) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": { "code": "invalid_cost" } })),
        )
            .into_response(),
        Err(ChargeError::CallTooExpensive) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": { "code": "call_too_expensive" } })),
        )
            .into_response(),
        Err(ChargeError::DailyCapReached) => (
            StatusCode::PAYMENT_REQUIRED,
            Json(json!({ "error": { "code": "daily_cap_reached" } })),
        )
            .into_response(),
        Err(ChargeError::Db(e)) => {
            tracing::warn!("charge db error: {e:#}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": { "code": "internal" } })),
            )
                .into_response()
        }
    }
}

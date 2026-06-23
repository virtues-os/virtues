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
        .route("/v1/usage", get(usage))
        .route("/v1/charge-test", post(charge_test))
}

/// Balance + recent ledger entries for the authenticated account. Drives the
/// box's billing/usage surface ("here's your balance, here's where it went").
async fn usage(State(state): State<Arc<AppState>>, BearerAuth(acct): BearerAuth) -> impl IntoResponse {
    match entitlement::usage_summary(&state.db, &acct.account_id, 50).await {
        Ok(summary) => (StatusCode::OK, Json(summary)).into_response(),
        Err(e) => {
            tracing::warn!("usage summary failed: {e:#}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": { "code": "internal" } })),
            )
                .into_response()
        }
    }
}

/// Returns a non-sensitive summary of the resolved account (balance, caps).
/// Useful for verifying api_key auth + account lookup work end-to-end.
async fn whoami(BearerAuth(acct): BearerAuth) -> impl IntoResponse {
    Json(json!({
        "balance_micros": acct.balance_micros,
        "today_spent_micros": acct.today_spent_micros,
        "today_reset_at": acct.today_reset_at,
        "expires_at": acct.expires_at,
        "daily_cap_micros": acct.daily_cap_micros,
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
    let pool = &state.db;

    let cost = params.cost_micros.unwrap_or(1_000); // default $0.001
    if cost <= 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": { "code": "invalid_cost", "message": "cost_micros must be > 0" } })),
        )
            .into_response();
    }

    let _ = &headers; // X-Virtues-Purpose accepted, no-op in v3

    match entitlement::charge(pool, &ent.account_id, cost).await {
        Ok(ok) => (
            StatusCode::OK,
            Json(json!({
                "real_cost_micros": ok.real_micros,
                "billed_micros": ok.billed_micros,
                "balance_micros": ok.balance_micros,
            })),
        )
            .into_response(),
        Err(ChargeError::Expired) => (
            StatusCode::PAYMENT_REQUIRED,
            Json(json!({ "error": { "code": "wallet_expired" } })),
        )
            .into_response(),
        Err(ChargeError::InsufficientBudget) => (
            StatusCode::PAYMENT_REQUIRED,
            Json(json!({ "error": { "code": "insufficient_budget" } })),
        )
            .into_response(),
        Err(ChargeError::NotFound) => (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": { "code": "unknown_key" } })),
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

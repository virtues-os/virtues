//! Bearer-auth account endpoints: who am I, and what have I spent.
//!
//! `/v1/charge-test` used to live here too — a smoke endpoint that debited
//! any caller-chosen amount from the bearer's own wallet. It was mounted in
//! production; a leaked device key could drain a wallet in one call. Removed.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use serde_json::json;
use std::sync::Arc;

use crate::bearer_auth::{BearerAuth, BoxKey};
use crate::entitlement;
use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/whoami", get(whoami))
        .route("/v1/usage", get(usage))
}

/// Balance + recent ledger entries for the authenticated account. Drives the
/// box's billing/usage surface ("here's your balance, here's where it went").
/// It needs only the box key: an account with no live wallet reads a zero
/// balance, not an error.
async fn usage(State(state): State<Arc<AppState>>, key: BoxKey) -> impl IntoResponse {
    match entitlement::usage_summary(&state.db, &key.account_id, 50).await {
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

/// Returns a non-sensitive summary of the live wallet (balance, expiry).
/// Useful for verifying api_key auth + wallet lookup work end-to-end.
async fn whoami(BearerAuth(wallet): BearerAuth) -> impl IntoResponse {
    Json(json!({
        "balance_micros": wallet.balance_micros,
        "expires_at": wallet.expires_at,
    }))
}

//! Subscription API
//!
//! Endpoints for checking subscription status and accessing billing portal.
//!
//! Routes:
//! - GET /v1/subscription - Get subscription status for authenticated user
//! - POST /v1/billing/portal - Create a Stripe Customer Portal session (via Atlas)

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;

use crate::auth::AuthenticatedRequest;
use crate::AppState;

// =============================================================================
// Route Handlers
// =============================================================================

/// GET /v1/subscription
/// Returns current subscription status for the authenticated user
async fn get_subscription(
    State(state): State<Arc<AppState>>,
    auth: AuthenticatedRequest,
) -> Response {
    let status = state.subscription.get_status(&auth.user_id);

    tracing::debug!(
        user_id = %auth.user_id,
        status = %status.status,
        is_active = %status.is_active,
        "Subscription status check"
    );

    (StatusCode::OK, Json(status)).into_response()
}

/// POST /v1/billing/portal
/// Creates a Stripe Customer Portal session URL by proxying to Atlas.
/// Returns { "url": "https://billing.stripe.com/session/..." }
///
/// If Atlas is not configured, returns a helpful error.
async fn create_billing_portal(
    State(state): State<Arc<AppState>>,
    _auth: AuthenticatedRequest,
) -> Response {
    let atlas_url = match &state.config.atlas_url {
        Some(url) => url,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": {
                        "message": "Billing portal not available (Atlas not configured)",
                        "code": "atlas_not_configured"
                    }
                })),
            )
                .into_response();
        }
    };

    let atlas_secret = match &state.config.atlas_secret {
        Some(secret) => secret,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": {
                        "message": "Billing portal not available (Atlas secret not configured)",
                        "code": "atlas_not_configured"
                    }
                })),
            )
                .into_response();
        }
    };

    // Resolve subdomain — required for billing portal
    let subdomain = match &state.config.subdomain {
        Some(sub) => sub,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": {
                        "message": "Billing portal not available (subdomain not configured)",
                        "code": "subdomain_not_configured"
                    }
                })),
            )
                .into_response();
        }
    };

    // Proxy to Atlas: POST /api/internal/billing/portal
    let response = state
        .http_client
        .post(format!("{}/api/internal/billing/portal", atlas_url))
        .header("X-Atlas-Secret", atlas_secret)
        .json(&serde_json::json!({
            "subdomain": subdomain,
        }))
        .send()
        .await;

    match response {
        Ok(resp) => {
            let status = resp.status();
            let body_bytes = resp.bytes().await.unwrap_or_default();

            if status.is_success() {
                // Forward Atlas response directly (contains { "url": "..." })
                (
                    StatusCode::OK,
                    [(axum::http::header::CONTENT_TYPE, "application/json")],
                    axum::body::Bytes::from(body_bytes),
                )
                    .into_response()
            } else {
                tracing::warn!(
                    "Atlas billing portal error ({}): {}",
                    status,
                    String::from_utf8_lossy(&body_bytes)
                );
                (
                    StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
                    [(axum::http::header::CONTENT_TYPE, "application/json")],
                    axum::body::Bytes::from(body_bytes),
                )
                    .into_response()
            }
        }
        Err(e) => {
            tracing::error!("Failed to reach Atlas for billing portal: {}", e);
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": {
                        "message": "Failed to reach billing service",
                        "code": "atlas_unreachable"
                    }
                })),
            )
                .into_response()
        }
    }
}

// =============================================================================
// Router
// =============================================================================

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/subscription", get(get_subscription))
        .route("/billing/portal", post(create_billing_portal))
}

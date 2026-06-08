//! Connection Limits API
//!
//! Endpoints for checking source connection limits based on user tier.
//!
//! Routes:
//! - GET /v1/limits/connections - Check connection limit for a source

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::auth::AuthenticatedRequest;
use crate::AppState;

// =============================================================================
// Request/Response Types
// =============================================================================

#[derive(Debug, Deserialize)]
pub struct ConnectionLimitQuery {
    /// Source type to check (e.g., "plaid", "google")
    pub source: String,
    /// Current number of connections for this source (provided by Core)
    #[serde(default)]
    pub current_count: i64,
}

#[derive(Debug, Serialize)]
pub struct LimitsError {
    pub error: LimitsErrorDetails,
}

#[derive(Debug, Serialize)]
pub struct LimitsErrorDetails {
    pub message: String,
    pub code: String,
}

impl LimitsError {
    fn unknown_source(source: &str) -> Self {
        Self {
            error: LimitsErrorDetails {
                message: format!("Unknown source: {}", source),
                code: "unknown_source".to_string(),
            },
        }
    }
}

// =============================================================================
// Route Handlers
// =============================================================================

/// GET /v1/limits/connections
/// Check connection limit for a source based on user's tier
///
/// Query parameters:
/// - source: Source type (e.g., "plaid", "google")
/// - current_count: Current number of connections (optional, default 0)
///
/// Returns: ConnectionLimitResponse with limit, tier, can_add, remaining
async fn check_connection_limit(
    State(state): State<Arc<AppState>>,
    auth: AuthenticatedRequest,
    Query(params): Query<ConnectionLimitQuery>,
) -> Response {
    match state.tier.check_connection_limit(
        &auth.user_id,
        &params.source,
        params.current_count,
    ) {
        Some(response) => {
            tracing::debug!(
                user_id = %auth.user_id,
                source = %params.source,
                tier = %response.tier,
                limit = %response.limit,
                current = %response.current,
                can_add = %response.can_add,
                "Connection limit check"
            );
            (StatusCode::OK, Json(response)).into_response()
        }
        None => {
            tracing::warn!(
                user_id = %auth.user_id,
                source = %params.source,
                "Unknown source in limit check"
            );
            (
                StatusCode::NOT_FOUND,
                Json(LimitsError::unknown_source(&params.source)),
            )
                .into_response()
        }
    }
}

/// GET /v1/limits/tier
/// Get user's current tier
async fn get_tier(
    State(state): State<Arc<AppState>>,
    auth: AuthenticatedRequest,
) -> Response {
    let tier = state.tier.get_tier(&auth.user_id);
    (StatusCode::OK, Json(serde_json::json!({ "tier": tier }))).into_response()
}

/// GET /v1/limits/sources
/// Get all source limits for user's tier
async fn get_all_source_limits(
    State(state): State<Arc<AppState>>,
    auth: AuthenticatedRequest,
) -> Response {
    let tier = state.tier.get_tier(&auth.user_id);
    
    let sources: Vec<_> = virtues_registry::registered_sources()
        .iter()
        .filter(|s| s.enabled)
        .map(|s| {
            let limit = virtues_registry::get_connection_limit(s.name, &tier).unwrap_or(1);
            let is_multi = virtues_registry::is_multi_instance(s.name);
            serde_json::json!({
                "name": s.name,
                "display_name": s.display_name,
                "limit": limit,
                "is_multi_instance": is_multi,
            })
        })
        .collect();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "tier": tier,
            "sources": sources,
        })),
    )
        .into_response()
}

// =============================================================================
// Router
// =============================================================================

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/limits/connections", get(check_connection_limit))
        .route("/limits/tier", get(get_tier))
        .route("/limits/sources", get(get_all_source_limits))
}

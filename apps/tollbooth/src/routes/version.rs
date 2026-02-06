//! Version & Update API
//!
//! Endpoints for checking latest available version and triggering updates.
//! Proxies to Atlas for version info and update orchestration.
//!
//! Routes:
//! - GET /v1/version - Get latest available version (from cache or Atlas)
//! - POST /v1/update - Trigger a rolling update for this tenant (via Atlas)

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

/// GET /v1/version
/// Returns the latest available version from cache (populated by usage reporter)
/// or falls back to a direct Atlas call if cache is empty.
///
/// Response: { version: "abc1234", image: "ghcr.io/virtues-os/virtues-core:abc1234" }
async fn get_version(
    State(state): State<Arc<AppState>>,
    _auth: AuthenticatedRequest,
) -> Response {
    // Try cache first (populated every ~30s by usage reporter)
    if let Some(info) = state.version_cache.get().await {
        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "version": info.version,
                "image": info.image,
            })),
        )
            .into_response();
    }

    // Cache miss — try fetching directly from Atlas if configured
    let (atlas_url, atlas_secret, subdomain) =
        match (&state.config.atlas_url, &state.config.atlas_secret, &state.config.subdomain) {
            (Some(url), Some(secret), Some(sub)) => (url, secret, sub),
            _ => {
                // Standalone mode — no Atlas configured, no version info available.
                // Return 200 so tower_http doesn't log it as an error.
                return (
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "version": null,
                        "image": null,
                    })),
                )
                    .into_response();
            }
        };

    // Proxy to Atlas: GET /api/internal/version
    let response = state
        .http_client
        .get(format!(
            "{}/api/internal/version?subdomain={}",
            atlas_url, subdomain
        ))
        .header("X-Atlas-Secret", atlas_secret)
        .send()
        .await;

    match response {
        Ok(resp) => {
            let status = resp.status();
            let body_bytes = resp.bytes().await.unwrap_or_default();

            if status.is_success() {
                // Parse and cache the result for future requests
                if let Ok(info) =
                    serde_json::from_slice::<crate::version::VersionInfo>(&body_bytes)
                {
                    state.version_cache.set(info).await;
                }

                (
                    StatusCode::OK,
                    [(axum::http::header::CONTENT_TYPE, "application/json")],
                    axum::body::Bytes::from(body_bytes),
                )
                    .into_response()
            } else {
                tracing::warn!(
                    "Atlas version check error ({}): {}",
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
            tracing::error!("Failed to reach Atlas for version check: {}", e);
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": {
                        "message": "Failed to reach version service",
                        "code": "atlas_unreachable"
                    }
                })),
            )
                .into_response()
        }
    }
}

/// POST /v1/update
/// Triggers a rolling update for this tenant via Atlas.
/// Atlas will: backup SQLite → re-submit Nomad job with new image tag.
///
/// Auth guard is required but user ID is ignored (tenant-level action).
async fn trigger_update(
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
                        "message": "Update not available (Atlas not configured)",
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
                        "message": "Update not available (Atlas secret not configured)",
                        "code": "atlas_not_configured"
                    }
                })),
            )
                .into_response();
        }
    };

    let subdomain = match &state.config.subdomain {
        Some(sub) => sub,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": {
                        "message": "Update not available (subdomain not configured)",
                        "code": "subdomain_not_configured"
                    }
                })),
            )
                .into_response();
        }
    };

    tracing::info!("Triggering update for tenant: {}", subdomain);

    // Proxy to Atlas: POST /api/internal/update
    let response = state
        .http_client
        .post(format!("{}/api/internal/update", atlas_url))
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
                tracing::info!("Update triggered successfully for tenant: {}", subdomain);
                (
                    StatusCode::OK,
                    [(axum::http::header::CONTENT_TYPE, "application/json")],
                    axum::body::Bytes::from(body_bytes),
                )
                    .into_response()
            } else {
                tracing::warn!(
                    "Atlas update error ({}): {}",
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
            tracing::error!("Failed to reach Atlas for update: {}", e);
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": {
                        "message": "Failed to reach update service",
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
        .route("/version", get(get_version))
        .route("/update", post(trigger_update))
}

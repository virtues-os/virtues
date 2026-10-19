//! Unsplash search via bearer-auth (WS-6b).
//!
//! No per-call charge (Unsplash itself is free at our volume), but
//! gated to paid-tier bearers. A future iteration adds rate-limited
//! free-tier access per WS-6a's `FreeWithRateLimit` policy.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::post,
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::bearer_auth::BearerAuth;
use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/v1/unsplash/search", post(unsplash_search))
}

#[derive(Debug, Deserialize)]
struct SearchRequest {
    query: String,
    #[serde(default = "default_page")]
    page: u32,
    #[serde(default = "default_per_page")]
    per_page: u32,
}

fn default_page() -> u32 {
    1
}
fn default_per_page() -> u32 {
    20
}

async fn unsplash_search(
    State(state): State<Arc<AppState>>,
    BearerAuth(_ent): BearerAuth,
    Json(request): Json<SearchRequest>,
) -> axum::response::Response {
    // A valid (non-expired) bearer is all that's required — Unsplash is
    // free for us, so no budget charge. BearerAuth already enforced expiry.
    let Some(access_key) = state.config.unsplash_access_key.as_ref() else {
        return err(
            StatusCode::SERVICE_UNAVAILABLE,
            "service_not_configured",
            "Unsplash access key not set",
        );
    };

    let upstream = state
        .http_client
        .get("https://api.unsplash.com/search/photos")
        .header("Authorization", format!("Client-ID {}", access_key))
        .query(&[
            ("query", request.query.as_str()),
            ("page", &request.page.to_string()),
            ("per_page", &request.per_page.to_string()),
        ])
        .send()
        .await;

    match upstream {
        Ok(resp) => {
            let status = resp.status();
            let body: Value = resp.json().await.unwrap_or_else(|_| json!({}));
            (
                StatusCode::from_u16(status.as_u16())
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                Json(body),
            )
                .into_response()
        }
        Err(e) => err(StatusCode::BAD_GATEWAY, "upstream_error", &e.to_string()),
    }
}

fn err(status: StatusCode, code: &str, message: &str) -> axum::response::Response {
    (
        status,
        Json(json!({ "error": { "code": code, "message": message } })),
    )
        .into_response()
}

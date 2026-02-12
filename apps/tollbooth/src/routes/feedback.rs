use axum::{
    extract::{State, Json},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, error};

use crate::AppState;

/// Payload received from core
#[derive(Debug, Deserialize)]
pub struct FeedbackPayload {
    #[serde(rename = "type")]
    pub feedback_type: String,
    pub content: String,
}

/// Payload forwarded to Atlas
#[derive(Debug, Serialize)]
struct AtlasFeedbackPayload {
    subdomain: String,
    #[serde(rename = "type")]
    feedback_type: String,
    content: String,
    metadata: serde_json::Value,
}

pub async fn handle_feedback(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<FeedbackPayload>,
) -> impl IntoResponse {
    info!("Received feedback proxy request: type={}", payload.feedback_type);

    // If Atlas is configured, forward immediately
    if let Some(atlas_url) = &state.config.atlas_url {
        let subdomain = match &state.config.subdomain {
            Some(s) => s.clone(),
            None => {
                error!("Cannot forward feedback: subdomain not configured");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };

        let target_url = format!("{}/internal/feedback", atlas_url);
        let secret = state.config.atlas_secret.clone().unwrap_or_default();

        let atlas_payload = AtlasFeedbackPayload {
            subdomain,
            feedback_type: payload.feedback_type,
            content: payload.content,
            metadata: serde_json::json!({}),
        };

        match state.http_client
            .post(&target_url)
            .header("X-Atlas-Secret", secret)
            .json(&atlas_payload)
            .send()
            .await
        {
            Ok(res) => {
                if !res.status().is_success() {
                    error!("Atlas rejected feedback: {}", res.status());
                    return StatusCode::BAD_GATEWAY.into_response();
                }
                info!("Successfully forwarded feedback to Atlas");
            },
            Err(e) => {
                error!("Failed to forward feedback to Atlas: {}", e);
                return StatusCode::SERVICE_UNAVAILABLE.into_response();
            }
        }
    } else {
        info!("Standalone mode (No Atlas): Feedback logged: {:?}", payload);
    }

    StatusCode::OK.into_response()
}

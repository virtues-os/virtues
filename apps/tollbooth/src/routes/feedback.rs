use axum::{
    extract::{State, Json},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, error};

use crate::AppState;

#[derive(Debug, Deserialize, Serialize)]
pub struct FeedbackPayload {
    #[serde(rename = "type")]
    pub feedback_type: String,
    pub message: String,
    // Add any metadata we might want to attach (user_id optional since it comes from header)
    #[serde(default)]
    pub user_id: Option<String>,
}

pub async fn handle_feedback(
    State(state): State<Arc<AppState>>,
    Json(mut payload): Json<FeedbackPayload>,
) -> impl IntoResponse {
    info!("Received feedback proxy request: type={}", payload.feedback_type);

    // If Atlas is configured, forward immediately
    if let Some(atlas_url) = &state.config.atlas_url {
        let target_url = format!("{}/internal/feedback", atlas_url);
        let secret = state.config.atlas_secret.clone().unwrap_or_default();
        
        // Ensure user_id is populated if we have it in context (TODO: middleware extraction)
        // For now, we trust the payload or headers passed through proxy

        match state.http_client
            .post(&target_url)
            .header("X-Atlas-Secret", secret)
            .json(&payload)
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
        // Fallback for standalone mode: just log it
        info!("Standalone mode (No Atlas): Feedback logged: {:?}", payload);
    }

    StatusCode::OK.into_response()
}

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::env;
use tracing::{error, info};
use crate::server::ingest::AppState;

#[derive(Debug, Serialize)]
pub struct FeedbackResponse {
    success: bool,
    data: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct FeedbackRequest {
    #[serde(rename = "type")]
    pub feedback_type: String, // "feedback", "bug", "feature"
    pub message: String,
}

pub async fn submit_feedback(
    State(_state): State<AppState>,
    Json(payload): Json<FeedbackRequest>,
) -> impl IntoResponse {
    info!(
        "Feedback received: Type={}, Message={}", 
        payload.feedback_type, 
        payload.message
    );

    // In production, we forward this to the Tollbooth sidecar
    if let Ok(tollbooth_url) = env::var("TOLLBOOTH_URL") {
        let client = reqwest::Client::new();
        let target_url = format!("{}/services/feedback", tollbooth_url);
        
        // We fire and forget - don't block the user if tollbooth is down
        // Realistically we should probably queue this, but for now we just try
        tokio::spawn(async move {
            let secret = env::var("TOLLBOOTH_SECRET").unwrap_or_default();
            
            match client
                .post(&target_url)
                .header("X-Virtues-Secret", secret)
                // We'd add user context here if available in env or passed in
                .json(&payload)
                .send()
                .await 
            {
                Ok(res) => {
                    if !res.status().is_success() {
                        error!("Failed to forward feedback to tollbooth: {}", res.status());
                    }
                },
                Err(e) => error!("Failed to reach tollbooth for feedback: {}", e),
            }
        });
    } else {
        info!("TOLLBOOTH_URL not set - feedback logged locally only");
    }

    (StatusCode::OK, Json(FeedbackResponse {
        success: true,
        data: Some("Feedback received".to_string()),
        error: None,
    }))
}

//! Health check endpoints

use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;
use std::sync::Arc;

use crate::AppState;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub service: &'static str,
    pub version: &'static str,
}

#[derive(Serialize)]
pub struct ReadinessResponse {
    pub status: &'static str,
    pub budgets_loaded: usize,
    pub ai_gateway_configured: bool,
}

/// Liveness probe - is the service running?
pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "tollbooth",
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// Readiness probe - is the service ready to handle requests?
pub async fn readiness_check(State(state): State<Arc<AppState>>) -> (StatusCode, Json<ReadinessResponse>) {
    let budgets_loaded = state.budget.budgets_count();
    let ai_gateway_configured = state.config.has_llm_provider();

    let response = ReadinessResponse {
        status: if ai_gateway_configured { "ready" } else { "degraded" },
        budgets_loaded,
        ai_gateway_configured,
    };

    let status = if ai_gateway_configured {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status, Json(response))
}

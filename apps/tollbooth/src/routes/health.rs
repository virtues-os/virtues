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
    pub providers: ProvidersStatus,
}

#[derive(Serialize)]
pub struct ProvidersStatus {
    pub openai: bool,
    pub anthropic: bool,
    pub cerebras: bool,
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

    let providers = ProvidersStatus {
        openai: state.config.openai_api_key.is_some(),
        anthropic: state.config.anthropic_api_key.is_some(),
        cerebras: state.config.cerebras_api_key.is_some(),
    };

    let has_any_provider = providers.openai || providers.anthropic || providers.cerebras;

    let response = ReadinessResponse {
        status: if has_any_provider { "ready" } else { "degraded" },
        budgets_loaded,
        providers,
    };

    let status = if has_any_provider {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status, Json(response))
}

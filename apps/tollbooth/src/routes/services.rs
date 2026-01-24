//! External Service Proxies
//!
//! All billable external services are proxied through Tollbooth for unified budget enforcement.
//! Services: Exa (web search), Google Places (autocomplete)
//!
//! Each proxy:
//! 1. Validates auth headers (extracts user_id)
//! 2. Checks budget (rejects if insufficient)
//! 3. Forwards request with appropriate auth
//! 4. Calculates cost from response
//! 5. Deducts from user's budget

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::auth::AuthenticatedRequest;
use crate::AppState;

// =============================================================================
// Pricing Constants
// =============================================================================

/// Exa pricing: ~$0.003 per search request
const EXA_COST_PER_REQUEST: f64 = 0.003;

/// Google Places pricing: ~$0.003 per autocomplete request
const GOOGLE_PLACES_COST_PER_REQUEST: f64 = 0.003;

// =============================================================================
// Error Types
// =============================================================================

#[derive(Debug, Serialize)]
struct ServiceError {
    error: ServiceErrorDetails,
}

#[derive(Debug, Serialize)]
struct ServiceErrorDetails {
    message: String,
    code: String,
}

impl ServiceError {
    fn insufficient_budget() -> Self {
        Self {
            error: ServiceErrorDetails {
                message: "Insufficient budget".to_string(),
                code: "insufficient_budget".to_string(),
            },
        }
    }

    fn service_not_configured(service: &str) -> Self {
        Self {
            error: ServiceErrorDetails {
                message: format!("{} API key not configured", service),
                code: "service_not_configured".to_string(),
            },
        }
    }

    fn upstream_error(message: String) -> Self {
        Self {
            error: ServiceErrorDetails {
                message,
                code: "upstream_error".to_string(),
            },
        }
    }
}

// =============================================================================
// Exa Proxy
// =============================================================================

#[derive(Debug, Deserialize, Serialize)]
struct ExaSearchRequest {
    query: String,
    #[serde(flatten)]
    other: serde_json::Value,
}

/// POST /v1/services/exa/search
/// Web search via Exa
async fn exa_search(
    State(state): State<Arc<AppState>>,
    auth: AuthenticatedRequest,
    Json(request): Json<ExaSearchRequest>,
) -> Response {
    // Check budget
    if !state.budget.has_budget(&auth.user_id) {
        return (StatusCode::PAYMENT_REQUIRED, Json(ServiceError::insufficient_budget()))
            .into_response();
    }

    // Get API key
    let api_key = match &state.config.exa_api_key {
        Some(key) => key,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ServiceError::service_not_configured("Exa")),
            )
                .into_response();
        }
    };

    // Forward request
    let response = match state
        .http_client
        .post("https://api.exa.ai/search")
        .header("x-api-key", api_key)
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(ServiceError::upstream_error(e.to_string())),
            )
                .into_response();
        }
    };

    let upstream_status = response.status();
    let status_code = StatusCode::from_u16(upstream_status.as_u16())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body: serde_json::Value = match response.json().await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(ServiceError::upstream_error(e.to_string())),
            )
                .into_response();
        }
    };

    // Deduct cost on successful search
    if upstream_status.is_success() {
        state.budget.deduct(&auth.user_id, EXA_COST_PER_REQUEST);
        tracing::info!(
            user_id = %auth.user_id,
            query = %request.query,
            cost_usd = EXA_COST_PER_REQUEST,
            "Exa search complete, budget deducted"
        );
    }

    (status_code, Json(body)).into_response()
}

/// POST /v1/services/exa/contents
/// Get contents of URLs via Exa
async fn exa_contents(
    State(state): State<Arc<AppState>>,
    auth: AuthenticatedRequest,
    Json(request): Json<serde_json::Value>,
) -> Response {
    // Check budget
    if !state.budget.has_budget(&auth.user_id) {
        return (StatusCode::PAYMENT_REQUIRED, Json(ServiceError::insufficient_budget()))
            .into_response();
    }

    // Get API key
    let api_key = match &state.config.exa_api_key {
        Some(key) => key,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ServiceError::service_not_configured("Exa")),
            )
                .into_response();
        }
    };

    // Forward request
    let response = match state
        .http_client
        .post("https://api.exa.ai/contents")
        .header("x-api-key", api_key)
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(ServiceError::upstream_error(e.to_string())),
            )
                .into_response();
        }
    };

    let upstream_status = response.status();
    let status_code = StatusCode::from_u16(upstream_status.as_u16())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body: serde_json::Value = match response.json().await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(ServiceError::upstream_error(e.to_string())),
            )
                .into_response();
        }
    };

    // Deduct cost on success
    if upstream_status.is_success() {
        state.budget.deduct(&auth.user_id, EXA_COST_PER_REQUEST);
        tracing::info!(
            user_id = %auth.user_id,
            cost_usd = EXA_COST_PER_REQUEST,
            "Exa contents fetch complete, budget deducted"
        );
    }

    (status_code, Json(body)).into_response()
}

// =============================================================================
// Google Places Proxy
// =============================================================================

#[derive(Debug, Deserialize, Serialize)]
struct PlacesAutocompleteRequest {
    input: String,
    #[serde(flatten)]
    other: serde_json::Value,
}

/// POST /v1/services/google/places/autocomplete
/// Google Places autocomplete
async fn google_places_autocomplete(
    State(state): State<Arc<AppState>>,
    auth: AuthenticatedRequest,
    Json(request): Json<PlacesAutocompleteRequest>,
) -> Response {
    // Check budget
    if !state.budget.has_budget(&auth.user_id) {
        return (StatusCode::PAYMENT_REQUIRED, Json(ServiceError::insufficient_budget()))
            .into_response();
    }

    // Get API key
    let api_key = match &state.config.google_api_key {
        Some(key) => key,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ServiceError::service_not_configured("Google Places")),
            )
                .into_response();
        }
    };

    // Forward request to Google Places API (New)
    let response = match state
        .http_client
        .post("https://places.googleapis.com/v1/places:autocomplete")
        .header("X-Goog-Api-Key", api_key)
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(ServiceError::upstream_error(e.to_string())),
            )
                .into_response();
        }
    };

    let upstream_status = response.status();
    let status_code = StatusCode::from_u16(upstream_status.as_u16())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let response_body: serde_json::Value = match response.json().await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(ServiceError::upstream_error(e.to_string())),
            )
                .into_response();
        }
    };

    // Deduct cost on success
    if upstream_status.is_success() {
        state.budget.deduct(&auth.user_id, GOOGLE_PLACES_COST_PER_REQUEST);
        tracing::info!(
            user_id = %auth.user_id,
            input = %request.input,
            cost_usd = GOOGLE_PLACES_COST_PER_REQUEST,
            "Google Places autocomplete complete, budget deducted"
        );
    }

    (status_code, Json(response_body)).into_response()
}

/// GET /v1/services/google/places/:place_id
/// Google Places details
async fn google_places_details(
    State(state): State<Arc<AppState>>,
    auth: AuthenticatedRequest,
    axum::extract::Path(place_id): axum::extract::Path<String>,
) -> Response {
    // Check budget
    if !state.budget.has_budget(&auth.user_id) {
        return (StatusCode::PAYMENT_REQUIRED, Json(ServiceError::insufficient_budget()))
            .into_response();
    }

    // Get API key
    let api_key = match &state.config.google_api_key {
        Some(key) => key,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ServiceError::service_not_configured("Google Places")),
            )
                .into_response();
        }
    };

    // Forward request to Google Places API (New)
    let response = match state
        .http_client
        .get(format!("https://places.googleapis.com/v1/places/{}", place_id))
        .header("X-Goog-Api-Key", api_key)
        .header("X-Goog-FieldMask", "id,displayName,formattedAddress,location")
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(ServiceError::upstream_error(e.to_string())),
            )
                .into_response();
        }
    };

    let upstream_status = response.status();
    let status_code = StatusCode::from_u16(upstream_status.as_u16())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let response_body: serde_json::Value = match response.json().await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(ServiceError::upstream_error(e.to_string())),
            )
                .into_response();
        }
    };

    // Deduct cost on success
    if upstream_status.is_success() {
        state.budget.deduct(&auth.user_id, GOOGLE_PLACES_COST_PER_REQUEST);
        tracing::info!(
            user_id = %auth.user_id,
            place_id = %place_id,
            cost_usd = GOOGLE_PLACES_COST_PER_REQUEST,
            "Google Places details complete, budget deducted"
        );
    }

    (status_code, Json(response_body)).into_response()
}

// =============================================================================
// Budget Check Endpoint (for pre-flight checks)
// =============================================================================

#[derive(Debug, Serialize)]
struct BudgetCheckResponse {
    has_budget: bool,
    balance_usd: f64,
}

/// GET /v1/budget/check
/// Check if user has sufficient budget (for pre-flight checks)
async fn check_budget(
    State(state): State<Arc<AppState>>,
    auth: AuthenticatedRequest,
) -> Json<BudgetCheckResponse> {
    let has_budget = state.budget.has_budget(&auth.user_id);
    let balance = state.budget.get_balance(&auth.user_id);

    Json(BudgetCheckResponse {
        has_budget,
        balance_usd: balance,
    })
}

// =============================================================================
// Router
// =============================================================================

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        // Budget check
        .route("/budget/check", axum::routing::get(check_budget))
        // Exa
        .route("/services/exa/search", post(exa_search))
        .route("/services/exa/contents", post(exa_contents))
        // Google Places
        .route(
            "/services/google/places/autocomplete",
            post(google_places_autocomplete),
        )
        .route(
            "/services/google/places/:place_id",
            axum::routing::get(google_places_details),
        )
        // Feedback
        .route("/services/feedback", post(crate::routes::feedback::handle_feedback))
}

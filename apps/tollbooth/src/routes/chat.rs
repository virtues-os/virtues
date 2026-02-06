//! Chat Completions API Routes
//!
//! All LLM chat requests come through here (OpenAI-compatible format).
//! Routes all requests to Vercel AI Gateway which handles provider routing.
//!
//! Flow:
//! 1. Validate auth headers (done by auth extractor)
//! 2. Check budget in RAM
//! 3. Forward to Vercel AI Gateway
//! 4. Extract usage from response
//! 5. Deduct cost from budget
//!
//! PRIVACY GUARANTEE:
//! We do NOT log request bodies (prompts) or response bodies (completions).
//! This code is open source so you can verify this guarantee.

use axum::{
    body::Bytes,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{
    auth::AuthenticatedRequest,
    providers::{calculate_cost, get_embeddings_config, get_provider_config},
    proxy::ProxyError,
    AppState,
};

/// OpenAI-format request (what clients send)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    /// Messages array - use Value to support complex message types (tool calls, etc.)
    pub messages: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// Tool definitions for function calling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<serde_json::Value>>,
    /// Tool choice: "auto", "none", "required", or specific tool
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
}

/// Usage data for billing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Build OpenAI-compatible router
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/chat/completions", post(chat_completions))
        .route("/completions", post(completions))
        .route("/embeddings", post(embeddings))
        .route("/models", axum::routing::get(list_models))
        .route("/models/recommended", axum::routing::get(list_recommended_models))
}

/// POST /v1/chat/completions
///
/// Main chat endpoint - routes to Vercel AI Gateway
async fn chat_completions(
    State(state): State<Arc<AppState>>,
    auth: AuthenticatedRequest,
    Json(request): Json<CompletionRequest>,
) -> Result<Response, ProxyError> {
    complete_with_billing(&state, &auth, request).await
}

/// POST /v1/completions
///
/// Legacy completions endpoint (forwards to chat completions)
async fn completions(
    State(state): State<Arc<AppState>>,
    auth: AuthenticatedRequest,
    Json(request): Json<CompletionRequest>,
) -> Result<Response, ProxyError> {
    complete_with_billing(&state, &auth, request).await
}

/// POST /v1/embeddings
///
/// Embeddings endpoint - forwards to AI Gateway
async fn embeddings(
    State(state): State<Arc<AppState>>,
    auth: AuthenticatedRequest,
    body: Bytes,
) -> Result<Response, ProxyError> {
    // Check subscription first
    if !state.subscription.is_active(&auth.user_id) {
        let status = state.subscription.get_status(&auth.user_id);
        return Err(ProxyError::SubscriptionExpired {
            status: status.status,
        });
    }

    // Check budget
    if !state.budget.has_budget(&auth.user_id) {
        let balance = state.budget.get_balance(&auth.user_id);
        return Err(ProxyError::InsufficientBudget { balance });
    }

    let config = get_embeddings_config(&state.config);

    // Forward to AI Gateway embeddings endpoint
    let response = state
        .http_client
        .post(&config.endpoint)
        .header("Authorization", format!("Bearer {}", config.api_key))
        .header("Content-Type", "application/json")
        .body(body.to_vec())
        .send()
        .await
        .map_err(|e| ProxyError::NetworkError {
            message: e.to_string(),
        })?;

    let status = response.status();
    let body_bytes = response.bytes().await.map_err(|e| ProxyError::NetworkError {
        message: e.to_string(),
    })?;

    // Extract usage for billing
    if status.is_success() {
        if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
            if let Some(usage) = json.get("usage") {
                let total_tokens = usage
                    .get("total_tokens")
                    .and_then(|t| t.as_u64())
                    .unwrap_or(0) as u32;
                // Embedding cost: ~$0.0001 per 1K tokens
                let cost = (total_tokens as f64 / 1000.0) * 0.0001;
                state.budget.deduct(&auth.user_id, cost);
            }
        }
    }

    Ok((
        StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK),
        body_bytes,
    )
        .into_response())
}

/// GET /v1/models
///
/// Proxy to Vercel AI Gateway to get full model catalog.
/// Falls back to local registry if gateway is unavailable.
async fn list_models(State(state): State<Arc<AppState>>) -> Response {
    // Try to fetch from gateway
    let gateway_url = format!("{}/v1/models", state.config.ai_gateway_url);

    match state
        .http_client
        .get(&gateway_url)
        .header("Authorization", format!("Bearer {}", state.config.ai_gateway_api_key))
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {
            match response.json::<serde_json::Value>().await {
                Ok(gateway_response) => {
                    tracing::debug!("Fetched models from Vercel AI Gateway");
                    return Json(gateway_response).into_response();
                }
                Err(e) => {
                    tracing::warn!("Failed to parse gateway response: {}", e);
                }
            }
        }
        Ok(response) => {
            tracing::warn!("Gateway returned status {}", response.status());
        }
        Err(e) => {
            tracing::warn!("Failed to fetch from gateway: {}", e);
        }
    }

    // Fallback to local registry
    tracing::info!("Falling back to local model registry");
    let registry = crate::models::get();
    let enabled_models = registry.get_enabled_models(&state.config);

    let models: Vec<serde_json::Value> = enabled_models
        .iter()
        .map(|m| {
            serde_json::json!({
                "id": m.model_id,
                "object": "model",
                "created": 1700000000,
                "owned_by": m.provider.to_lowercase(),
                "display_name": m.display_name,
                "context_window": m.context_window,
                "max_output_tokens": m.max_output_tokens,
                "supports_tools": m.supports_tools
            })
        })
        .collect();

    Json(serde_json::json!({
        "object": "list",
        "data": models
    })).into_response()
}

/// GET /v1/models/recommended
///
/// Returns the 4 curated slot models (chat, lite, reasoning, coding).
/// These are the default models for each purpose slot.
async fn list_recommended_models(State(_state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let registry = crate::models::get();

    let models: Vec<serde_json::Value> = registry
        .models
        .iter()
        .map(|m| {
            // Determine which slot this model belongs to
            let slot = if m.model_id == virtues_registry::models::default_model_for_slot(
                virtues_registry::models::ModelSlot::Chat
            ) {
                "chat"
            } else if m.model_id == virtues_registry::models::default_model_for_slot(
                virtues_registry::models::ModelSlot::Lite
            ) {
                "lite"
            } else if m.model_id == virtues_registry::models::default_model_for_slot(
                virtues_registry::models::ModelSlot::Reasoning
            ) {
                "reasoning"
            } else if m.model_id == virtues_registry::models::default_model_for_slot(
                virtues_registry::models::ModelSlot::Coding
            ) {
                "coding"
            } else {
                "other"
            };

            serde_json::json!({
                "id": m.model_id,
                "object": "model",
                "created": 1700000000,
                "owned_by": m.provider.to_lowercase(),
                "display_name": m.display_name,
                "context_window": m.context_window,
                "max_output_tokens": m.max_output_tokens,
                "supports_tools": m.supports_tools,
                "slot": slot,
                "input_cost_per_1k": m.input_cost_per_1k,
                "output_cost_per_1k": m.output_cost_per_1k
            })
        })
        .collect();

    Json(serde_json::json!({
        "object": "list",
        "data": models,
        "slots": {
            "chat": virtues_registry::models::default_model_for_slot(virtues_registry::models::ModelSlot::Chat),
            "lite": virtues_registry::models::default_model_for_slot(virtues_registry::models::ModelSlot::Lite),
            "reasoning": virtues_registry::models::default_model_for_slot(virtues_registry::models::ModelSlot::Reasoning),
            "coding": virtues_registry::models::default_model_for_slot(virtues_registry::models::ModelSlot::Coding)
        }
    }))
}

/// Common completion logic with budget checking and billing
async fn complete_with_billing(
    state: &AppState,
    auth: &AuthenticatedRequest,
    request: CompletionRequest,
) -> Result<Response, ProxyError> {
    // 0. Check subscription (0ms - in RAM)
    if !state.subscription.is_active(&auth.user_id) {
        let status = state.subscription.get_status(&auth.user_id);
        tracing::debug!(
            user_id = %auth.user_id,
            status = %status.status,
            "Request blocked: subscription expired"
        );
        return Err(ProxyError::SubscriptionExpired {
            status: status.status,
        });
    }

    // 1. Check budget (0ms - in RAM)
    if !state.budget.has_budget(&auth.user_id) {
        let balance = state.budget.get_balance(&auth.user_id);
        tracing::debug!(
            user_id = %auth.user_id,
            balance = %balance,
            "Request blocked: insufficient budget"
        );
        return Err(ProxyError::InsufficientBudget { balance });
    }

    // 2. Handle streaming requests
    if request.stream == Some(true) {
        let streaming_req = crate::routes::streaming::StreamingRequest {
            model: request.model.clone(),
            messages: request.messages.clone(),
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            tools: request.tools.clone(),
            tool_choice: request.tool_choice.clone(),
        };

        return crate::routes::streaming::create_streaming_response(
            &state.http_client,
            &state.config,
            &state.budget,
            &auth.user_id,
            streaming_req,
        )
        .await;
    }

    // 3. Get provider configuration (AI Gateway)
    let provider = get_provider_config(&request.model, &state.config);
    let model = request.model.clone();

    // 4. Build OpenAI-compatible request body
    let mut body = serde_json::json!({
        "model": provider.model_name,
        "messages": request.messages,
        "max_tokens": request.max_tokens.unwrap_or(4096),
        "temperature": request.temperature.unwrap_or(0.7)
    });

    // Only include tools if present and non-empty (providers reject null/empty arrays)
    if let Some(ref tools) = request.tools {
        if !tools.is_empty() {
            body["tools"] = serde_json::json!(tools);
            if let Some(ref choice) = request.tool_choice {
                body["tool_choice"] = choice.clone();
            }
        }
    }

    // 5. Send request to AI Gateway
    let response = state
        .http_client
        .post(&provider.endpoint)
        .header("Authorization", format!("Bearer {}", provider.api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| ProxyError::NetworkError {
            message: e.to_string(),
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();

        tracing::warn!(
            status = status.as_u16(),
            model = %request.model,
            endpoint = %provider.endpoint,
            error_preview = %error_text.chars().take(500).collect::<String>(),
            "AI Gateway returned error"
        );

        return Err(ProxyError::UpstreamError {
            status: status.as_u16(),
            message: error_text,
        });
    }

    let response_bytes = response.bytes().await.map_err(|e| ProxyError::NetworkError {
        message: e.to_string(),
    })?;

    // 6. Parse response and extract usage
    let resp: serde_json::Value =
        serde_json::from_slice(&response_bytes).map_err(|e| ProxyError::UpstreamError {
            status: 500,
            message: format!("Failed to parse AI Gateway response: {}", e),
        })?;

    let usage = resp
        .get("usage")
        .map(|u| Usage {
            prompt_tokens: u
                .get("prompt_tokens")
                .and_then(|t| t.as_u64())
                .unwrap_or(0) as u32,
            completion_tokens: u
                .get("completion_tokens")
                .and_then(|t| t.as_u64())
                .unwrap_or(0) as u32,
            total_tokens: u
                .get("total_tokens")
                .and_then(|t| t.as_u64())
                .unwrap_or(0) as u32,
        })
        .unwrap_or(Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        });

    // 7. Calculate cost and deduct from budget
    let cost = calculate_cost(&model, usage.prompt_tokens, usage.completion_tokens);
    state.budget.deduct(&auth.user_id, cost);

    tracing::debug!(
        user_id = %auth.user_id,
        model = %model,
        prompt_tokens = %usage.prompt_tokens,
        completion_tokens = %usage.completion_tokens,
        cost_usd = %cost,
        "Request completed, budget deducted"
    );

    // 8. Return response (already in OpenAI format from gateway)
    Ok(Json(resp).into_response())
}

//! Chat Completions API Routes
//!
//! All LLM chat requests come through here (OpenAI-compatible format).
//! Routes to multiple providers: OpenAI, Anthropic, Google Vertex AI, Cerebras, xAI.
//!
//! Flow:
//! 1. Validate auth headers (done by auth extractor)
//! 2. Check budget in RAM
//! 3. Route to provider via direct HTTP
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
    providers::{calculate_cost, get_provider_config, get_vertex_ai_token},
    proxy::ProxyError,
    AppState,
};

/// OpenAI-format request (what clients send)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
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
}

/// POST /v1/chat/completions
///
/// Main chat endpoint - routes to appropriate provider via direct HTTP
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
/// Embeddings endpoint - requires OpenAI API key
async fn embeddings(
    State(state): State<Arc<AppState>>,
    auth: AuthenticatedRequest,
    body: Bytes,
) -> Result<Response, ProxyError> {
    // Check budget first
    if !state.budget.has_budget(&auth.user_id) {
        let balance = state.budget.get_balance(&auth.user_id);
        return Err(ProxyError::InsufficientBudget { balance });
    }

    // Embeddings require OpenAI - use shared client
    let api_key = state.config.openai_api_key.as_ref().ok_or_else(|| {
        ProxyError::UpstreamError {
            status: 503,
            message: "OpenAI not configured (required for embeddings)".to_string(),
        }
    })?;

    // Forward to OpenAI embeddings endpoint using shared client
    let response = state
        .http_client
        .post("https://api.openai.com/v1/embeddings")
        .header("Authorization", format!("Bearer {}", api_key))
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
/// List available models from the model registry
async fn list_models(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
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
    }))
}

/// Common completion logic with budget checking and billing
async fn complete_with_billing(
    state: &AppState,
    auth: &AuthenticatedRequest,
    request: CompletionRequest,
) -> Result<Response, ProxyError> {
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

    // 2. Handle streaming requests (all providers)
    if request.stream == Some(true) {
        // Use streaming module for SSE passthrough
        let streaming_req = crate::routes::streaming::StreamingRequest {
            model: request.model.clone(),
            messages: request
                .messages
                .iter()
                .map(|m| serde_json::json!({ "role": m.role, "content": m.content }))
                .collect(),
            max_tokens: request.max_tokens,
            temperature: request.temperature,
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

    // 3. Get provider configuration
    let provider = get_provider_config(&request.model, &state.config).ok_or_else(|| {
        ProxyError::UpstreamError {
            status: 503,
            message: format!("No provider configured for model: {}", request.model),
        }
    })?;

    let model = request.model.clone();

    // 4. Build request body (handle Anthropic format difference)
    let body = if provider.is_anthropic {
        // Anthropic Messages API format
        // - system message is separate from messages array
        // - max_tokens is REQUIRED
        let mut system_prompt: Option<String> = None;
        let messages: Vec<serde_json::Value> = request
            .messages
            .iter()
            .filter_map(|m| {
                if m.role == "system" {
                    system_prompt = Some(m.content.clone());
                    None
                } else {
                    Some(serde_json::json!({
                        "role": m.role,
                        "content": m.content
                    }))
                }
            })
            .collect();

        let mut body = serde_json::json!({
            "model": provider.model_name,
            "messages": messages,
            "max_tokens": request.max_tokens.unwrap_or(4096)
        });

        if let Some(system) = system_prompt {
            body["system"] = serde_json::Value::String(system);
        }

        if let Some(temp) = request.temperature {
            body["temperature"] = serde_json::json!(temp);
        }

        body
    } else {
        // OpenAI-compatible format (OpenAI, Cerebras, xAI, Vertex AI)
        serde_json::json!({
            "model": provider.model_name,
            "messages": request.messages.iter().map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content
                })
            }).collect::<Vec<_>>(),
            "max_tokens": request.max_tokens.unwrap_or(4096),
            "temperature": request.temperature.unwrap_or(0.7)
        })
    };

    // 5. Build HTTP request with proper auth headers
    let mut req_builder = state
        .http_client
        .post(&provider.endpoint)
        .header("Content-Type", "application/json");

    if provider.is_anthropic {
        req_builder = req_builder
            .header("x-api-key", provider.api_key.as_ref().unwrap())
            .header("anthropic-version", "2023-06-01");
    } else if provider.is_vertex_ai {
        // Vertex AI uses OAuth2 access tokens
        let access_token = get_vertex_ai_token().await?;
        req_builder = req_builder.header("Authorization", format!("Bearer {}", access_token));
    } else {
        // OpenAI, Cerebras, xAI - all use API keys
        req_builder = req_builder.header(
            "Authorization",
            format!("Bearer {}", provider.api_key.as_ref().unwrap()),
        );
    }

    // 6. Send request
    let response = req_builder
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
            "LLM provider returned error"
        );

        return Err(ProxyError::UpstreamError {
            status: status.as_u16(),
            message: error_text,
        });
    }

    let response_bytes = response.bytes().await.map_err(|e| ProxyError::NetworkError {
        message: e.to_string(),
    })?;

    // 7. Parse response and extract usage
    let (openai_response, usage) = if provider.is_anthropic {
        // Transform Anthropic response to OpenAI format
        let anthropic_resp: serde_json::Value =
            serde_json::from_slice(&response_bytes).map_err(|e| ProxyError::UpstreamError {
                status: 500,
                message: format!("Failed to parse Anthropic response: {}", e),
            })?;

        // Extract content from Anthropic format
        let content = anthropic_resp
            .get("content")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|item| item.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();

        // Extract usage
        let input_tokens = anthropic_resp
            .get("usage")
            .and_then(|u| u.get("input_tokens"))
            .and_then(|t| t.as_u64())
            .unwrap_or(0) as u32;
        let output_tokens = anthropic_resp
            .get("usage")
            .and_then(|u| u.get("output_tokens"))
            .and_then(|t| t.as_u64())
            .unwrap_or(0) as u32;

        let usage = Usage {
            prompt_tokens: input_tokens,
            completion_tokens: output_tokens,
            total_tokens: input_tokens + output_tokens,
        };

        // Build OpenAI-format response
        let openai_resp = serde_json::json!({
            "id": anthropic_resp.get("id").and_then(|v| v.as_str()).unwrap_or("chatcmpl-anthropic"),
            "object": "chat.completion",
            "created": chrono::Utc::now().timestamp(),
            "model": model,
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": content
                },
                "finish_reason": match anthropic_resp.get("stop_reason").and_then(|v| v.as_str()) {
                    Some("end_turn") => "stop",
                    Some("max_tokens") => "length",
                    Some(other) => other,
                    None => "stop"
                }
            }],
            "usage": {
                "prompt_tokens": usage.prompt_tokens,
                "completion_tokens": usage.completion_tokens,
                "total_tokens": usage.total_tokens
            }
        });

        (openai_resp, usage)
    } else {
        // OpenAI-compatible response (OpenAI, Cerebras, xAI, Vertex AI)
        let resp: serde_json::Value =
            serde_json::from_slice(&response_bytes).map_err(|e| ProxyError::UpstreamError {
                status: 500,
                message: format!("Failed to parse provider response: {}", e),
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

        (resp, usage)
    };

    // 8. Calculate cost and deduct from budget
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

    // 9. Return OpenAI-format response
    Ok(Json(openai_response).into_response())
}

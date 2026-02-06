//! SSE Streaming Support for Chat Completions
//!
//! Handles streaming passthrough to Vercel AI Gateway with budget enforcement.
//! Usage is extracted from final SSE chunk for billing.
//!
//! PRIVACY GUARANTEE:
//! We do NOT log request bodies (prompts) or response bodies (completions).
//! We only extract usage metadata from the final chunk for billing.

use axum::response::{sse::Event as SseEvent, IntoResponse, Response, Sse};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use tokio_stream::wrappers::ReceiverStream;

use crate::{
    budget::BudgetManager,
    config::Config,
    providers::{calculate_cost, get_provider_config},
    proxy::ProxyError,
};

/// OpenAI streaming chunk format
#[derive(Debug, Deserialize)]
pub struct StreamChunk {
    pub choices: Option<Vec<StreamChoice>>,
    pub usage: Option<StreamUsage>,
}

#[derive(Debug, Deserialize)]
pub struct StreamChoice {
    pub delta: Option<StreamDelta>,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct StreamDelta {
    pub content: Option<String>,
    pub role: Option<String>,
}

/// Usage data from final streaming chunk (when stream_options.include_usage = true)
#[derive(Debug, Clone, Deserialize)]
pub struct StreamUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Internal request format for streaming
#[derive(Clone, Serialize)]
pub struct StreamingRequest {
    pub model: String,
    pub messages: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Tool definitions for function calling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<serde_json::Value>>,
    /// Tool choice: "auto", "none", "required", or specific tool
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
}

/// Create SSE streaming response with budget tracking
pub async fn create_streaming_response(
    client: &reqwest::Client,
    config: &Config,
    budget: &BudgetManager,
    user_id: &str,
    request: StreamingRequest,
) -> Result<Response, ProxyError> {
    let provider = get_provider_config(&request.model, config);

    // Build OpenAI-compatible request body with stream_options for usage tracking
    let mut body = serde_json::json!({
        "model": provider.model_name,
        "messages": request.messages,
        "max_tokens": request.max_tokens.unwrap_or(4096),
        "temperature": request.temperature.unwrap_or(0.7),
        "stream": true,
        "stream_options": { "include_usage": true }
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

    let response = client
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
            "AI Gateway returned error - check API key configuration"
        );

        return Err(ProxyError::UpstreamError {
            status: status.as_u16(),
            message: error_text,
        });
    }

    let model = request.model.clone();
    let user_id = user_id.to_string();
    let budget = budget.clone();
    let bytes_stream = response.bytes_stream();

    // Create channel for SSE events
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<SseEvent, Infallible>>(100);

    // Spawn task to process stream and track usage
    tokio::spawn(async move {
        let mut buffer = String::new();
        let mut final_usage: Option<StreamUsage> = None;

        tokio::pin!(bytes_stream);

        while let Some(chunk_result) = bytes_stream.next().await {
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("Stream error: {}", e);
                    break;
                }
            };

            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Process complete lines
            while let Some(line_end) = buffer.find('\n') {
                let line = buffer[..line_end].trim().to_string();
                buffer = buffer[line_end + 1..].to_string();

                if line.is_empty() {
                    continue;
                }

                if line.starts_with("data: ") {
                    let data = &line[6..];

                    if data == "[DONE]" {
                        // Send [DONE] event
                        let _ = tx.send(Ok(SseEvent::default().data("[DONE]"))).await;

                        // Deduct budget with collected usage
                        let (prompt_tokens, completion_tokens) = {
                            let usage = final_usage.take().unwrap_or(StreamUsage {
                                prompt_tokens: 0,
                                completion_tokens: 0,
                                total_tokens: 0,
                            });
                            (usage.prompt_tokens, usage.completion_tokens)
                        };

                        if prompt_tokens + completion_tokens > 0 {
                            let cost = calculate_cost(&model, prompt_tokens, completion_tokens);
                            budget.deduct(&user_id, cost);
                            tracing::debug!(
                                user_id = %user_id,
                                model = %model,
                                prompt_tokens = prompt_tokens,
                                completion_tokens = completion_tokens,
                                cost_usd = cost,
                                "Streaming complete, budget deducted"
                            );
                        }
                        break;
                    }

                    // Parse chunk and extract usage if present
                    if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
                        if let Some(usage) = chunk.usage {
                            final_usage = Some(usage);
                        }
                    }

                    // Forward the data to client
                    let _ = tx.send(Ok(SseEvent::default().data(data))).await;
                }
            }
        }

        // Ensure channel is properly closed
        drop(tx);
    });

    // Return SSE response
    Ok(Sse::new(ReceiverStream::new(rx))
        .keep_alive(axum::response::sse::KeepAlive::new())
        .into_response())
}

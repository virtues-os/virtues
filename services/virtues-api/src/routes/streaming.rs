//! SSE Streaming Support for Chat Completions
//!
//! Handles streaming passthrough to Vercel AI Gateway with budget enforcement.
//! Usage is extracted from final SSE chunk for billing.
//!
//! PRIVACY GUARANTEE:
//! We do NOT log request bodies (prompts) or response bodies (completions).
//! We only extract usage metadata from the final chunk for billing.
//!
//! The charge side is a callback supplied by the caller: the bearer-auth
//! path in `ai.rs` passes a closure that calls `entitlement::charge()`
//! against Postgres once the final usage is known.

use axum::response::{sse::Event as SseEvent, IntoResponse, Response, Sse};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::future::Future;
use tokio_stream::wrappers::ReceiverStream;

use crate::{
    config::Config,
    providers::{calculate_cost, get_provider_config},
    proxy::ProxyError,
};

/// OpenAI streaming chunk format
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct StreamChunk {
    pub choices: Option<Vec<StreamChoice>>,
    pub usage: Option<StreamUsage>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct StreamChoice {
    pub delta: Option<StreamDelta>,
    pub finish_reason: Option<String>,
}

// F8: intentionally an empty struct (with `serde(default)`) — we parse the
// chunk shape but never read the inner content. Do NOT add `content` or
// `role` fields here: a future `tracing::debug!(?delta)` would then log
// prompt/completion text and quietly void the never-logged guarantee.
// Chunks are forwarded opaquely as raw bytes via the SSE stream.
#[derive(Debug, Default, Deserialize)]
pub struct StreamDelta {}

/// Usage data from final streaming chunk (when stream_options.include_usage = true).
/// `cost` is Vercel AI Gateway's reported USD cost (added 2026-05). When
/// present, it's authoritative; otherwise we fall back to token × registry
/// pricing.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct StreamUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    #[serde(default)]
    pub cost: Option<f64>,
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
    /// Optional reasoning budget hint forwarded to the gateway.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
}

/// Create SSE streaming response with caller-supplied charge callback.
///
/// `on_complete` is called once with the resolved `cost_micros` after the
/// upstream stream emits `[DONE]`. The bearer-auth AI route wires this to
/// `entitlement::charge()`. The streaming hot path knows nothing about
/// budget storage.
pub async fn create_streaming_response<F, Fut>(
    client: &reqwest::Client,
    config: &Config,
    catalog: &crate::catalog::Catalog,
    request: StreamingRequest,
    on_complete: F,
) -> Result<Response, ProxyError>
where
    F: FnOnce(i64) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
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

    if let Some(ref effort) = request.reasoning_effort {
        body["reasoning_effort"] = serde_json::json!(effort);
    }

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

        // F1: never log the upstream error body — provider 4xx bodies often
        // echo back prompt fragments. Body propagates to caller via
        // UpstreamError below, not to tracing.
        tracing::warn!(
            status = status.as_u16(),
            model = %request.model,
            endpoint = %provider.endpoint,
            "AI Gateway returned error"
        );

        return Err(ProxyError::UpstreamError {
            status: status.as_u16(),
            message: error_text,
        });
    }

    let model = request.model.clone();
    // Owned handle: the fallback price is resolved inside the spawned stream
    // task, long after this fn returns. Cheap — it's an Arc.
    let catalog = catalog.clone();
    let bytes_stream = response.bytes_stream();

    // Create channel for SSE events
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<SseEvent, Infallible>>(100);

    // Spawn task to process stream and track usage
    tokio::spawn(async move {
        let mut buffer = String::new();
        let mut final_usage: Option<StreamUsage> = None;
        // `FnOnce` callback wrapped in Option so we can take() inside the
        // loop without moving across iterations.
        let mut on_complete = Some(on_complete);

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

                        // Resolve cost: Vercel-reported (authoritative) or
                        // token × live catalog pricing (fallback).
                        let cost_micros = match final_usage.take() {
                            Some(u) => {
                                if let Some(cost_usd) = u.cost {
                                    (cost_usd * 1_000_000.0).round() as i64
                                } else if u.prompt_tokens + u.completion_tokens > 0 {
                                    let cost_usd = calculate_cost(
                                        &catalog,
                                        &model,
                                        u.prompt_tokens,
                                        u.completion_tokens,
                                    );
                                    (cost_usd * 1_000_000.0).round() as i64
                                } else {
                                    0
                                }
                            }
                            None => 0,
                        };

                        if cost_micros > 0 {
                            if let Some(cb) = on_complete.take() {
                                cb(cost_micros).await;
                                tracing::debug!(
                                    model = %model,
                                    cost_micros,
                                    "streaming complete, charge callback fired"
                                );
                            }
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

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
use serde::Deserialize;
use std::convert::Infallible;
use std::future::Future;
use tokio_stream::wrappers::ReceiverStream;

use crate::{
    config::Config,
    providers::{calculate_cost, get_provider_config, upstream_body},
    proxy::ProxyError,
};

/// OpenAI streaming chunk format
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct StreamChunk {
    pub choices: Option<Vec<StreamChoice>>,
    pub usage: Option<StreamUsage>,
    /// Present when the upstream reports a failure in-stream. Never read
    /// past `is_some()`: the value could quote request text.
    pub error: Option<serde_json::Value>,
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

/// The one frame a chat-completions stream has for "this broke": an
/// OpenAI-shaped `error` object in place of a chunk. The box's parser
/// (`agent/stream.rs`) stops on it and reports the reply as interrupted.
/// The message is transport text, never prompt or completion content.
fn error_frame(message: &str) -> String {
    serde_json::json!({
        "error": { "code": "upstream_stream_broken", "message": message }
    })
    .to_string()
}

/// Resolve a stream's cost: Vercel-reported (authoritative) or token × live
/// catalog pricing (fallback). `None` usage, or an unknown rate, is 0 — served
/// unbilled rather than invented (see `providers::calculate_cost`).
fn resolve_cost_micros(
    catalog: &crate::catalog::Catalog,
    model: &str,
    usage: Option<StreamUsage>,
) -> i64 {
    match usage {
        Some(u) => {
            if let Some(cost_usd) = u.cost {
                (cost_usd * 1_000_000.0).round() as i64
            } else if u.prompt_tokens + u.completion_tokens > 0 {
                calculate_cost(catalog, model, u.prompt_tokens, u.completion_tokens)
                    .map(|c| (c * 1_000_000.0).round() as i64)
                    .unwrap_or(0)
            } else {
                0
            }
        }
        None => 0,
    }
}

/// Create SSE streaming response with caller-supplied charge callback.
///
/// `on_complete` is called once with the resolved `cost_micros` after the
/// upstream stream emits `[DONE]`. The bearer-auth AI route wires this to
/// `entitlement::charge()`. The streaming hot path knows nothing about
/// budget storage.
///
/// `request` is the caller's body, opaque; `model` is the id the route
/// already read from it. See `routes/ai.rs`.
pub async fn create_streaming_response<F, Fut>(
    client: &reqwest::Client,
    config: &Config,
    catalog: &crate::catalog::Catalog,
    model: &str,
    request: serde_json::Value,
    on_complete: F,
) -> Result<Response, ProxyError>
where
    F: FnOnce(i64) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let provider = get_provider_config(model, config);

    // One pass-through for both paths; `stream: true` adds stream_options.
    let body = upstream_body(request, &provider.model_name, true, catalog.enforce_zdr(model));

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
            model = %model,
            endpoint = %provider.endpoint,
            "AI Gateway returned error"
        );

        return Err(ProxyError::UpstreamError {
            status: status.as_u16(),
            message: error_text,
        });
    }

    let model = model.to_string();
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
        // The upstream said it was done. A stream that ends any other way —
        // a read error, the idle timeout, or bytes that simply stop — is
        // reported to the box as an error frame rather than passed off as a
        // clean end, which is what the box used to see and save (VIR-334).
        let mut saw_done = false;
        // The read-error branch sends its own frame; the trailer below must
        // not send a second one for the same break.
        let mut reported = false;
        // `FnOnce` callback wrapped in Option so we can take() inside the
        // loop without moving across iterations.
        let mut on_complete = Some(on_complete);

        tokio::pin!(bytes_stream);

        let mut upstream_open = true;
        'read: while upstream_open {
            match bytes_stream.next().await {
                Some(Ok(chunk)) => buffer.push_str(&String::from_utf8_lossy(&chunk)),
                Some(Err(e)) => {
                    // reqwest's error text names the transport failure, never
                    // the body, so it is safe to log and to forward.
                    tracing::error!(model = %model, "upstream stream broke: {}", e);
                    let _ = tx
                        .send(Ok(SseEvent::default().data(error_frame(&format!(
                            "the connection to the model dropped mid-reply: {e}"
                        )))))
                        .await;
                    reported = true;
                    break 'read;
                }
                None => {
                    // The upstream closed. A final line that arrived without
                    // its newline is still a line — left in the buffer it
                    // would turn a `[DONE]` into a false "ended early".
                    upstream_open = false;
                    if !buffer.trim().is_empty() && !buffer.ends_with('\n') {
                        buffer.push('\n');
                    }
                }
            }

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
                        saw_done = true;
                        // Send [DONE] event
                        let _ = tx.send(Ok(SseEvent::default().data("[DONE]"))).await;

                        let cost_micros = resolve_cost_micros(&catalog, &model, final_usage.take());
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
                        break 'read;
                    }

                    // Parse chunk and extract usage if present
                    if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
                        if let Some(usage) = chunk.usage {
                            final_usage = Some(usage);
                        }
                        // An upstream that reports its own failure as an
                        // `error` chunk has said how it ended; forwarding it
                        // is the whole report, so no second frame below.
                        if chunk.error.is_some() {
                            saw_done = true;
                        }
                    }

                    // Forward the data to client
                    let _ = tx.send(Ok(SseEvent::default().data(data))).await;
                }
            }
        }

        if !saw_done {
            // Bytes stopped without the sentinel. A read error already sent
            // its own frame above; this frame covers the upstream closing
            // quietly. The usage trailer rides with the last chunk, so an
            // unfinished stream almost never carries one — the turn goes
            // unbilled, and that is logged rather than guessed at.
            if !reported {
                let _ = tx
                    .send(Ok(SseEvent::default().data(error_frame(
                        "the model's reply ended before it was finished",
                    ))))
                    .await;
            }
            let cost_micros = resolve_cost_micros(&catalog, &model, final_usage.take());
            if cost_micros > 0 {
                if let Some(cb) = on_complete.take() {
                    cb(cost_micros).await;
                }
            } else {
                tracing::warn!(model = %model, "upstream stream ended without [DONE]; turn unbilled");
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

//! Chat API with SSE streaming
//!
//! Implements the AI SDK v6 UI Message Stream Protocol for chat completions.
//! Protocol uses JSON events with "type" field:
//!   - text-start: marks beginning of text block
//!   - text-delta: incremental text content
//!   - text-end: marks end of text block
//!   - reasoning-start/delta/end: for thinking tokens
//!   - error: error events
//!
//! Requires header: x-vercel-ai-ui-message-stream: v1
//!
//! Streams responses through Tollbooth for budget enforcement and usage tracking.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response, Sse},
    Json,
};
use chrono::Utc;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use tokio_stream::StreamExt;
use uuid::Uuid;

use crate::api::compaction::{build_context_for_llm, compact_session, CompactionOptions};
use crate::api::session_usage::{record_session_usage, UsageData};
use crate::api::sessions::{append_message, ChatMessage};
use crate::api::token_estimation::{estimate_tokens, ContextStatus};
use crate::http_client::tollbooth_streaming_client;
use crate::middleware::auth::AuthUser;

// ============================================================================
// Types
// ============================================================================

/// Chat request from frontend
#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<UIMessage>,
    #[serde(rename = "sessionId")]
    pub session_id: Uuid,
    /// Model ID is required - frontend must send selected model from picker
    pub model: String,
    #[serde(rename = "agentId", default = "default_agent")]
    pub agent_id: String,
}

fn default_agent() -> String {
    "auto".to_string()
}

/// UI Message format (AI SDK v6)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UIMessage {
    pub id: Option<String>,
    pub role: String,
    #[serde(default)]
    pub parts: Vec<UIPart>,
    // Legacy format support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// UI Part types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum UIPart {
    Text {
        text: String,
    },
    Reasoning {
        text: String,
    },
    #[serde(rename = "tool-invocation")]
    ToolInvocation {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        #[serde(default)]
        input: serde_json::Value,
        #[serde(default)]
        state: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        output: Option<serde_json::Value>,
    },
}

/// Streaming event types (AI SDK v6 UI Message Stream Protocol)
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum StreamEvent {
    // Text streaming
    TextStart {
        id: String,
    },
    TextDelta {
        id: String,
        delta: String,
    },
    TextEnd {
        id: String,
    },

    // Reasoning/thinking tokens
    ReasoningStart {
        id: String,
    },
    ReasoningDelta {
        id: String,
        delta: String,
    },
    ReasoningEnd {
        id: String,
    },

    // Error handling
    Error {
        #[serde(rename = "errorText")]
        error_text: String,
    },
}

/// Chat error response
#[derive(Debug, Serialize)]
pub struct ChatError {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

// ============================================================================
// SSE Types
// ============================================================================

type SseEvent = axum::response::sse::Event;

// ============================================================================
// Helper Functions
// ============================================================================

/// Safely serialize a stream event to JSON
fn serialize_event(event: &StreamEvent) -> String {
    serde_json::to_string(event).unwrap_or_else(|e| {
        tracing::error!("Failed to serialize stream event: {}", e);
        r#"{"type":"error","errorText":"Internal serialization error"}"#.to_string()
    })
}

/// Parse Tollbooth error response and return user-friendly StreamEvent
fn parse_tollbooth_error(status: u16, body: &str) -> StreamEvent {
    // Try to parse as JSON error
    let error_text = if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
        let code = json
            .get("error")
            .and_then(|e| e.get("code"))
            .and_then(|c| c.as_str())
            .unwrap_or("");
        let message = json
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or(body);

        match (status, code) {
            (402, "insufficient_budget") | (402, _) => {
                "You've reached your usage limit. Please try again later or upgrade your plan."
                    .to_string()
            }
            (429, _) => "Too many requests. Please wait a moment and try again.".to_string(),
            (401, _) | (403, _) => {
                tracing::error!(status, code, message, "LLM provider authentication failed - check API keys");
                format!("LLM provider auth failed ({}): {}. Check your API key configuration.", status, message)
            }
            (503, _) => {
                "Service temporarily unavailable. Please try again in a few minutes.".to_string()
            }
            _ => format!("Error: {}", message),
        }
    } else {
        // Fallback for non-JSON errors
        match status {
            402 => "You've reached your usage limit.".to_string(),
            429 => "Too many requests. Please wait and try again.".to_string(),
            401 | 403 => {
                tracing::error!(status, body, "LLM provider authentication failed - check API keys");
                format!("LLM provider auth failed ({}). Check your API key configuration.", status)
            }
            _ => "An error occurred. Please try again.".to_string(),
        }
    };

    StreamEvent::Error { error_text }
}

/// Tollbooth configuration validated at handler entry
struct TollboothConfig {
    url: String,
    secret: String,
    user_id: String,
}

impl TollboothConfig {
    /// Load and validate Tollbooth config from environment
    fn from_env(user_id: &str) -> Result<Self, ChatError> {
        let url = std::env::var("TOLLBOOTH_URL").unwrap_or_else(|_| {
            tracing::warn!("TOLLBOOTH_URL not set, using default localhost:9002");
            "http://localhost:9002".to_string()
        });

        let secret = std::env::var("TOLLBOOTH_INTERNAL_SECRET").map_err(|_| ChatError {
            error: "Service misconfigured".to_string(),
            details: Some("TOLLBOOTH_INTERNAL_SECRET not set".to_string()),
        })?;

        crate::tollbooth::validate_secret(&secret).map_err(|e| ChatError {
            error: "Configuration error".to_string(),
            details: Some(e.to_string()),
        })?;

        Ok(Self {
            url,
            secret,
            user_id: user_id.to_string(),
        })
    }
}

// ============================================================================
// Handler
// ============================================================================

/// POST /api/chat - Stream chat completion
///
/// Requires authentication. Routes through Tollbooth for budget enforcement.
pub async fn chat_handler(
    State(pool): State<SqlitePool>,
    user: AuthUser,
    Json(request): Json<ChatRequest>,
) -> Response {
    // Validate model against database
    let valid_models = match crate::api::models::list_models(&pool).await {
        Ok(models) => models,
        Err(e) => {
            tracing::error!("Failed to load models from database: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ChatError {
                    error: "Failed to load models".to_string(),
                    details: Some(e.to_string()),
                }),
            )
                .into_response();
        }
    };

    let allowed_ids: Vec<&str> = valid_models.iter().map(|m| m.model_id.as_str()).collect();
    if !allowed_ids.contains(&request.model.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ChatError {
                error: "Invalid model".to_string(),
                details: Some(format!("Allowed models: {:?}", allowed_ids)),
            }),
        )
            .into_response();
    }

    // Validate Tollbooth config at handler entry (before starting stream)
    let tollbooth_config = match TollboothConfig::from_env(&user.id.to_string()) {
        Ok(config) => Arc::new(config),
        Err(error) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response();
        }
    };

    // Generate message ID
    let msg_id = format!("msg_{}", generate_id());

    // Ensure session exists - use INSERT OR IGNORE to handle race conditions
    let session_id_str = request.session_id.to_string();
    let title = request
        .messages
        .iter()
        .find(|m| m.role == "user")
        .and_then(|m| {
            m.content.clone().or_else(|| {
                m.parts.iter().find_map(|p| match p {
                    UIPart::Text { text } => Some(text.clone()),
                    _ => None,
                })
            })
        })
        .unwrap_or_else(|| "New conversation".to_string());

    let title = if title.len() > 50 {
        format!("{}...", &title[..47])
    } else {
        title
    };

    // Use INSERT OR IGNORE to handle concurrent requests for same session
    if let Err(e) = sqlx::query(
        "INSERT OR IGNORE INTO app_chat_sessions (id, title, messages, message_count) VALUES ($1, $2, '[]', 0)"
    )
    .bind(&session_id_str)
    .bind(&title)
    .execute(&pool)
    .await
    {
        tracing::error!("Failed to create session: {}", e);
    }

    // Save the user message to the session
    // Find the last user message from the request
    if let Some(last_user_msg) = request.messages.iter().rev().find(|m| m.role == "user") {
        let user_content = last_user_msg.content.clone().unwrap_or_else(|| {
            last_user_msg.parts.iter().filter_map(|p| match p {
                UIPart::Text { text } => Some(text.clone()),
                _ => None,
            }).collect::<Vec<_>>().join("\n")
        });

        let user_message = ChatMessage {
            role: "user".to_string(),
            content: user_content,
            timestamp: Utc::now().to_rfc3339(),
            model: None,
            provider: None,
            agent_id: None,
            tool_calls: None,
            reasoning: None,
            intent: None,
            subject: None,
        };

        if let Err(e) = append_message(&pool, request.session_id, user_message).await {
            tracing::error!("Failed to save user message: {}", e);
        }
    }

    // Check if compaction is needed before sending to LLM
    let compaction_status = crate::api::session_usage::check_compaction_needed(
        &pool,
        request.session_id,
        &request.model,
    )
    .await;

    // Auto-compact if critical (>= 85% context usage)
    if matches!(compaction_status, Ok(ContextStatus::Critical)) {
        tracing::info!(
            session_id = %request.session_id,
            "Context critical, auto-compacting session"
        );
        if let Err(e) = compact_session(&pool, request.session_id, CompactionOptions::default()).await
        {
            tracing::warn!(
                session_id = %request.session_id,
                error = %e,
                "Auto-compaction failed, continuing with full context"
            );
        }
    }

    // Load session from DB and build context with compaction summary
    let session_row = match sqlx::query!(
        r#"SELECT messages, conversation_summary, summary_up_to_index
           FROM app_chat_sessions WHERE id = $1"#,
        session_id_str
    )
    .fetch_one(&pool)
    .await
    {
        Ok(row) => row,
        Err(e) => {
            tracing::error!("Failed to load session: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ChatError {
                    error: "Failed to load session".to_string(),
                    details: Some(e.to_string()),
                }),
            )
                .into_response();
        }
    };

    let messages: Vec<ChatMessage> = match serde_json::from_str(&session_row.messages) {
        Ok(msgs) => msgs,
        Err(e) => {
            tracing::error!("Corrupted messages JSON in session {}: {}", session_id_str, e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ChatError {
                    error: "Corrupted session data".to_string(),
                    details: Some(format!("Failed to parse messages: {}", e)),
                }),
            )
                .into_response();
        }
    };

    // Build context using compaction summary if available
    let api_messages = build_context_for_llm(
        &messages,
        session_row.conversation_summary.as_deref(),
        session_row.summary_up_to_index.unwrap_or(0) as usize,
        None, // No system prompt for now
    );

    // Create the streaming response with validated config
    let stream = create_chat_stream(pool, tollbooth_config, request, api_messages, msg_id);

    // AI SDK v6 requires this header for UI Message Stream Protocol
    let sse_response = Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::new());

    (
        [(axum::http::header::HeaderName::from_static("x-vercel-ai-ui-message-stream"), "v1")],
        sse_response
    ).into_response()
}

/// Convert UI messages to OpenAI API format
fn convert_to_api_messages(messages: &[UIMessage]) -> Vec<serde_json::Value> {
    messages
        .iter()
        .filter_map(|msg| {
            // Get text content
            let content = msg.content.clone().unwrap_or_else(|| {
                msg.parts
                    .iter()
                    .filter_map(|p| match p {
                        UIPart::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("")
            });

            if content.is_empty() && msg.role == "system" {
                return None;
            }

            Some(serde_json::json!({
                "role": msg.role,
                "content": content
            }))
        })
        .collect()
}

/// Create the SSE stream for chat
fn create_chat_stream(
    pool: SqlitePool,
    tollbooth_config: Arc<TollboothConfig>,
    request: ChatRequest,
    api_messages: Vec<serde_json::Value>,
    msg_id: String,
) -> Pin<Box<dyn Stream<Item = Result<SseEvent, Infallible>> + Send>> {
    let model = request.model.clone();
    let session_id = request.session_id;

    Box::pin(async_stream::stream! {
        // Build provider options for reasoning if applicable
        let provider_options = build_provider_options(&model);

        // Prepare request body
        let mut body = serde_json::json!({
            "model": model,
            "messages": api_messages,
            "stream": true
        });

        if let Some(opts) = provider_options {
            body["provider_options"] = opts;
        }

        // Make streaming request to Tollbooth using shared client with timeouts
        let client = tollbooth_streaming_client();
        let response = match crate::tollbooth::with_tollbooth_auth(
            client.post(format!("{}/v1/chat/completions", tollbooth_config.url)),
            &tollbooth_config.user_id,
            &tollbooth_config.secret,
        )
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                let event = StreamEvent::Error {
                    error_text: format!("Failed to connect to AI service: {}", e),
                };
                yield Ok(SseEvent::default().data(serialize_event(&event)));
                return;
            }
        };

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let error_body = response.text().await.unwrap_or_default();
            let event = parse_tollbooth_error(status, &error_body);
            yield Ok(SseEvent::default().data(serialize_event(&event)));
            return;
        }

        // AI SDK v6: Send text-start event
        let start_event = StreamEvent::TextStart { id: msg_id.clone() };
        yield Ok(SseEvent::default().data(serialize_event(&start_event)));

        // Stream the response
        let mut bytes_stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut full_content = String::new();
        let mut reasoning_content = String::new();
        let mut in_reasoning = false;

        while let Some(chunk) = bytes_stream.next().await {
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("Stream error: {}", e);
                    break;
                }
            };

            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Process complete SSE lines
            while let Some(line_end) = buffer.find('\n') {
                let line = buffer[..line_end].trim().to_string();
                buffer = buffer[line_end + 1..].to_string();

                if line.is_empty() || !line.starts_with("data: ") {
                    continue;
                }

                let data = &line[6..]; // Strip "data: " prefix

                if data == "[DONE]" {
                    break;
                }

                // Parse the SSE data as JSON
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                    // Extract delta from OpenAI format
                    if let Some(choices) = json.get("choices").and_then(|c| c.as_array()) {
                        if let Some(choice) = choices.first() {
                            if let Some(delta) = choice.get("delta") {
                                // Handle content delta
                                if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                    if !content.is_empty() {
                                        full_content.push_str(content);
                                        let event = StreamEvent::TextDelta {
                                            id: msg_id.clone(),
                                            delta: content.to_string(),
                                        };
                                        yield Ok(SseEvent::default().data(serialize_event(&event)));
                                    }
                                }

                                // Handle reasoning delta (provider-specific)
                                if let Some(reasoning) = delta.get("reasoning_content").and_then(|r| r.as_str()) {
                                    if !reasoning.is_empty() {
                                        if !in_reasoning {
                                            in_reasoning = true;
                                            let event = StreamEvent::ReasoningStart { id: msg_id.clone() };
                                            yield Ok(SseEvent::default().data(serialize_event(&event)));
                                        }
                                        reasoning_content.push_str(reasoning);
                                        let event = StreamEvent::ReasoningDelta {
                                            id: msg_id.clone(),
                                            delta: reasoning.to_string(),
                                        };
                                        yield Ok(SseEvent::default().data(serialize_event(&event)));
                                    }
                                }
                            }
                        }
                    }
                    // Gemini format: candidates[].content.parts[] with thought: true flag
                    else if let Some(candidates) = json.get("candidates").and_then(|c| c.as_array()) {
                        if let Some(candidate) = candidates.first() {
                            if let Some(content) = candidate.get("content") {
                                if let Some(parts) = content.get("parts").and_then(|p| p.as_array()) {
                                    for part in parts {
                                        let text = part.get("text").and_then(|t| t.as_str()).unwrap_or("");
                                        let is_thought = part.get("thought").and_then(|t| t.as_bool()).unwrap_or(false);

                                        if !text.is_empty() {
                                            if is_thought {
                                                // Handle as reasoning content
                                                if !in_reasoning {
                                                    in_reasoning = true;
                                                    let event = StreamEvent::ReasoningStart { id: msg_id.clone() };
                                                    yield Ok(SseEvent::default().data(serialize_event(&event)));
                                                }
                                                reasoning_content.push_str(text);
                                                let event = StreamEvent::ReasoningDelta {
                                                    id: msg_id.clone(),
                                                    delta: text.to_string(),
                                                };
                                                yield Ok(SseEvent::default().data(serialize_event(&event)));
                                            } else {
                                                // Handle as regular content
                                                // End reasoning if we were in it
                                                if in_reasoning {
                                                    in_reasoning = false;
                                                    let event = StreamEvent::ReasoningEnd { id: msg_id.clone() };
                                                    yield Ok(SseEvent::default().data(serialize_event(&event)));
                                                }
                                                full_content.push_str(text);
                                                let event = StreamEvent::TextDelta {
                                                    id: msg_id.clone(),
                                                    delta: text.to_string(),
                                                };
                                                yield Ok(SseEvent::default().data(serialize_event(&event)));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // End reasoning if we were in it
        if in_reasoning {
            let event = StreamEvent::ReasoningEnd { id: msg_id.clone() };
            yield Ok(SseEvent::default().data(serialize_event(&event)));
        }

        // Send text-end event
        let end_event = StreamEvent::TextEnd { id: msg_id.clone() };
        yield Ok(SseEvent::default().data(serialize_event(&end_event)));

        // Send [DONE] marker for SSE completion
        yield Ok(SseEvent::default().data("[DONE]"));

        // Save assistant message to session
        if !full_content.is_empty() {
            let provider = model.split('/').next().unwrap_or("unknown").to_string();
            let assistant_message = ChatMessage {
                role: "assistant".to_string(),
                content: full_content.clone(),
                timestamp: Utc::now().to_rfc3339(),
                model: Some(model.clone()),
                provider: Some(provider),
                agent_id: Some(request.agent_id),
                tool_calls: None,
                reasoning: if reasoning_content.is_empty() { None } else { Some(reasoning_content.clone()) },
                intent: None,
                subject: None,
            };

            if let Err(e) = append_message(&pool, session_id, assistant_message).await {
                tracing::error!("Failed to save assistant message: {}", e);
            }

            // Record estimated token usage
            // Estimate input tokens from the request messages
            let input_content: String = api_messages.iter()
                .filter_map(|m| m.get("content").and_then(|c| c.as_str()))
                .collect::<Vec<_>>()
                .join(" ");
            let estimated_input_tokens = estimate_tokens(&input_content);
            let estimated_output_tokens = estimate_tokens(&full_content);
            let estimated_reasoning_tokens = if reasoning_content.is_empty() {
                0
            } else {
                estimate_tokens(&reasoning_content)
            };

            let usage_data = UsageData {
                input_tokens: estimated_input_tokens,
                output_tokens: estimated_output_tokens,
                reasoning_tokens: estimated_reasoning_tokens,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
            };

            if let Err(e) = record_session_usage(&pool, session_id, &model, usage_data).await {
                tracing::warn!(
                    session_id = %session_id,
                    error = %e,
                    "Failed to record session usage"
                );
            }
        }
    })
}

/// Build provider-specific options for reasoning/thinking
fn build_provider_options(model: &str) -> Option<serde_json::Value> {
    let provider = model.split('/').next()?;

    match provider {
        "anthropic" => Some(serde_json::json!({
            "anthropic": {
                "thinking": {
                    "type": "enabled",
                    "budgetTokens": 10000
                }
            }
        })),
        "openai" => {
            // Only for reasoning models
            if model.contains("gpt-5") || model.contains("o1") || model.contains("o3") {
                Some(serde_json::json!({
                    "openai": {
                        "reasoningEffort": "medium",
                        "reasoningSummary": "auto"
                    }
                }))
            } else {
                None
            }
        }
        "google" => Some(serde_json::json!({
            "google": {
                "thinkingConfig": {
                    "enableThinking": true,
                    "thinkingBudget": 8000
                }
            }
        })),
        _ => None,
    }
}

/// Generate a random ID for messages
fn generate_id() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    let bytes: [u8; 8] = rng.random();
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_id() {
        let id1 = generate_id();
        let id2 = generate_id();
        assert_ne!(id1, id2);
        assert_eq!(id1.len(), 16); // 8 bytes = 16 hex chars
    }
}

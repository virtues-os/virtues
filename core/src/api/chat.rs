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

use crate::agent::{prompt::BASE_SYSTEM_PROMPT, AgentConfig, AgentEvent, AgentLoop};
use crate::api::chat_usage::{record_chat_usage, UsageData};
use crate::api::chats::{append_message, ChatMessage, ToolCall};
use crate::api::compaction::{build_context_for_llm, compact_chat, CompactionOptions};
use crate::api::token_estimation::{estimate_tokens, ContextStatus};
use crate::http_client::tollbooth_streaming_client;
use crate::middleware::auth::AuthUser;
use crate::tools::ToolContext;

// ============================================================================
// Types
// ============================================================================

/// Active page context for AI page editing
#[derive(Debug, Deserialize)]
pub struct ActivePageContext {
    /// Bound page ID for editing
    pub page_id: Option<String>,
    /// Page title (for better LLM context)
    pub page_title: Option<String>,
    /// Current content from Yjs document (source of truth for edits)
    pub content: Option<String>,
}

/// Chat request from frontend
#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<UIMessage>,
    #[serde(rename = "chatId")]
    pub chat_id: String,
    /// Model ID is required - frontend must send selected model from picker
    pub model: String,
    #[serde(rename = "agentId", default = "default_agent")]
    pub agent_id: String,
    /// Optional client-generated message ID for idempotency
    #[serde(rename = "messageId")]
    pub message_id: Option<String>,
    /// Optional space ID for auto-add to space_items (not stored on chat)
    #[serde(rename = "spaceId")]
    pub space_id: Option<String>,
    /// Optional active page context for AI page editing
    #[serde(rename = "activePage")]
    pub active_page: Option<ActivePageContext>,
    /// Optional Gemini thought signature for subsequent tool calls
    #[serde(rename = "thoughtSignature")]
    pub thought_signature: Option<String>,
    /// User's timezone (IANA format, e.g., "America/Los_Angeles")
    #[serde(default)]
    pub timezone: Option<String>,
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
    pub parts: Option<Vec<UIPart>>,
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
        /// Tool name - defaults to empty string if not provided (AI SDK may omit it)
        #[serde(rename = "toolName", default)]
        tool_name: String,
        #[serde(default)]
        input: serde_json::Value,
        #[serde(default)]
        state: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        output: Option<serde_json::Value>,
    },
    #[serde(rename = "tool-web_search")]
    ToolWebSearch {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        /// Tool name - defaults to "web_search" since we know the type
        #[serde(rename = "toolName", default = "default_web_search_tool_name")]
        tool_name: String,
        #[serde(default)]
        input: serde_json::Value,
        #[serde(default)]
        state: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        output: Option<serde_json::Value>,
    },
    #[serde(other)]
    Unknown,
}

/// Default tool name for web_search variant when toolName is missing from JSON
fn default_web_search_tool_name() -> String {
    "web_search".to_string()
}

/// Streaming event types (AI SDK v6 UI Message Stream Protocol)
///
/// These must exactly match the AI SDK's expected schema (strictObject validation).
/// See: https://sdk.vercel.ai/docs/ai-sdk-ui/stream-protocol
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

    // Tool input streaming (AI SDK v6 format)
    #[serde(rename = "tool-input-start")]
    ToolInputStart {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
    },
    #[serde(rename = "tool-input-delta")]
    ToolInputDelta {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "inputTextDelta")]
        input_text_delta: String,
    },
    #[serde(rename = "tool-input-available")]
    ToolInputAvailable {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        input: serde_json::Value,
    },

    // Tool output (AI SDK v6: tool-output-available)
    #[serde(rename = "tool-output-available")]
    ToolOutputAvailable {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        output: serde_json::Value,
    },

    // Error handling
    Error {
        #[serde(rename = "errorText")]
        error_text: String,
    },

    // Custom event to sync thought signature to client
    #[serde(rename = "thought-signature")]
    ThoughtSignature {
        signature: String,
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
                tracing::error!(
                    status,
                    code,
                    message,
                    "LLM provider authentication failed - check API keys"
                );
                format!(
                    "LLM provider auth failed ({}): {}. Check your API key configuration.",
                    status, message
                )
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
                tracing::error!(
                    status,
                    body,
                    "LLM provider authentication failed - check API keys"
                );
                format!(
                    "LLM provider auth failed ({}). Check your API key configuration.",
                    status
                )
            }
            _ => "An error occurred. Please try again.".to_string(),
        }
    };

    StreamEvent::Error { error_text }
}

/// Build system prompt with dynamic active page context
///
/// Combines the static BASE_SYSTEM_PROMPT with any active context (e.g., bound page).
/// Includes current date/time for temporal awareness in searches and responses.
fn build_system_prompt(active_page: Option<&ActivePageContext>, timezone: Option<&str>) -> String {
    let mut prompt = BASE_SYSTEM_PROMPT.to_string();

    // Add current date/time for temporal awareness
    let now = Utc::now();
    let date_str = now.format("%A, %B %d, %Y").to_string(); // e.g., "Thursday, January 29, 2026"
    let time_str = now.format("%H:%M UTC").to_string(); // e.g., "15:30 UTC"
    
    if let Some(tz) = timezone {
        prompt.push_str(&format!(
            "\n\n<datetime>\nToday is {}. Current time: {}. User's timezone: {}.\n</datetime>",
            date_str, time_str, tz
        ));
    } else {
        prompt.push_str(&format!(
            "\n\n<datetime>\nToday is {}. Current time: {}.\n</datetime>",
            date_str, time_str
        ));
    }

    if let Some(ctx) = active_page {
        if let Some(page_id) = &ctx.page_id {
            let title = ctx.page_title.as_deref().unwrap_or("Untitled");

            // Include the current content from Yjs if available
            // This is the source of truth - use this for edits, not the database content
            if let Some(content) = &ctx.content {
                prompt.push_str(&format!(
                    "\n\n<active_context>\nThe user has \"{}\" (id: {}) open for editing.\n\n<current_content>\n{}\n</current_content>\n\nUse the edit_page tool to edit this page. The content above is from the user's editor - use it as the source of truth for propose_edit operations. For full rewrites, use propose_replace_all.\n</active_context>",
                    title, page_id, content
                ));
            } else {
                prompt.push_str(&format!(
                    "\n\n<active_context>\nThe user has \"{}\" (id: {}) open for editing. Use the edit_page tool with get_content to read it, then propose_edit or propose_replace_all to edit it.\n</active_context>",
                    title, page_id
                ));
            }
        }
    }

    prompt
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
    // Validate model against registry
    let valid_models = match crate::api::models::list_models().await {
        Ok(models) => models,
        Err(e) => {
            tracing::error!("Failed to load models from registry: {}", e);
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

    // Use client-provided message ID for idempotency, or generate one
    let msg_id = request
        .message_id
        .clone()
        .unwrap_or_else(|| format!("msg_{}", generate_id()));

    // Ensure chat exists - use INSERT OR IGNORE to handle race conditions
    let chat_id_str = request.chat_id.clone();
    let title = request
        .messages
        .iter()
        .find(|m| m.role == "user")
        .and_then(|m| {
            m.content.clone().or_else(|| {
                m.parts.as_ref().and_then(|p| {
                    p.iter().find_map(|p| match p {
                        UIPart::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                })
            })
        })
        .unwrap_or_else(|| "New conversation".to_string());

    let title = if title.len() > 50 {
        format!("{}...", &title[..47])
    } else {
        title
    };

    // Use INSERT OR IGNORE to handle concurrent requests for same chat
    // Returns rows_affected = 1 if inserted, 0 if already exists
    let insert_result =
        sqlx::query("INSERT OR IGNORE INTO chats (id, title, message_count) VALUES ($1, $2, 0)")
            .bind(&chat_id_str)
            .bind(&title)
            .execute(&pool)
            .await;

    let chat_was_created = match insert_result {
        Ok(result) => result.rows_affected() > 0,
        Err(e) => {
            tracing::error!("Failed to create chat: {}", e);
            false
        }
    };

    // Auto-add to space_items if chat was just created and space_id provided (not system space)
    if chat_was_created {
        if let Some(space_id) = &request.space_id {
            if space_id != "space_system" {
                let url = format!("/chat/{}", chat_id_str);
                if let Err(e) = crate::api::views::add_space_item(&pool, space_id, &url).await {
                    tracing::warn!("Failed to auto-add chat to space {}: {}", space_id, e);
                }
            }
        }
    }

    // Save the user message to the chat
    // Find the last user message from the request
    if let Some(last_user_msg) = request.messages.iter().rev().find(|m| m.role == "user") {
        let user_content = last_user_msg.content.clone().unwrap_or_else(|| {
            last_user_msg
                .parts
                .as_ref()
                .map(|p| {
                    p.iter()
                        .filter_map(|p| match p {
                            UIPart::Text { text } => Some(text.clone()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default()
        });

        let user_message = ChatMessage {
            id: None,
            role: "user".to_string(),
            content: user_content,
            timestamp: Utc::now().to_rfc3339(),
            model: None,
            provider: None,
            agent_id: None,
            parts: last_user_msg.parts.clone(),
            tool_calls: None,
            reasoning: None,
            intent: None,
            subject: None,
            thought_signature: None,
        };

        if let Err(e) = append_message(&pool, request.chat_id.clone(), user_message).await {
            tracing::error!("Failed to save user message: {}", e);
        }
    }

    // Check if compaction is needed before sending to LLM
    let compaction_status = crate::api::chat_usage::check_compaction_needed(
        &pool,
        request.chat_id.clone(),
        &request.model,
    )
    .await;

    // Auto-compact if critical (>= 85% context usage)
    if matches!(compaction_status, Ok(ContextStatus::Critical)) {
        tracing::info!(
            chat_id = %request.chat_id,
            "Context critical, auto-compacting chat"
        );
        if let Err(e) =
            compact_chat(&pool, request.chat_id.clone(), CompactionOptions::default()).await
        {
            tracing::warn!(
                chat_id = %request.chat_id,
                error = %e,
                "Auto-compaction failed, continuing with full context"
            );
        }
    }

    // Load chat from DB and build context with compaction summary
    let chat_row = match sqlx::query(
        r#"SELECT conversation_summary, summary_up_to_index
           FROM chats WHERE id = ?"#,
    )
    .bind(&chat_id_str)
    .fetch_one(&pool)
    .await
    {
        Ok(row) => row,
        Err(e) => {
            tracing::error!("Failed to load chat: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ChatError {
                    error: "Failed to load chat".to_string(),
                    details: Some(e.to_string()),
                }),
            )
                .into_response();
        }
    };

    use sqlx::Row;
    let conversation_summary: Option<String> = chat_row.get("conversation_summary");
    let summary_up_to_index: i32 = chat_row.get("summary_up_to_index");

    // Load messages from normalized table
    let message_rows = match sqlx::query(
        r#"
        SELECT
            id, role, content, created_at, model, provider, agent_id,
            reasoning, tool_calls, intent, subject, thought_signature, parts
        FROM chat_messages
        WHERE chat_id = ?
        ORDER BY sequence_num ASC
        "#,
    )
    .bind(&chat_id_str)
    .fetch_all(&pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!("Failed to load messages for chat {}: {}", chat_id_str, e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ChatError {
                    error: "Failed to load messages".to_string(),
                    details: Some(e.to_string()),
                }),
            )
                .into_response();
        }
    };

    // Convert rows to ChatMessage
    let messages: Vec<ChatMessage> = message_rows
        .into_iter()
        .map(|msg| {
            let id: String = msg.get("id");
            let role: String = msg.get("role");
            let content: String = msg.get("content");
            let created_at: String = msg.get("created_at");
            let model: Option<String> = msg.get("model");
            let provider: Option<String> = msg.get("provider");
            let agent_id: Option<String> = msg.get("agent_id");
            let reasoning: Option<String> = msg.get("reasoning");
            let tool_calls_raw: Option<String> = msg.get("tool_calls");
            let intent_raw: Option<String> = msg.get("intent");
            let subject: Option<String> = msg.get("subject");
            let thought_signature: Option<String> = msg.get("thought_signature");
            let parts_raw: Option<String> = msg.get("parts");

            // Parse JSON fields
            let tool_calls = tool_calls_raw.and_then(|t| serde_json::from_str(&t).ok());
            let intent = intent_raw.and_then(|i| serde_json::from_str(&i).ok());
            let parts = parts_raw.and_then(|p| serde_json::from_str(&p).ok());

            ChatMessage {
                id: Some(id),
                role,
                content,
                timestamp: created_at,
                model,
                provider,
                agent_id,
                parts,
                reasoning,
                tool_calls,
                intent,
                subject,
                thought_signature,
            }
        })
        .collect();

    // Build system prompt with active page context and timezone
    let system_prompt = build_system_prompt(request.active_page.as_ref(), request.timezone.as_deref());

    // Build context using compaction summary if available
    let api_messages = build_context_for_llm(
        &messages,
        conversation_summary.as_deref(),
        summary_up_to_index as usize,
        Some(&system_prompt),
    );

    // Create the streaming response with agent loop for tool execution
    let stream = create_agent_stream(pool, tollbooth_config, request, api_messages, msg_id);

    // AI SDK v6 requires this header for UI Message Stream Protocol
    let mut response = Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::new())
        .into_response();

    response.headers_mut().insert(
        axum::http::header::HeaderName::from_static("x-vercel-ai-ui-message-stream"),
        axum::http::HeaderValue::from_static("v1"),
    );

    // We can't easily send the signature in headers for a streaming response
    // because it's discovered DURING the stream.
    // However, the frontend can extract it from the stream itself if we emit a special event.

    response.into_response()
}

/// Legacy: Create SSE stream without agent loop (no tool execution)
/// Kept for reference. Use create_agent_stream instead.
#[allow(dead_code)]
fn _create_chat_stream_legacy(
    pool: SqlitePool,
    tollbooth_config: Arc<TollboothConfig>,
    request: ChatRequest,
    api_messages: Vec<serde_json::Value>,
    msg_id: String,
) -> Pin<Box<dyn Stream<Item = Result<SseEvent, Infallible>> + Send>> {
    let model = request.model.clone();
    let chat_id = request.chat_id.clone();

    Box::pin(async_stream::stream! {
        // Build provider options for reasoning if applicable
        let provider_options = _build_provider_options_legacy(&model);

        // Get tool definitions for LLM
        let tools = crate::tools::get_tool_definitions_for_llm();

        // Prepare request body
        let mut body = serde_json::json!({
            "model": model,
            "messages": api_messages,
            "stream": true
        });

        // Add tools if available
        if !tools.is_empty() {
            body["tools"] = serde_json::json!(tools);
            body["tool_choice"] = serde_json::json!("auto");
        }

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

        // Tool call tracking: track active tool calls by index
        let mut tool_calls_map: std::collections::HashMap<i64, (String, String, String)> = std::collections::HashMap::new(); // index -> (id, name, args)
        let mut tool_calls_started: std::collections::HashSet<i64> = std::collections::HashSet::new();

        // Actual token usage from provider (if available)
        let mut actual_input_tokens: Option<i64> = None;
        let mut actual_output_tokens: Option<i64> = None;
        let mut actual_reasoning_tokens: Option<i64> = None;

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

                                // Handle tool call streaming (OpenAI format)
                                if let Some(tool_calls) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                                    for tool_call in tool_calls {
                                        let idx = tool_call.get("index").and_then(|i| i.as_i64()).unwrap_or(0);
                                        let tc_id = tool_call.get("id").and_then(|i| i.as_str()).unwrap_or("");

                                        if let Some(function) = tool_call.get("function") {
                                            let name = function.get("name").and_then(|n| n.as_str()).unwrap_or("");
                                            let args = function.get("arguments").and_then(|a| a.as_str()).unwrap_or("");

                                            // Track or update this tool call
                                            let entry = tool_calls_map.entry(idx).or_insert_with(|| (tc_id.to_string(), String::new(), String::new()));
                                            if !tc_id.is_empty() {
                                                entry.0 = tc_id.to_string();
                                            }
                                            if !name.is_empty() {
                                                entry.1 = name.to_string();
                                            }
                                            entry.2.push_str(args);

                                            // Emit start event on first encounter
                                            if !tc_id.is_empty() && !name.is_empty() && !tool_calls_started.contains(&idx) {
                                                tool_calls_started.insert(idx);
                                                let event = StreamEvent::ToolInputStart {
                                                    tool_call_id: tc_id.to_string(),
                                                    tool_name: name.to_string(),
                                                };
                                                yield Ok(SseEvent::default().data(serialize_event(&event)));
                                            }

                                            // Emit delta for arguments
                                            if !args.is_empty() && tool_calls_started.contains(&idx) {
                                                let event = StreamEvent::ToolInputDelta {
                                                    tool_call_id: entry.0.clone(),
                                                    input_text_delta: args.to_string(),
                                                };
                                                yield Ok(SseEvent::default().data(serialize_event(&event)));
                                            }
                                        }
                                    }
                                }
                            }

                            // Check for finish_reason to end tool calls
                            if let Some(finish_reason) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                                if finish_reason == "tool_calls" || finish_reason == "stop" {
                                    // End all active tool calls
                                    // TODO: Implement server-side tool execution here:
                                    // 1. For each tool call in tool_calls_map:
                                    //    - Parse args JSON
                                    //    - Call tool_executor.execute(name, args, context)
                                    //    - Populate result field with tool output
                                    // 2. If finish_reason == "tool_calls":
                                    //    - Make another API call with tool results
                                    //    - Continue streaming the response
                                    //
                                    // For now, tools are sent to frontend for display only.
                                    // Use the ToolExecutor from AppState for execution:
                                    //   crate::tools::ToolExecutor::from_env(pool)
                                    for (idx, (tc_id, name, args_str)) in tool_calls_map.iter() {
                                        if tool_calls_started.contains(idx) {
                                            // Parse args and emit input-available
                                            let input = serde_json::from_str(args_str).unwrap_or(serde_json::Value::Null);
                                            let event = StreamEvent::ToolInputAvailable {
                                                tool_call_id: tc_id.clone(),
                                                tool_name: name.clone(),
                                                input,
                                            };
                                            yield Ok(SseEvent::default().data(serialize_event(&event)));
                                            // Note: In legacy mode, tool results would need to be executed here
                                            // For now, we don't emit tool-result as tools aren't executed
                                        }
                                    }
                                    tool_calls_started.clear();
                                }
                            }
                        }
                    }

                    // Extract actual token usage from OpenAI format
                    if let Some(usage) = json.get("usage") {
                        actual_input_tokens = usage.get("prompt_tokens").and_then(|t| t.as_i64());
                        actual_output_tokens = usage.get("completion_tokens").and_then(|t| t.as_i64());
                        // Some providers include reasoning tokens separately
                        if let Some(completion_details) = usage.get("completion_tokens_details") {
                            actual_reasoning_tokens = completion_details.get("reasoning_tokens").and_then(|t| t.as_i64());
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

                        // Extract actual token usage from Gemini format
                        if let Some(usage) = json.get("usageMetadata") {
                            actual_input_tokens = usage.get("promptTokenCount").and_then(|t| t.as_i64());
                            actual_output_tokens = usage.get("candidatesTokenCount").and_then(|t| t.as_i64());
                            // Gemini may include thoughtsTokenCount for reasoning
                            if actual_reasoning_tokens.is_none() {
                                actual_reasoning_tokens = usage.get("thoughtsTokenCount").and_then(|t| t.as_i64());
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

        // Save assistant message to chat
        if !full_content.is_empty() {
            let provider = model.split('/').next().unwrap_or("unknown").to_string();
            let assistant_message =             ChatMessage {
                id: None,
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
                thought_signature: None,
                parts: None,
            };

            if let Err(e) = append_message(&pool, chat_id.clone(), assistant_message).await {
                tracing::error!("Failed to save assistant message: {}", e);
            }

            // Record token usage - use actual from provider if available, else estimate
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

            // Prefer actual usage from provider, fall back to estimates
            let final_input = actual_input_tokens.unwrap_or(estimated_input_tokens as i64);
            let final_output = actual_output_tokens.unwrap_or(estimated_output_tokens as i64);
            let final_reasoning = actual_reasoning_tokens.unwrap_or(estimated_reasoning_tokens as i64);

            let is_actual = actual_input_tokens.is_some() || actual_output_tokens.is_some();
            tracing::debug!(
                chat_id = %chat_id,
                input_tokens = final_input,
                output_tokens = final_output,
                reasoning_tokens = final_reasoning,
                is_actual = is_actual,
                "Recording token usage ({})", if is_actual { "actual" } else { "estimated" }
            );

            let usage_data = UsageData {
                input_tokens: final_input,
                output_tokens: final_output,
                reasoning_tokens: final_reasoning,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
            };

            if let Err(e) = record_chat_usage(&pool, chat_id.clone(), &model, usage_data).await {
                tracing::warn!(
                    chat_id = %chat_id,
                    error = %e,
                    "Failed to record chat usage"
                );
            }
        }
    })
}

/// Create the SSE stream using the AgentLoop for tool execution
fn create_agent_stream(
    pool: SqlitePool,
    tollbooth_config: Arc<TollboothConfig>,
    request: ChatRequest,
    api_messages: Vec<serde_json::Value>,
    msg_id: String,
) -> Pin<Box<dyn Stream<Item = Result<SseEvent, Infallible>> + Send>> {
    let model = request.model.clone();
    let chat_id = request.chat_id.clone();
    let agent_id = request.agent_id.clone();

    Box::pin(async_stream::stream! {
        // Create AgentLoop
        let agent = AgentLoop::new(
            pool.clone(),
            tollbooth_config.url.clone(),
            tollbooth_config.user_id.clone(),
            tollbooth_config.secret.clone(),
        )
        .with_config(AgentConfig {
            max_steps: 10,
            tool_timeout: std::time::Duration::from_secs(30),
            parallel_tools: true,
        });

        // Build tool context from request
        let context = ToolContext {
            page_id: request.active_page.as_ref().and_then(|p| p.page_id.clone()),
            user_id: None,
            space_id: request.space_id.clone(),
        };

        // Get tool definitions
        let tools = crate::tools::get_tool_definitions_for_llm();

        // Send text-start event
        let start_event = StreamEvent::TextStart { id: msg_id.clone() };
        yield Ok(SseEvent::default().data(serialize_event(&start_event)));

        // Track accumulated content
        let mut full_content = String::new();
        let mut reasoning_content = String::new();
        let mut in_reasoning = false;

        // Token usage tracking
        let mut total_input_tokens: u32 = 0;
        let mut total_output_tokens: u32 = 0;

        // Tool call tracking for persistence
        let mut all_tool_calls: Vec<ToolCall> = Vec::new();

        // Run the agent loop
        let mut agent_stream = agent.run(
            model.clone(),
            api_messages.clone(),
            tools,
            context,
            request.thought_signature.clone().or_else(|| {
                // Fallback: look for signature in the last assistant message of the history
                api_messages.iter().rev()
                    .filter_map(|m| m.get("thought_signature").and_then(|s| s.as_str()))
                    .next()
                    .map(|s| s.to_string())
            })
        );

        while let Some(event) = agent_stream.next().await {
            match event {
                AgentEvent::TextDelta { content } => {
                    // End reasoning if we were in it
                    if in_reasoning {
                        in_reasoning = false;
                        let event = StreamEvent::ReasoningEnd { id: msg_id.clone() };
                        yield Ok(SseEvent::default().data(serialize_event(&event)));
                    }
                    full_content.push_str(&content);
                    let event = StreamEvent::TextDelta {
                        id: msg_id.clone(),
                        delta: content,
                    };
                    yield Ok(SseEvent::default().data(serialize_event(&event)));
                }

                AgentEvent::ReasoningDelta { content } => {
                    if !in_reasoning {
                        in_reasoning = true;
                        let event = StreamEvent::ReasoningStart { id: msg_id.clone() };
                        yield Ok(SseEvent::default().data(serialize_event(&event)));
                    }
                    reasoning_content.push_str(&content);
                    let event = StreamEvent::ReasoningDelta {
                        id: msg_id.clone(),
                        delta: content,
                    };
                    yield Ok(SseEvent::default().data(serialize_event(&event)));
                }

                AgentEvent::ToolCallStart { id, name, args } => {
                    // Track tool call for persistence
                    all_tool_calls.push(ToolCall {
                        tool_name: name.clone(),
                        tool_call_id: Some(id.clone()),
                        arguments: args.clone().unwrap_or(serde_json::Value::Null),
                        result: None, // Will be populated by ToolCallResult
                        timestamp: Utc::now().to_rfc3339(),
                    });
                    // AI SDK v6: tool-input-start event
                    let event = StreamEvent::ToolInputStart {
                        tool_call_id: id,
                        tool_name: name,
                    };
                    yield Ok(SseEvent::default().data(serialize_event(&event)));
                }

                AgentEvent::ToolCallArgsPartial { id, args_delta } => {
                    // AI SDK v6: tool-input-delta event
                    let event = StreamEvent::ToolInputDelta {
                        tool_call_id: id,
                        input_text_delta: args_delta,
                    };
                    yield Ok(SseEvent::default().data(serialize_event(&event)));
                }

                AgentEvent::ToolCallArgsComplete { id, args } => {
                    // AI SDK v6: tool-input-available event (args parsing complete)
                    // Find the tool name from tracked tool calls
                    let tool_name = all_tool_calls.iter()
                        .find(|tc| tc.tool_call_id.as_deref() == Some(&id))
                        .map(|tc| tc.tool_name.clone())
                        .unwrap_or_default();
                    let event = StreamEvent::ToolInputAvailable {
                        tool_call_id: id,
                        tool_name,
                        input: args,
                    };
                    yield Ok(SseEvent::default().data(serialize_event(&event)));
                }

                AgentEvent::ToolCallResult { id, result, success: _, error: _ } => {
                    // Update the tracked tool call with the result
                    if let Some(tc) = all_tool_calls.iter_mut().find(|tc| tc.tool_call_id.as_deref() == Some(&id)) {
                        tc.result = Some(result.clone());
                    }
                    // AI SDK v6: tool-output-available event
                    let event = StreamEvent::ToolOutputAvailable {
                        tool_call_id: id,
                        output: result,
                    };
                    yield Ok(SseEvent::default().data(serialize_event(&event)));
                }

                AgentEvent::Usage { prompt_tokens, completion_tokens, total_tokens: _ } => {
                    total_input_tokens += prompt_tokens;
                    total_output_tokens += completion_tokens;
                }

                AgentEvent::ThoughtSignature { signature } => {
                    let event = StreamEvent::ThoughtSignature { signature };
                    yield Ok(SseEvent::default().data(serialize_event(&event)));
                }

                AgentEvent::Error { message, code: _, recoverable: _ } => {
                    let event = StreamEvent::Error { error_text: message };
                    yield Ok(SseEvent::default().data(serialize_event(&event)));
                }

                // Events we don't need to forward to client
                AgentEvent::LoopStarted { .. } |
                AgentEvent::StepComplete { .. } |
                AgentEvent::MessageId { .. } |
                AgentEvent::Done { .. } => {}
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

        // Send [DONE] marker
        yield Ok(SseEvent::default().data("[DONE]"));

        // Save assistant message to chat
        if !full_content.is_empty() {
            let provider = model.split('/').next().unwrap_or("unknown").to_string();
            let assistant_message = ChatMessage {
                id: None,
                role: "assistant".to_string(),
                content: full_content.clone(),
                timestamp: Utc::now().to_rfc3339(),
                model: Some(model.clone()),
                provider: Some(provider),
                agent_id: Some(agent_id),
                tool_calls: if all_tool_calls.is_empty() { None } else { Some(all_tool_calls.clone()) },
                reasoning: if reasoning_content.is_empty() { None } else { Some(reasoning_content.clone()) },
                intent: None,
                subject: None,
                thought_signature: None,
                parts: None,
            };

            if let Err(e) = append_message(&pool, chat_id.clone(), assistant_message).await {
                tracing::error!("Failed to save assistant message: {}", e);
            }

            // Record token usage
            let usage_data = UsageData {
                input_tokens: total_input_tokens as i64,
                output_tokens: total_output_tokens as i64,
                reasoning_tokens: 0,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
            };

            if let Err(e) = record_chat_usage(&pool, chat_id.clone(), &model, usage_data).await {
                tracing::warn!(
                    chat_id = %chat_id,
                    error = %e,
                    "Failed to record chat usage"
                );
            }
        }
    })
}

/// Build provider-specific options for reasoning/thinking
/// Note: This is also implemented in agent/stream.rs
#[allow(dead_code)]
fn _build_provider_options_legacy(model: &str) -> Option<serde_json::Value> {
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

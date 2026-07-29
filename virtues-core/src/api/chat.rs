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
//! Streams responses through virtues-api for budget enforcement and usage tracking.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response, Sse},
    Json,
};
use chrono::Utc;
use chrono_tz::Tz;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use tokio_stream::StreamExt;

use crate::agent::{AgentConfig, AgentEvent, AgentLoop};
use crate::api::chat_usage::{record_chat_usage, UsageData};
use crate::api::chats::{append_message, ChatMessage, ToolCall};
use crate::api::compaction::{build_context_for_llm, compact_chat, CompactionOptions};
use crate::api::token_estimation::ContextStatus;
use crate::middleware::auth::AuthUser;
use crate::server::yjs::YjsState;
use crate::tools::ToolContext;
use crate::types::Timestamp;
use tokio_util::sync::CancellationToken;

// ============================================================================
// Cancellation State
// ============================================================================

/// Shared state for tracking active chat requests that can be cancelled
#[derive(Clone, Default)]
pub struct ChatCancellationState {
    /// Map of chat_id -> cancellation token for active requests
    tokens: Arc<std::sync::RwLock<std::collections::HashMap<String, CancellationToken>>>,
}

impl ChatCancellationState {
    pub fn new() -> Self {
        Self {
            tokens: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Register a new chat request and get its cancellation token
    pub fn register(&self, chat_id: &str) -> CancellationToken {
        let token = CancellationToken::new();
        // Recover from poisoned lock - the data is still valid
        let mut guard = self.tokens.write().unwrap_or_else(|e| e.into_inner());
        guard.insert(chat_id.to_string(), token.clone());
        token
    }

    /// Cancel an active chat request
    pub fn cancel(&self, chat_id: &str) -> bool {
        // Recover from poisoned lock - the data is still valid
        let guard = self.tokens.read().unwrap_or_else(|e| e.into_inner());
        if let Some(token) = guard.get(chat_id) {
            token.cancel();
            true
        } else {
            false
        }
    }

    /// Remove a chat request (called when stream completes)
    pub fn remove(&self, chat_id: &str) {
        // Recover from poisoned lock - the data is still valid
        let mut guard = self.tokens.write().unwrap_or_else(|e| e.into_inner());
        guard.remove(chat_id);
    }

    /// Check if a chat has an active request
    pub fn is_active(&self, chat_id: &str) -> bool {
        // Recover from poisoned lock - the data is still valid
        let guard = self.tokens.read().unwrap_or_else(|e| e.into_inner());
        guard.contains_key(chat_id)
    }
}

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
    /// The Notebook (room) this chat lives in. Stored on the chat and inlined into
    /// the system prompt as a salience lens (name + memo + member URLs).
    #[serde(rename = "notebookId", default)]
    pub notebook_id: Option<String>,
    /// Optional active page context for AI page editing
    #[serde(rename = "activePage")]
    pub active_page: Option<ActivePageContext>,
    /// Optional Gemini thought signature for subsequent tool calls
    #[serde(rename = "thoughtSignature")]
    pub thought_signature: Option<String>,
    /// User's timezone (IANA format, e.g., "America/Los_Angeles")
    #[serde(default)]
    pub timezone: Option<String>,
    /// AI persona for system prompt customization (per-chat)
    #[serde(default = "default_persona")]
    pub persona: String,
    /// Agent mode controlling tool availability (agent, chat, research)
    #[serde(rename = "agentMode", default = "default_agent_mode")]
    pub agent_mode: String,
    /// Retrieval scope for notebook chats: "open" (default — whole graph,
    /// notebook items up-weighted) or "scoped" (grounded — items only).
    /// Ignored without a notebook_id.
    #[serde(rename = "chatMode", default = "default_chat_mode")]
    pub chat_mode: String,
}

fn default_chat_mode() -> String {
    "open".to_string()
}

fn default_agent() -> String {
    "auto".to_string()
}

fn default_persona() -> String {
    "default".to_string()
}

fn default_agent_mode() -> String {
    "chat".to_string()
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
    /// Checkpoint from conversation compaction
    #[serde(rename = "checkpoint")]
    Checkpoint {
        /// Summary version number
        version: i32,
        /// Number of messages that were summarized
        messages_summarized: i32,
        /// The summary text (XML structured)
        summary: String,
        /// When the checkpoint was created
        timestamp: String,
    },
    /// File attachment (image / PDF / audio) — matches the AI SDK v6 file part.
    /// `url` is a data URL (base64) so it round-trips to the provider and renders
    /// on reload without a separate authenticated fetch.
    #[serde(rename = "file")]
    File {
        #[serde(rename = "mediaType", default)]
        media_type: String,
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
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

    // Deep Research subagent status (data event for the live panel)
    #[serde(rename = "subagent-status")]
    SubagentStatus {
        #[serde(rename = "dispatchId")]
        dispatch_id: u64,
        #[serde(rename = "subagentId")]
        subagent_id: u32,
        title: String,
        model: String,
        status: String,
        tokens: u32,
    },

    // Checkpoint event emitted after auto-compaction
    #[serde(rename = "checkpoint")]
    Checkpoint {
        /// Message ID for the checkpoint
        id: String,
        /// Summary version number
        version: i32,
        /// Number of messages that were summarized
        #[serde(rename = "messagesSummarized")]
        messages_summarized: i32,
        /// The summary text (XML structured)
        summary: String,
        /// When the checkpoint was created
        timestamp: String,
    },
}

// ============================================================================
// AI SDK v6 Data Event Types
// ============================================================================

/// AI SDK v6 data event wrapper for custom events
/// Custom events must use "data-*" prefix to be properly handled by DefaultChatTransport
#[derive(Debug, Serialize)]
struct DataEvent<T: Serialize> {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    data: T,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    transient: bool,
}

/// Checkpoint data payload for AI SDK v6 data event
#[derive(Debug, Serialize)]
struct CheckpointData {
    version: i32,
    #[serde(rename = "messagesSummarized")]
    messages_summarized: i32,
    summary: String,
    timestamp: String,
}

/// Thought signature data payload for AI SDK v6 data event
#[derive(Debug, Serialize)]
struct ThoughtSignatureData {
    signature: String,
}

/// Subagent status payload for AI SDK v6 data event (live Deep Research panel)
#[derive(Debug, Serialize)]
struct SubagentStatusData {
    #[serde(rename = "dispatchId")]
    dispatch_id: u64,
    #[serde(rename = "subagentId")]
    subagent_id: u32,
    title: String,
    model: String,
    /// "thinking" | "done" | "failed"
    status: String,
    tokens: u32,
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

/// Fetch the latest checkpoint message from a chat and convert to StreamEvent
async fn get_latest_checkpoint(pool: &PgPool, chat_id: &str) -> Option<StreamEvent> {
    use sqlx::Row;

    let row = sqlx::query(
        r#"
        SELECT id, parts, created_at
        FROM app_chat_messages
        WHERE chat_id = $1 AND role = 'checkpoint'
        ORDER BY sequence_num DESC
        LIMIT 1
        "#,
    )
    .bind(chat_id)
    .fetch_optional(pool)
    .await
    .ok()??;

    let id: String = row.get("id");
    let parts_json: Option<String> = row.get("parts");
    let created_at: String = row.get("created_at");

    // Parse parts JSON to extract checkpoint data
    let parts: Vec<UIPart> = parts_json
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();

    // Find checkpoint part
    for part in parts {
        if let UIPart::Checkpoint {
            version,
            messages_summarized,
            summary,
            timestamp,
        } = part
        {
            return Some(StreamEvent::Checkpoint {
                id,
                version,
                messages_summarized,
                summary,
                // Use checkpoint timestamp, falling back to created_at if empty
                timestamp: if timestamp.is_empty() { created_at } else { timestamp },
            });
        }
    }

    None
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Safely serialize a stream event to JSON
/// Custom events (checkpoint, thought-signature) are wrapped in AI SDK v6 data-* format
fn serialize_event(event: &StreamEvent) -> String {
    match event {
        // Wrap checkpoint events in AI SDK v6 data event format
        StreamEvent::Checkpoint { id, version, messages_summarized, summary, timestamp } => {
            let wrapper = DataEvent {
                event_type: "data-checkpoint".to_string(),
                id: Some(id.clone()),
                data: CheckpointData {
                    version: *version,
                    messages_summarized: *messages_summarized,
                    summary: summary.clone(),
                    timestamp: timestamp.clone(),
                },
                transient: false, // Checkpoint should persist in message parts
            };
            serde_json::to_string(&wrapper).unwrap_or_else(|e| {
                tracing::error!("Failed to serialize checkpoint event: {}", e);
                r#"{"type":"error","errorText":"Serialization error"}"#.to_string()
            })
        }
        // Wrap thought signature events in AI SDK v6 data event format
        StreamEvent::ThoughtSignature { signature } => {
            let wrapper = DataEvent {
                event_type: "data-thought-signature".to_string(),
                id: None,
                data: ThoughtSignatureData { signature: signature.clone() },
                transient: true, // Ephemeral - only needed during streaming session
            };
            serde_json::to_string(&wrapper).unwrap_or_else(|e| {
                tracing::error!("Failed to serialize thought-signature event: {}", e);
                r#"{"type":"error","errorText":"Serialization error"}"#.to_string()
            })
        }
        // Wrap subagent status in AI SDK v6 data event format (transient — live panel only)
        StreamEvent::SubagentStatus { dispatch_id, subagent_id, title, model, status, tokens } => {
            let wrapper = DataEvent {
                event_type: "data-subagent".to_string(),
                id: None,
                data: SubagentStatusData {
                    dispatch_id: *dispatch_id,
                    subagent_id: *subagent_id,
                    title: title.clone(),
                    model: model.clone(),
                    status: status.clone(),
                    tokens: *tokens,
                },
                transient: true,
            };
            serde_json::to_string(&wrapper).unwrap_or_else(|e| {
                tracing::error!("Failed to serialize subagent event: {}", e);
                r#"{"type":"error","errorText":"Serialization error"}"#.to_string()
            })
        }
        // All other events use standard serde serialization
        _ => serde_json::to_string(event).unwrap_or_else(|e| {
            tracing::error!("Failed to serialize stream event: {}", e);
            r#"{"type":"error","errorText":"Internal serialization error"}"#.to_string()
        }),
    }
}

/// Maximum characters for page content in system prompt
/// ~10K chars ≈ 2.5K tokens, leaving room for rest of context
const MAX_PAGE_CONTENT_CHARS: usize = 10_000;

/// Build narrative identity content for the system prompt.
///
/// Queries the user's narrative identity (present-orientation self-portrait) to provide
/// Returns the user's narrative identity content (up to 800 chars), or empty string.
async fn build_narrative_identity(pool: &PgPool) -> String {
    match sqlx::query_scalar::<_, String>(
        "SELECT content FROM wiki_narrative_identity LIMIT 1"
    )
    .fetch_one(pool)
    .await
    {
        Ok(content) if !content.is_empty() => {
            content.chars().take(800).collect()
        }
        _ => String::new(),
    }
}

/// Build user context block for system prompt enrichment.
///
/// Queries lightweight indexed data (~20ms total) to give the LLM personal context:
/// - Identity: occupation, employer, home location
/// - Recent days: last 3 autobiographies (truncated)
/// - Connected sources: active data source names
async fn build_user_context(pool: &PgPool, user_name: &str) -> Option<String> {
    let mut sections = Vec::new();

    // 1. Identity — occupation, employer, home place
    if let Ok(row) = sqlx::query_as::<_, (Option<String>, Option<String>, Option<String>)>(
        r#"SELECT p.occupation, p.employer, wp.name
         FROM app_user_profile p
         LEFT JOIN wiki_places wp ON p.home_place_id = wp.id
         WHERE p.id = '00000000-0000-0000-0000-000000000001'"#
    )
    .fetch_one(pool)
    .await
    {
        let mut parts = Vec::new();
        if let (Some(occ), Some(emp)) = (&row.0, &row.1) {
            parts.push(format!("{} at {}", occ, emp));
        } else if let Some(occ) = &row.0 {
            parts.push(occ.clone());
        }
        if let Some(place) = &row.2 {
            parts.push(format!("Lives in {}", place));
        }
        if !parts.is_empty() {
            sections.push(format!("<identity>{}</identity>", parts.join(". ")));
        }
    }

    // 2. Recent days — last 3 autobiographies
    if let Ok(rows) = sqlx::query_as::<_, (String, Option<String>)>(
        r#"SELECT date, autobiography FROM wiki_days
         WHERE autobiography IS NOT NULL AND autobiography != ''
         ORDER BY date DESC LIMIT 3"#
    )
    .fetch_all(pool)
    .await
    {
        if !rows.is_empty() {
            let mut day_lines = Vec::new();
            for (date, auto) in &rows {
                if let Some(text) = auto {
                    let truncated = if text.chars().count() > 300 {
                        let t: String = text.chars().take(300).collect();
                        format!("{}...", t)
                    } else {
                        text.clone()
                    };
                    day_lines.push(format!("{}: {}", date, truncated));
                }
            }
            if !day_lines.is_empty() {
                sections.push(format!("<recent_days>\n{}\n</recent_days>", day_lines.join("\n")));
            }
        }
    }

    // 3. Connected sources — active credential names
    if let Ok(rows) = sqlx::query_as::<_, (String,)>(
        "SELECT name FROM credentials WHERE status = 'active' ORDER BY name"
    )
    .fetch_all(pool)
    .await
    {
        if !rows.is_empty() {
            let names: Vec<&str> = rows.iter().map(|r| r.0.as_str()).collect();
            sections.push(format!("<connected_sources>{}</connected_sources>", names.join(", ")));
        }
    }

    if sections.is_empty() {
        None
    } else {
        Some(format!(
            "\n\n<user_context>\nBackground context about {}. Reference when relevant; do not recite unprompted.\n{}\n</user_context>",
            user_name,
            sections.join("\n")
        ))
    }
}

/// Build system prompt with dynamic context and personalization.
///
/// Assembles: identity → persona → narrative_identity → tools → datetime → user_context → active_page.
/// Loads user name, assistant name, persona, and narrative identity from profiles.
/// When `is_new_user` is true, appends the onboarding prompt for first conversations.
async fn build_system_prompt(
    pool: &PgPool,
    active_page: Option<&ActivePageContext>,
    timezone: Option<&str>,
    agent_mode: &str,
    persona_id: &str,
    is_new_user: bool,
    notebook_id: Option<&str>,
) -> String {
    use crate::agent::prompt::build_personalized_prompt;
    use crate::api::assistant_profile::get_assistant_name;
    use crate::api::personas::get_persona_content;
    use crate::api::profile::get_display_name;

    // Load personalization from profiles (with fallbacks)
    let assistant_name = get_assistant_name(pool).await.unwrap_or_else(|_| "Ari".to_string());
    let user_name = get_display_name(pool).await.unwrap_or_else(|_| "there".to_string());

    // Load persona content from database (or fallback to registry default)
    let persona_content = get_persona_content(pool, persona_id).await.ok().flatten();

    // Build narrative identity (user's present-orientation self-portrait)
    let narrative_identity = build_narrative_identity(pool).await;

    // Build personalized base prompt (identity → persona → narrative_identity → tools)
    let mut prompt = build_personalized_prompt(&assistant_name, &user_name, persona_id, persona_content.as_deref(), agent_mode, &narrative_identity);

    // Inject onboarding prompt for new users (first conversation)
    if is_new_user {
        prompt.push_str(crate::agent::prompt::NEW_USER_PROMPT);
    }

    // Load AI persistent memory (if any)
    if let Ok(Some(memory)) = sqlx::query_scalar::<_, String>(
        "SELECT memory FROM app_assistant_profile WHERE memory IS NOT NULL LIMIT 1"
    )
    .fetch_optional(pool)
    .await
    {
        if !memory.is_empty() {
            prompt.push_str(&format!(
                "\n\n<memory>\nYour persistent memory (saved via update_memory tool). Reference when relevant:\n{}\n</memory>",
                memory
            ));
        }
    }

    // Add current date/time for temporal awareness
    let now = Utc::now();

    if let Some(tz_str) = timezone {
        // Try to parse the IANA timezone and convert
        if let Ok(tz) = tz_str.parse::<Tz>() {
            let local = now.with_timezone(&tz);
            let date_str = local.format("%A, %B %d, %Y").to_string();
            let time_str = local.format("%I:%M %p %Z").to_string(); // e.g., "7:20 PM EST"
            prompt.push_str(&format!(
                "\n\n<datetime>\nToday is {}. Current time: {}.\n</datetime>",
                date_str, time_str
            ));
        } else {
            // Fallback to UTC if timezone parsing fails
            let date_str = now.format("%A, %B %d, %Y").to_string();
            let time_str = now.format("%H:%M UTC").to_string();
            prompt.push_str(&format!(
                "\n\n<datetime>\nToday is {}. Current time: {}.\n</datetime>",
                date_str, time_str
            ));
        }
    } else {
        let date_str = now.format("%A, %B %d, %Y").to_string();
        let time_str = now.format("%H:%M UTC").to_string();
        prompt.push_str(&format!(
            "\n\n<datetime>\nToday is {}. Current time: {}.\n</datetime>",
            date_str, time_str
        ));
    }

    // Add user context (identity, recent days, connected sources)
    if let Some(user_context) = build_user_context(pool, &user_name).await {
        prompt.push_str(&user_context);
    }

    // Inline the active Notebook (room) as a salience lens: its name, catch-up
    // memo, and member URLs. This is the room the chat lives in.
    if let Some(notebook_id) = notebook_id {
        if let Some(block) = build_notebook_context(pool, notebook_id).await {
            prompt.push_str(&block);
        }
    }

    if let Some(ctx) = active_page {
        if let Some(page_id) = &ctx.page_id {
            let title = ctx.page_title.as_deref().unwrap_or("Untitled");

            // Include the current content from Yjs if available
            // This is the source of truth - use this for edits, not the database content
            if let Some(content) = &ctx.content {
                // Truncate large content to avoid consuming too much context
                let (content_display, truncation_note) = if content.chars().count() > MAX_PAGE_CONTENT_CHARS {
                    let truncated_content: String = content.chars().take(MAX_PAGE_CONTENT_CHARS).collect();
                    let remaining = content.chars().count() - MAX_PAGE_CONTENT_CHARS;
                    let truncated = format!(
                        "{}...\n\n[Content truncated - {} more characters]",
                        truncated_content,
                        remaining
                    );
                    (truncated, " The content shown is truncated. Call get_page_content for the complete document before making edits.")
                } else {
                    (content.clone(), "")
                };

                prompt.push_str(&format!(
                    "\n\n<active_context>\nThe user has \"{}\" (id: {}) open for editing.\n\n<current_content>\n{}\n</current_content>\n\nUse the edit_page tool to make changes. The 'find' parameter locates text, 'replace' provides the new text. For a full rewrite, set find to empty string. Edits are applied immediately via real-time sync.{}\n</active_context>",
                    title, page_id, content_display, truncation_note
                ));
            } else {
                prompt.push_str(&format!(
                    "\n\n<active_context>\nThe user has \"{}\" (id: {}) open for editing. Use get_page_content to read it first, then edit_page to make changes.\n</active_context>",
                    title, page_id
                ));
            }
        }
    }

    prompt
}

/// Maximum member URLs to inline for a Notebook before truncating.
const MAX_NOTEBOOK_ITEMS_INLINED: usize = 100;

/// Build a context block for the active Notebook (room) the chat lives in.
/// Returns None if the Notebook can't be loaded.
async fn build_notebook_context(pool: &PgPool, notebook_id: &str) -> Option<String> {
    let detail = match crate::api::notebooks::get_notebook(pool, notebook_id).await {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("[chat] failed to load active notebook {}: {}", notebook_id, e);
            return None;
        }
    };

    let mut out = String::new();
    out.push_str(&format!(
        "\n\n<active_notebook name=\"{}\">",
        escape_attr(&detail.notebook.name),
    ));

    if let Some(instr) = detail.notebook.instructions.as_deref() {
        if !instr.is_empty() {
            out.push_str(&format!(
                "\n  <instructions>{}</instructions>",
                escape_attr(instr)
            ));
        }
    }

    if let Some(memo) = detail.notebook.current_status.as_deref() {
        if !memo.is_empty() {
            out.push_str(&format!("\n  <memo>{}</memo>", escape_attr(memo)));
        }
    }

    let total = detail.items.len();
    for item in detail.items.iter().take(MAX_NOTEBOOK_ITEMS_INLINED) {
        out.push_str(&format!("\n  <member url=\"{}\"/>", escape_attr(&item.url)));
    }
    if total > MAX_NOTEBOOK_ITEMS_INLINED {
        out.push_str(&format!(
            "\n  <!-- {} more members not shown -->",
            total - MAX_NOTEBOOK_ITEMS_INLINED
        ));
    }

    out.push_str("\n</active_notebook>");

    let preamble = "\n\n<active_notebook_preamble>\nThis chat lives in the Notebook (room) below — a collection the user returns to (a project, pet, hobby, goal, or topic). Treat its members as high-salience: they are the user's actively curated focus for this room. <instructions>, if present, are standing directions for how you should behave in this notebook — follow them. <memo>, if present, is a catch-up note about the notebook's current state. Members are also boosted in semantic search while this notebook is active.\n</active_notebook_preamble>";

    Some(format!("{}{}", preamble, out))
}

/// Minimal XML attribute escaping for the inlined context block.
fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ============================================================================
// Handler
// ============================================================================

/// POST /api/chat - Stream chat completion
///
/// Requires authentication. Routes through virtues-api for budget enforcement.
pub async fn chat_handler(
    State(pool): State<PgPool>,
    State(yjs_state): State<YjsState>,
    State(cancel_state): State<ChatCancellationState>,
    _user: AuthUser,
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


    // Use client-provided message ID for idempotency, or generate one
    let msg_id = request
        .message_id
        .clone()
        .unwrap_or_else(|| format!("msg_{}", generate_id()));

    // Check onboarding status early — needed for synthetic message injection and tool filtering
    let _onboarding_status = sqlx::query_scalar::<_, String>(
        "SELECT onboarding_status FROM app_user_profile LIMIT 1"
    )
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten()
    .unwrap_or_else(|| "active".to_string());

    // DISABLED for demo — onboarding was repeating the same message
    let is_new_user = false; // onboarding_status == "new";
    let is_onboarding = false; // onboarding_status == "new" || onboarding_status == "onboarding";

    // Ensure chat exists - use ON CONFLICT DO NOTHING to handle race conditions
    let chat_id_str = request.chat_id.clone();
    let title = if is_new_user {
        // Onboarding: first conversation, use a friendly default title
        "Welcome".to_string()
    } else {
        let raw_title = request
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

        if raw_title.chars().count() > 50 {
            let t: String = raw_title.chars().take(47).collect();
            format!("{}...", t)
        } else {
            raw_title
        }
    };

    // Use ON CONFLICT DO NOTHING to handle concurrent requests for same chat
    // Returns rows_affected = 1 if inserted, 0 if already exists
    let insert_result =
        sqlx::query("INSERT INTO app_chats (id, title, message_count) VALUES ($1, $2, 0) ON CONFLICT (id) DO NOTHING")
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

    // Bind the chat to its Notebook on first creation (stores notebook_id + folds
    // the chat into the Notebook's membership).
    if chat_was_created {
        if let Err(e) =
            crate::api::notebooks::set_chat_notebook(&pool, &chat_id_str, request.notebook_id.as_deref()).await
        {
            tracing::warn!("Failed to set chat notebook: {}", e);
        }
    }

    // Save the last user message to the chat
    if let Some(last_user_msg) = request.messages.iter().rev().find(|m| m.role == "user") {
        // Normal flow: save the last user message from the request
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
            timestamp: Timestamp::now(),
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

    // Pass compaction_needed flag to stream - compaction will run inside stream
    // and emit a checkpoint event for real-time UI updates
    let compaction_needed = matches!(compaction_status, Ok(ContextStatus::Critical));

    // Load chat from DB and build context with compaction summary
    let chat_row = match sqlx::query(
        r#"SELECT conversation_summary, summary_up_to_index
           FROM app_chats WHERE id = $1"#,
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
    let summary_up_to_index: i64 = chat_row.get("summary_up_to_index");

    // Load messages from normalized table
    let message_rows = match sqlx::query(
        r#"
        SELECT
            id, role, content, created_at, model, provider, agent_id,
            reasoning, tool_calls, intent, subject, thought_signature, parts
        FROM app_chat_messages
        WHERE chat_id = $1
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
            let created_at: Timestamp = msg.get("created_at");
            let model: Option<String> = msg.get("model");
            let provider: Option<String> = msg.get("provider");
            let agent_id: Option<String> = msg.get("agent_id");
            let reasoning: Option<String> = msg.get("reasoning");
            // Columns are jsonb; read as serde_json::Value, not String
            let tool_calls_raw: Option<serde_json::Value> = msg.get("tool_calls");
            let intent_raw: Option<serde_json::Value> = msg.get("intent");
            let subject: Option<String> = msg.get("subject");
            let thought_signature: Option<String> = msg.get("thought_signature");
            let parts_raw: Option<serde_json::Value> = msg.get("parts");

            // Parse JSON fields
            let tool_calls = tool_calls_raw.and_then(|t| serde_json::from_value(t).ok());
            let intent = intent_raw.and_then(|i| serde_json::from_value(i).ok());
            let parts = parts_raw.and_then(|p| serde_json::from_value(p).ok());

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

    // Resolve the chat's room from the persisted row (single source of truth) so
    // the active-notebook context always matches the binding, even if a stale client
    // sends a different per-message notebookId. The create path above already bound a
    // new chat from request.notebook_id, so the row is current by now.
    let effective_notebook_id: Option<String> =
        sqlx::query_scalar(r#"SELECT notebook_id FROM app_chats WHERE id = $1"#)
            .bind(&chat_id_str)
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten();

    // Build system prompt with active page context, timezone, personalization, and agent mode
    // is_onboarding keeps the onboarding prompt active until set_user_name completes
    let mut system_prompt = build_system_prompt(&pool, request.active_page.as_ref(), request.timezone.as_deref(), &request.agent_mode, &request.persona, is_onboarding, effective_notebook_id.as_deref()).await;
    // Scoped (grounded) chat: retrieval is hard-filtered to the notebook's
    // items (ScopeMode::Exclusive in ToolContext); this line sets the matching
    // answer contract. Only meaningful inside a notebook.
    if request.chat_mode == "scoped" && effective_notebook_id.is_some() {
        system_prompt.push_str(
            "\n\nSCOPED CHAT: this conversation is grounded in the current notebook's \
             materials only. Retrieval is restricted to them. Answer ONLY from what \
             retrieval returns, citing each load-bearing claim with its ref link. If \
             the materials don't cover the question, say so plainly — do not answer \
             from general knowledge.",
        );
    }

    // Flip 'new' → 'onboarding' after the first synthetic message (NOT to 'active').
    // The onboarding prompt stays active. set_user_name flips 'onboarding' → 'active'.
    if is_new_user {
        let _ = sqlx::query("UPDATE app_user_profile SET onboarding_status = 'onboarding' WHERE onboarding_status = 'new'")
            .execute(&pool)
            .await;
    }

    // Build context using compaction summary if available
    let api_messages = build_context_for_llm(
        &messages,
        conversation_summary.as_deref(),
        summary_up_to_index as usize,
        Some(&system_prompt),
    );

    // For brand-new users, skip the LLM and emit a preloaded opening message
    let stream = if is_new_user {
        create_preloaded_onboarding_stream(
            pool,
            request.chat_id.clone(),
            msg_id,
            request.agent_id.clone(),
        )
    } else {
        create_agent_stream(
            pool,
            yjs_state,
            cancel_state,
            request,
            api_messages,
            msg_id,
            compaction_needed,
            is_onboarding,
        )
    };

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

/// Create a preloaded SSE stream for the onboarding opening message.
/// Skips the LLM entirely — emits a hardcoded message, saves it to the DB.
fn create_preloaded_onboarding_stream(
    pool: PgPool,
    chat_id: String,
    msg_id: String,
    agent_id: String,
) -> Pin<Box<dyn Stream<Item = Result<SseEvent, Infallible>> + Send>> {
    use crate::agent::prompt::ONBOARDING_OPENING_MESSAGE;

    Box::pin(async_stream::stream! {
        // TextStart
        let start_event = StreamEvent::TextStart { id: msg_id.clone() };
        yield Ok(SseEvent::default().data(serialize_event(&start_event)));

        // TextDelta — emit the full preloaded message
        let event = StreamEvent::TextDelta {
            id: msg_id.clone(),
            delta: ONBOARDING_OPENING_MESSAGE.to_string(),
        };
        yield Ok(SseEvent::default().data(serialize_event(&event)));

        // TextEnd
        let end_event = StreamEvent::TextEnd { id: msg_id.clone() };
        yield Ok(SseEvent::default().data(serialize_event(&end_event)));

        // [DONE]
        yield Ok(SseEvent::default().data("[DONE]"));

        // Save assistant message to chat
        let assistant_message = ChatMessage {
            id: None,
            role: "assistant".to_string(),
            content: ONBOARDING_OPENING_MESSAGE.to_string(),
            timestamp: Timestamp::now(),
            model: None,
            provider: None,
            agent_id: Some(agent_id),
            tool_calls: None,
            reasoning: None,
            intent: None,
            subject: None,
            thought_signature: None,
            parts: None,
        };

        if let Err(e) = append_message(&pool, chat_id, assistant_message).await {
            tracing::error!("Failed to save preloaded onboarding message: {}", e);
        }
    })
}

/// Create the SSE stream using the AgentLoop for tool execution
fn create_agent_stream(
    pool: PgPool,
    yjs_state: YjsState,
    cancel_state: ChatCancellationState,
    request: ChatRequest,
    api_messages: Vec<serde_json::Value>,
    msg_id: String,
    compaction_needed: bool,
    is_onboarding: bool,
) -> Pin<Box<dyn Stream<Item = Result<SseEvent, Infallible>> + Send>> {
    let model = request.model.clone();
    let chat_id = request.chat_id.clone();
    let agent_id = request.agent_id.clone();

    Box::pin(async_stream::stream! {
        // Register cancellation token for this chat
        let cancel_token = cancel_state.register(&chat_id);

        // Run compaction BEFORE the agent loop if needed, and emit checkpoint event
        if compaction_needed {
            tracing::info!(
                chat_id = %chat_id,
                "Context critical, auto-compacting chat"
            );
            let compaction_options = CompactionOptions {
                model_id: Some(model.clone()),
                ..Default::default()
            };
            match compact_chat(&pool, chat_id.clone(), compaction_options).await {
                Ok(_) => {
                    // Fetch the checkpoint message that was just created
                    if let Some(checkpoint_event) = get_latest_checkpoint(&pool, &chat_id).await {
                        yield Ok(SseEvent::default().data(serialize_event(&checkpoint_event)));
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        chat_id = %chat_id,
                        error = %e,
                        "Auto-compaction failed, continuing with full context"
                    );
                }
            }
        }

        // Determine max_steps based on agent mode
        // - deep_research: 50 (read-only, needs more exploration)
        // - council: 40 (gate + 1-2 dispatch rounds + synthesis; bounded)
        // - chat / default: 20 (conversational, full tool access, multi-turn)
        let max_steps = match request.agent_mode.as_str() {
            "deep_research" => 50,
            "council" => 40, // gate + 1-2 dispatch rounds + synthesis; bounded
            _ => 20,         // "chat" or default
        };

        // Create AgentLoop with YjsState for real-time page editing
        let agent = AgentLoop::new_with_yjs(pool.clone(), yjs_state)
        .with_config(AgentConfig {
            max_steps,
            tool_timeout: std::time::Duration::from_secs(30),
            parallel_tools: true,
        });

        // Side-channel for live Deep Research subagent status. The dispatch_subagents tool sends
        // worker updates on `subagent_tx`; the select! loop below drains `subagent_rx` and streams
        // them to the panel while the tool is still executing.
        let (subagent_tx, mut subagent_rx) =
            tokio::sync::mpsc::channel::<crate::tools::SubagentUpdate>(64);

        // Per-turn budget of Deep Research workers, shared across repeated dispatches so the
        // orchestrator can't fan out without bound over its 50-step loop.
        let worker_budget = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(12));

        // Build tool context from request
        let context = ToolContext {
            page_id: request.active_page.as_ref().and_then(|p| p.page_id.clone()),
            user_id: None,
            notebook_id: request.notebook_id.clone(),
            scope_mode: if request.chat_mode == "scoped" {
                crate::search::ScopeMode::Exclusive
            } else {
                crate::search::ScopeMode::Weighted
            },
            chat_id: Some(request.chat_id.clone()),
            applet_id: None,
            subagent_tx: Some(subagent_tx),
            cancel_token: Some(cancel_token.clone()),
            worker_budget: Some(worker_budget),
        };

        // Get tool definitions — onboarding restricts to naming/memory tools only.
        let tools = if is_onboarding {
            crate::tools::get_tools_for_onboarding()
        } else {
            crate::tools::get_tools_for_agent_mode(&request.agent_mode)
        };

        // Send text-start event
        let start_event = StreamEvent::TextStart { id: msg_id.clone() };
        yield Ok(SseEvent::default().data(serialize_event(&start_event)));

        // Track accumulated content
        let mut full_content = String::new();
        let mut reasoning_content = String::new();
        let mut in_reasoning = false;
        // The whole turn streams as ONE text part (single TextStart/TextEnd), so
        // text emitted across agent steps would otherwise concatenate with no
        // separator ("…exact text.The earlier edit…"). When text resumes after a
        // tool call, insert a paragraph break so each narration reads on its own.
        let mut needs_text_break = false;

        // Token usage tracking
        let mut total_input_tokens: u32 = 0;
        let mut total_output_tokens: u32 = 0;
        let mut total_reasoning_tokens: u32 = 0;
        // Authoritative spend for this turn (sum of gateway-reported usage.cost
        // across every step), captured into app_ai_calls for the Usage tab.
        let mut total_cost_micros: i64 = 0;

        // Tool call tracking for persistence
        let mut all_tool_calls: Vec<ToolCall> = Vec::new();

        // Run the agent loop with cancellation support
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
            }),
            Some(cancel_token.clone()),
        );

        loop {
          tokio::select! {
            biased;
            // Live subagent status — drained even while dispatch_subagents is still executing.
            Some(update) = subagent_rx.recv() => {
                let ev = StreamEvent::SubagentStatus {
                    dispatch_id: update.dispatch_id,
                    subagent_id: update.id as u32,
                    title: update.title,
                    model: update.model,
                    status: update.status.as_str().to_string(),
                    tokens: update.tokens,
                };
                yield Ok(SseEvent::default().data(serialize_event(&ev)));
            }
            maybe_event = agent_stream.next() => {
              let event = match maybe_event {
                  Some(e) => e,
                  None => break,
              };
              match event {
                AgentEvent::TextDelta { content } => {
                    // End reasoning if we were in it
                    if in_reasoning {
                        in_reasoning = false;
                        let event = StreamEvent::ReasoningEnd { id: msg_id.clone() };
                        yield Ok(SseEvent::default().data(serialize_event(&event)));
                    }
                    // Text resuming after a tool call: break the paragraph so it
                    // doesn't butt against the previous segment's final sentence.
                    let delta = if needs_text_break
                        && !full_content.is_empty()
                        && !full_content.ends_with('\n')
                    {
                        format!("\n\n{}", content)
                    } else {
                        content
                    };
                    needs_text_break = false;
                    full_content.push_str(&delta);
                    let event = StreamEvent::TextDelta {
                        id: msg_id.clone(),
                        delta,
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
                    // Any text that resumes after this tool call starts a new
                    // paragraph (see needs_text_break).
                    needs_text_break = true;
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
                    // Bill nested Deep Research worker tokens to this chat's usage. The dispatch
                    // result carries aggregate worker token counts that the orchestrator's own
                    // Usage events don't include.
                    if let Some(usage) = result.get("usage") {
                        total_input_tokens += usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        total_output_tokens += usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    }
                    // AI SDK v6: tool-output-available event
                    let event = StreamEvent::ToolOutputAvailable {
                        tool_call_id: id,
                        output: result,
                    };
                    yield Ok(SseEvent::default().data(serialize_event(&event)));
                }

                AgentEvent::Usage { prompt_tokens, completion_tokens, total_tokens: _, reasoning_tokens, cost_micros } => {
                    total_input_tokens += prompt_tokens;
                    total_output_tokens += completion_tokens;
                    if let Some(r) = reasoning_tokens {
                        total_reasoning_tokens += r;
                    }
                    if let Some(c) = cost_micros {
                        total_cost_micros += c;
                    }
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
          }
        }

        // Drain any subagent updates buffered after the agent loop ended.
        while let Ok(update) = subagent_rx.try_recv() {
            let ev = StreamEvent::SubagentStatus {
                dispatch_id: update.dispatch_id,
                subagent_id: update.id as u32,
                title: update.title,
                model: update.model,
                status: update.status.as_str().to_string(),
                tokens: update.tokens,
            };
            yield Ok(SseEvent::default().data(serialize_event(&ev)));
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
            // Mark the message as user-stopped so the UI can show a "Stopped"
            // notice on reload (the partial content is kept either way).
            let was_cancelled = cancel_token.is_cancelled();
            let assistant_message = ChatMessage {
                id: None,
                role: "assistant".to_string(),
                content: full_content.clone(),
                timestamp: Timestamp::now(),
                model: Some(model.clone()),
                provider: Some(provider),
                agent_id: Some(agent_id),
                tool_calls: if all_tool_calls.is_empty() { None } else { Some(all_tool_calls.clone()) },
                reasoning: if reasoning_content.is_empty() { None } else { Some(reasoning_content.clone()) },
                intent: None,
                subject: if was_cancelled { Some("cancelled".to_string()) } else { None },
                thought_signature: None,
                parts: None,
            };

            if let Err(e) = append_message(&pool, chat_id.clone(), assistant_message).await {
                tracing::error!("Failed to save assistant message: {}", e);
            }

            // Record token usage. `cost_micros` is the gateway's authoritative
            // figure — the same one recorded in app_ai_calls below, and the one
            // the wallet was actually debited for. No estimating.
            let usage_data = UsageData {
                input_tokens: total_input_tokens as i64,
                output_tokens: total_output_tokens as i64,
                reasoning_tokens: 0,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                cost_micros: Some(total_cost_micros),
            };

            if let Err(e) = record_chat_usage(&pool, chat_id.clone(), &model, usage_data).await {
                tracing::warn!(
                    chat_id = %chat_id,
                    error = %e,
                    "Failed to record chat usage"
                );
            }

            // Box-local per-call cost log (authoritative gateway cost) for the
            // Usage/Telemetry tabs. Best-effort — never break the response.
            if let Err(e) = crate::api::ai_calls::record_ai_call(
                &pool,
                &crate::api::ai_calls::AiCall {
                    // Real feature bucket: chat | council | deep_research (these
                    // modes share this handler), so spend attributes correctly.
                    feature: request.agent_mode.clone(),
                    model: model.clone(),
                    prompt_tokens: total_input_tokens as i64,
                    completion_tokens: total_output_tokens as i64,
                    reasoning_tokens: total_reasoning_tokens as i64,
                    cost_micros: total_cost_micros,
                    chat_id: Some(chat_id.clone()),
                    applet_run_id: None,
                },
            )
            .await
            {
                tracing::warn!(chat_id = %chat_id, error = %e, "Failed to record ai_call");
            }
        }

        // Clean up cancellation token when stream ends
        cancel_state.remove(&chat_id);
    })
}

/// Generate a random ID for messages
fn generate_id() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    let bytes: [u8; 8] = rng.random();
    hex::encode(bytes)
}

// ============================================================================
// Cancel Handler
// ============================================================================

/// Request body for cancelling a chat
#[derive(Debug, Deserialize)]
pub struct CancelChatRequest {
    #[serde(rename = "chatId")]
    pub chat_id: String,
}

/// Response for cancel request
#[derive(Debug, Serialize)]
pub struct CancelChatResponse {
    pub cancelled: bool,
    pub message: String,
}

/// POST /api/chat/cancel - Cancel an in-progress chat request
///
/// Stops the agent loop for the specified chat, preserving any partial results.
pub async fn cancel_chat_handler(
    State(cancel_state): State<ChatCancellationState>,
    _user: AuthUser,
    Json(request): Json<CancelChatRequest>,
) -> impl IntoResponse {
    let cancelled = cancel_state.cancel(&request.chat_id);

    let response = CancelChatResponse {
        cancelled,
        message: if cancelled {
            "Chat request cancelled".to_string()
        } else {
            "No active request found for this chat".to_string()
        },
    };

    (StatusCode::OK, Json(response))
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

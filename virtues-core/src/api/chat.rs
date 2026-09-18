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
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response, Sse},
    Json,
};
use chrono::Utc;
use futures::stream::Stream;
use futures::FutureExt;
use serde::{Deserialize, Serialize};
use tracing::Instrument;
use sqlx::PgPool;
use std::convert::Infallible;
use std::pin::Pin;

use crate::api::live_turn::{self, LiveTurns};
use std::sync::Arc;
use tokio_stream::StreamExt;

use crate::agent::{AgentConfig, AgentEvent, AgentLoop, FinishReason, StepReason};
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
    /// The person's per-turn pick from the picker, if they made one.
    ///
    /// Absent — the ordinary case — means "whatever this turn's slot resolves
    /// to", decided by the box in `model_choice::resolve_turn_model`. A client
    /// is never required to know a model id, and an unpinned chat is not
    /// frozen to the model it opened with. Empty string counts as absent; see
    /// that module for why shipped clients depend on it.
    #[serde(default)]
    pub model: Option<String>,
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
    /// Why the client sent this request: `submit-message` (the default, and
    /// what a client older than the field means) or `regenerate-message`
    /// (SDK 7's spelling; the docs' `regenerate-assistant-message` is also
    /// read). On regenerate the client has removed
    /// its last assistant message and sends no new user turn, so the box
    /// removes its own copy of that message and answers the last user turn
    /// again. Before this the box kept the old answer in history, and the
    /// model "regenerated" with its previous reply in front of it.
    #[serde(default)]
    pub trigger: Option<String>,
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
    /// A temporary ("ghost") chat: nothing about it is written to the box.
    /// No chat row, no messages, no usage row. Its history arrives on the
    /// request every turn, because the box holds none.
    ///
    /// The client has sent this flag since ghost mode shipped and promised
    /// "never persisted" in its UI, while the box, which had no field to
    /// read, created the row and stored every message anyway. The only
    /// thing that still records a ghost turn is `app_ai_calls`: cost and
    /// token counts, no content, no chat id.
    #[serde(default)]
    pub temporary: bool,
}

/// A ghost chat's history, as the client sent it. The box stores nothing for
/// a temporary chat, so the wire is the only transcript. Text parts become
/// content; tool parts ride along in `parts` for the converter downstream.
fn ghost_history(messages: &[UIMessage]) -> Vec<ChatMessage> {
    messages
        .iter()
        .filter(|m| m.role == "user" || m.role == "assistant")
        .map(|m| {
            let content = m.content.clone().unwrap_or_else(|| {
                m.parts
                    .as_ref()
                    .map(|parts| {
                        parts
                            .iter()
                            .filter_map(|p| match p {
                                UIPart::Text { text } => Some(text.clone()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default()
            });
            ChatMessage {
                id: m.id.clone(),
                role: m.role.clone(),
                content,
                timestamp: Timestamp::now(),
                model: None,
                provider: None,
                agent_id: None,
                parts: m.parts.clone(),
                tool_calls: None,
                reasoning: None,
                intent: None,
                subject: None,
                reasoning_details: None,
            }
        })
        .filter(|m| !m.content.is_empty() || m.parts.is_some())
        .collect()
}

/// Materialize a turn's ordered `parts` from the pieces the stream collected.
///
/// Text runs keep their order relative to the tool calls that ran between them,
/// which is the whole point: the LAST text run is the reply, and everything
/// before it is the model narrating its way there.
///
/// A tool id in `turn_slots` with no matching entry in `tool_calls` is skipped
/// rather than written as an empty invocation — that only happens if the stream
/// ended between a tool starting and being recorded, and a part with no name or
/// input tells a reader nothing except that something is missing.
fn build_turn_parts(
    turn_slots: &[TurnSlot],
    text_segments: &[String],
    tool_calls: &[crate::api::chats::ToolCall],
    failed_tools: &std::collections::HashSet<String>,
    reasoning: &str,
) -> Vec<UIPart> {
    let mut parts = Vec::with_capacity(turn_slots.len() + 1);
    // The thinking comes before the turn it produced. It has its own column
    // too, but the client stopped reading that the moment `parts` existed —
    // it returns `parts` verbatim when there are any — so a turn that thought
    // reloaded with an empty thinking block.
    if !reasoning.trim().is_empty() {
        parts.push(UIPart::Reasoning { text: reasoning.to_string() });
    }
    for slot in turn_slots {
        match slot {
            TurnSlot::Text(i) => {
                let Some(text) = text_segments.get(*i) else { continue };
                // A run that produced nothing is not a paragraph of silence.
                if text.trim().is_empty() {
                    continue;
                }
                parts.push(UIPart::Text { text: text.clone() });
            }
            TurnSlot::Tool(id) => {
                let Some(tc) = tool_calls
                    .iter()
                    .find(|tc| tc.tool_call_id.as_deref() == Some(id.as_str()))
                else {
                    continue;
                };
                // The turn is over by the time this runs, so anything that
                // returned has its output and anything that did not, did not.
                // A tool that FAILED is its own state: picking the state off
                // `result.is_some()` alone stored a failure as
                // `output-available` with the reason buried inside `output`,
                // so the red block never rendered on reload and the replay
                // handed the model an error object as though it were an answer.
                let failed = failed_tools.contains(id);
                let error_text = failed.then(|| {
                    tc.result
                        .as_ref()
                        .and_then(|r| r.get("error"))
                        .and_then(|e| e.as_str())
                        .unwrap_or("the tool reported a failure")
                        .to_string()
                });
                let state = match (failed, tc.result.is_some()) {
                    (true, _) => "output-error",
                    (false, true) => "output-available",
                    (false, false) => "input-available",
                };
                parts.push(UIPart::ToolInvocation {
                    tool_call_id: id.clone(),
                    tool_name: tc.tool_name.clone(),
                    input: tc.arguments.clone(),
                    state: state.to_string(),
                    output: tc.result.clone(),
                    error_text,
                });
            }
        }
    }
    parts
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

/// Where one piece of a turn sat in time.
///
/// The row already stores the turn's text (`content`) and its tool calls
/// (`tool_calls`), but nothing recorded the ORDER they interleaved in — so a
/// reload could show what was said and what was called, never which was said
/// before which call. `parts` is built from this at save time.
enum TurnSlot {
    /// An index into the turn's text segments.
    Text(usize),
    /// A tool call id, resolved against `all_tool_calls` for its input/output.
    Tool(String),
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
    /// One tool call and, once it answers, its result.
    ///
    /// On disk and on the wire this is `tool-<toolName>`, NOT the variant's own
    /// name — the client matches an exact type per tool (`tool-create_page`,
    /// `tool-generate_image`, …) and has no branch for anything else, so a part
    /// stored as `tool-invocation` renders as nothing at all. `parts_to_jsonb`
    /// and `parts_from_jsonb` are the translation, and they are the ONLY way
    /// this column should be written or read. A single hardcoded
    /// `tool-web_search` variant used to stand in for the whole family; every
    /// other tool fell through to `Unknown` and was dropped.
    #[serde(rename = "tool-invocation")]
    ToolInvocation {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        /// Tool name - defaults to empty string if not provided (AI SDK may omit it)
        #[serde(rename = "toolName", default)]
        tool_name: String,
        #[serde(default)]
        input: serde_json::Value,
        /// `output-available` or `output-error`, the two the client renders.
        #[serde(default)]
        state: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        output: Option<serde_json::Value>,
        /// Set with `state: "output-error"`. The client keys its error block on
        /// the state and reads this; without it a failed tool reloaded as a
        /// successful one with the failure buried inside `output`.
        #[serde(rename = "errorText", default, skip_serializing_if = "Option::is_none")]
        error_text: Option<String>,
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

/// The `parts` jsonb column, read.
///
/// Normalizes `tool-<toolName>` — which is what the client writes and what we
/// now store — into the tagged variant, so one shape is understood everywhere.
/// A row whose JSON no longer matches says so rather than silently becoming
/// "this turn had no tools": that reads as a fact, not as a bug.
pub fn parts_from_jsonb(raw: serde_json::Value, msg_id: &str) -> Option<Vec<UIPart>> {
    let mut raw = raw;
    if let Some(items) = raw.as_array_mut() {
        for item in items.iter_mut() {
            let Some(obj) = item.as_object_mut() else { continue };
            let Some(kind) = obj.get("type").and_then(|t| t.as_str()).map(str::to_string) else {
                continue;
            };
            let Some(name) = kind.strip_prefix("tool-") else { continue };
            if name.is_empty() || name == "invocation" {
                continue;
            }
            obj.entry("toolName")
                .or_insert_with(|| serde_json::Value::String(name.to_string()));
            obj.insert("type".to_string(), serde_json::Value::String("tool-invocation".to_string()));
        }
    }
    serde_json::from_value(raw)
        .map_err(|e| tracing::warn!(msg_id, error = %e, "parts did not parse"))
        .ok()
}

/// The `parts` jsonb column, written. The inverse of `parts_from_jsonb`.
pub fn parts_to_jsonb(parts: &[UIPart]) -> serde_json::Value {
    let mut value = serde_json::to_value(parts).unwrap_or(serde_json::Value::Null);
    if let Some(items) = value.as_array_mut() {
        for item in items.iter_mut() {
            let Some(obj) = item.as_object_mut() else { continue };
            if obj.get("type").and_then(|t| t.as_str()) != Some("tool-invocation") {
                continue;
            }
            let name = obj.get("toolName").and_then(|n| n.as_str()).unwrap_or("");
            if name.is_empty() {
                continue;
            }
            let kind = format!("tool-{name}");
            obj.insert("type".to_string(), serde_json::Value::String(kind));
        }
    }
    value
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

    // Tool failure (AI SDK: tool-output-error). The UI's `output-error`
    // branches waited on this for months while failures rode inside
    // tool-output-available.
    #[serde(rename = "tool-output-error")]
    ToolOutputError {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "errorText")]
        error_text: String,
    },

    // Error handling
    Error {
        #[serde(rename = "errorText")]
        error_text: String,
    },

    // Message and step framing. `start` opens the message; each agent-loop
    // step is bracketed by start-step / finish-step (the SDK needs the
    // boundary to keep a tool call and the text after it apart); `finish`
    // says how the turn ended, and is the only place the client learns a
    // reply was cut short. `abort` is the person's own stop.
    Start {
        #[serde(rename = "messageId")]
        message_id: String,
    },
    StartStep,
    FinishStep,
    Finish {
        #[serde(rename = "finishReason")]
        finish_reason: String,
    },
    Abort {
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },

    // The interview's write_it_up finished: tell the client which page to open
    // beside the chat. A data part rather than the tool part, because tool
    // parts are pushed into a message's parts array MUTABLY mid-turn and
    // effects watching the messages array provably never see them arrive;
    // onData fires deterministically.
    #[serde(rename = "narrative-document-ready")]
    NarrativeDocumentReady {
        #[serde(rename = "pageId")]
        page_id: String,
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

/// Narrative-document-ready payload for AI SDK v6 data event
#[derive(Debug, Serialize)]
struct NarrativeDocumentData {
    #[serde(rename = "pageId")]
    page_id: String,
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
    .map_err(|e| tracing::error!(chat_id, error = %e, "checkpoint lookup failed"))
    .ok()??;

    // `parts` is jsonb and `created_at` is timestamptz. Reading either as a
    // String is not a wrong value, it is a PANIC inside the turn's task —
    // `Row::get` unwraps the decode — so auto-compaction killed the reply it
    // had just made room for, every time it succeeded.
    let id: String = row.get("id");
    let parts_json: Option<serde_json::Value> = row.get("parts");
    let created_at: Timestamp = row.get("created_at");
    let created_at = created_at.to_rfc3339();

    // Parse parts JSON to extract checkpoint data
    let parts: Vec<UIPart> = parts_json
        .and_then(|json| parts_from_jsonb(json, &id))
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
        // Wrap the narrative-document signal in AI SDK v6 data event format
        // (transient — a reload reconstructs the document from its page, and
        // replaying the auto-open on old chats would be wrong).
        StreamEvent::NarrativeDocumentReady { page_id } => {
            let wrapper = DataEvent {
                event_type: "data-narrative-document".to_string(),
                id: None,
                data: NarrativeDocumentData { page_id: page_id.clone() },
                transient: true,
            };
            serde_json::to_string(&wrapper).unwrap_or_else(|e| {
                tracing::error!("Failed to serialize narrative-document event: {}", e);
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
/// The user's telos document — values, character, aspirations — for the system
/// prompt. Empty string when unset, which is the ordinary case.
///
/// **Budget: ~2k tokens, and the reason is behavioural, not economic.** NI sits
/// in the SYSTEM prompt, so it is prompt-cached and the marginal cost per turn
/// is a cache read; 5k would be affordable. What a longer document costs is
/// precision. The prompt around this already spends four paragraphs telling the
/// model to hold it lightly — "the fastest way to lose trust is to psychoanalyse
/// a shopping list" — and every extra paragraph is more surface for a spurious
/// connection to a routine question. A longer NI does not make the assistant
/// understand you better; it makes it perform understanding more often.
///
/// Truncation happens at a PARAGRAPH boundary. The previous version cut at 800
/// characters mid-word, which would have fed the model a sentence that stops
/// in the middle and invited it to complete the thought itself. (It never fired
/// in practice: the table has no rows on a real box, so NI has been the empty
/// string in every prompt since it shipped. Fixing it is a build, not a repair.)
async fn build_narrative_identity(pool: &PgPool) -> String {
    // The DOCUMENT itself — "In your own words", the page the person edits.
    // There is deliberately no abridged copy between them and the assistant:
    // the old capsule (wiki_narrative_identity) drifted from the document and
    // was caught inventing standing directives. One artifact, read whole,
    // truncated at a paragraph boundary if the person writes past the budget.
    match crate::api::wiki_articles::get_article_prose(
        pool,
        "narrative_identity",
        crate::api::narrative_draft::NAR_IDENTITY_ID,
    )
    .await
    {
        Ok(Some(prose)) if !prose.content.trim().is_empty() => {
            truncate_to_budget(&prose.content, NI_BUDGET_CHARS)
        }
        _ => String::new(),
    }
}

/// ~2k tokens. English averages near four characters per token, and this is a
/// ceiling rather than a target.
const NI_BUDGET_CHARS: usize = 8_000;

/// Cut at the last paragraph break inside the budget; if there is no break to
/// find, cut at the last sentence end; only then fall back to a hard cut.
///
/// Never mid-word: a document that ends mid-sentence reads to the model as a
/// thought it should finish, and this one is about who a person is.
fn truncate_to_budget(text: &str, budget: usize) -> String {
    if text.chars().count() <= budget {
        return text.to_string();
    }
    let cut: String = text.chars().take(budget).collect();
    if let Some(i) = cut.rfind("\n\n") {
        return cut[..i].trim_end().to_string();
    }
    if let Some(i) = cut.rfind(['.', '!', '?']) {
        return cut[..=i].to_string();
    }
    match cut.rfind(char::is_whitespace) {
        Some(i) => cut[..i].to_string(),
        None => cut,
    }
}

/// The rules block — `wiki_rules`, grouped by kind.
///
/// THIS TABLE WAS WRITTEN AND NEVER READ. From 0101 until now, `wiki_rules` (and
/// `wiki_standing_order` before it) was populated by the interview's last
/// question and consumed by nothing: the box obeyed no rule anyone had written,
/// while the interview told them in as many words that "what you write here
/// stops being context and becomes a rule." This function is what makes that
/// sentence true.
///
/// Grouped rather than listed flat because `avoid` and `defend` need opposite
/// handling, and the prompt can only give them opposite handling if it can tell
/// them apart.
///
/// Empty string when there are no rules, so the caller can omit the section.
/// Errors are swallowed to an empty string on purpose — a database blip must
/// degrade to "no rules injected" rather than taking down chat. That is the
/// safe direction only because the alternative is worse; it does mean a failed
/// read silently un-enforces, which is why it is logged.
async fn build_rules(pool: &PgPool) -> String {
    let rows = match sqlx::query_as::<_, (String, String)>(
        "SELECT kind, rule FROM wiki_rules WHERE active ORDER BY created_at",
    )
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "rules: read failed — none will be enforced this turn");
            return String::new();
        }
    };

    let mut avoid: Vec<&str> = Vec::new();
    let mut defend: Vec<&str> = Vec::new();
    for (kind, rule) in &rows {
        let rule = rule.trim();
        if rule.is_empty() {
            continue;
        }
        match kind.as_str() {
            "defend" => defend.push(rule),
            // Anything unrecognised is treated as `avoid`. The CHECK constraint
            // makes that unreachable today, and if a third kind is ever added,
            // the conservative reading is the one that cannot cause harm.
            _ => avoid.push(rule),
        }
    }

    let mut out = String::new();
    if !avoid.is_empty() {
        out.push_str("Never raise these unless they do:\n");
        for r in avoid {
            out.push_str(&format!("- {r}\n"));
        }
    }
    if !defend.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str("Help them hold to these:\n");
        for r in defend {
            out.push_str(&format!("- {r}\n"));
        }
    }
    out
}

/// Build system prompt with dynamic context and personalization.
///
/// Assembles: identity → persona → narrative_identity → tools → datetime → user_context → active_page.
/// Loads user name, assistant name, persona, and narrative identity from profiles.
async fn build_system_prompt(
    pool: &PgPool,
    active_page: Option<&ActivePageContext>,
    timezone: Option<&str>,
    agent_mode: &str,
    persona_id: &str,
    notebook_id: Option<&str>,
) -> String {
    use crate::api::assistant_profile::get_assistant_name;
    use crate::api::profile::get_display_name;

    // Load personalization from profiles (with fallbacks)
    let assistant_name = get_assistant_name(pool).await.unwrap_or_else(|_| "Ari".to_string());
    let user_name = get_display_name(pool).await.unwrap_or_else(|_| "there".to_string());

    // The narrative interview is a different room entirely: no tools, no
    // persona, no data context, no narrative-identity injection (the document
    // this conversation exists to create). Its prompt stands alone.
    if agent_mode == "interview" {
        // The person's reply count is the one fact about progress the box can
        // vouch for; the prompt reads it as a floor on what can be covered.
        let their_replies = crate::api::narrative_draft::their_reply_count(pool)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(error = %e, "interview reply count unavailable; prompt says 0");
                0
            });
        return crate::agent::prompt::build_interview_prompt(&assistant_name, &user_name, their_replies);
    }

    // Getting started: its own prompt plus the derived state, regenerated per
    // turn so the model never holds a step done that the rows say is open.
    if agent_mode == crate::api::getting_started::AGENT_MODE {
        let block = match crate::api::getting_started::compute(pool).await {
            Ok(s) => s.render_prompt_block(),
            Err(e) => {
                tracing::warn!(error = %e, "getting-started state unavailable for the prompt");
                "<getting_started>\n(state unavailable this turn; say so if asked, never guess)\n</getting_started>".to_string()
            }
        };
        return crate::agent::prompt::build_getting_started_prompt(&assistant_name, &user_name, &block);
    }

    build_system_prompt_blocks(
        pool,
        active_page,
        timezone,
        agent_mode,
        persona_id,
        notebook_id,
        &assistant_name,
        &user_name,
    )
    .await
    .0
}

/// The registry: every prompt section as a named block, rendered in list
/// order by `prompt_blocks::assemble`. This is the current order verbatim —
/// the formula's reorder (rules last, quantized clock, cache breakpoint) is
/// a deliberate later slice, and it happens by editing THIS list.
#[allow(clippy::too_many_arguments)]
async fn build_system_prompt_blocks(
    pool: &PgPool,
    active_page: Option<&ActivePageContext>,
    timezone: Option<&str>,
    agent_mode: &str,
    persona_id: &str,
    notebook_id: Option<&str>,
    assistant_name: &str,
    user_name: &str,
) -> (String, Vec<crate::agent::prompt_blocks::RenderedBlock>) {
    use crate::agent::prompt::build_personalized_prompt;
    use crate::agent::prompt_blocks::{assemble, Author, Block, BlockMeta, Cadence, Mood};
    use crate::api::personas::get_persona_content;

    let blocks: Vec<Block<'_>> = vec![
        // The fused head: base identity + persona + narrative identity + tool
        // guidance + mode, still one string from build_personalized_prompt.
        // Splits into <character>/<narrative_identity>/<tools> in the reorder
        // slice.
        Block {
            meta: BlockMeta { tag: "base", author: Author::System, mood: Mood::Declarative, rung: 40, cadence: Cadence::Slow },
            body: Box::pin(async move {
                let persona_content = get_persona_content(pool, persona_id).await.ok().flatten();
                let narrative_identity = build_narrative_identity(pool).await;
                Some(build_personalized_prompt(
                    assistant_name,
                    user_name,
                    persona_id,
                    persona_content.as_deref(),
                    agent_mode,
                    &narrative_identity,
                ))
            }),
        },
        // The precedence ladder, stated once. GPT-5-era guidance and our own
        // formula doc agree: unresolved hierarchy ambiguity measurably
        // degrades compliance, so the ranking is text the model can cite,
        // not an emergent property of ordering.
        Block {
            meta: BlockMeta { tag: "precedence", author: Author::System, mood: Mood::Declarative, rung: 40, cadence: Cadence::Static },
            body: Box::pin(async move {
                Some(crate::agent::prompt_blocks::precedence_line().to_string())
            }),
        },
        // The machine's memory: per-note rows the person can read and edit
        // (Settings), rendered with ids so the update_memory ops can name
        // them. Three lanes per docs/narrative-identity.md: facts of their
        // world, their preferred manner, their practices.
        Block {
            meta: BlockMeta { tag: "memory", author: Author::Machine, mood: Mood::Declarative, rung: 50, cadence: Cadence::Session },
            body: Box::pin(async move {
                let memories = match crate::api::assistant_memories::list_memories(pool).await {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::warn!("[chat] memory omitted from the prompt: {e}");
                        return None;
                    }
                };
                if memories.is_empty() {
                    return None;
                }
                let mut out = String::from(
                    "\n\n<memory>\nWhat you have learned alongside {user}, in notes you keep (they can read and edit these in Settings; a note marked [theirs] is in their words — never revise or retire it). Use silently: reference when relevant, never recite unprompted.\n",
                );
                out = out.replace("{user}", user_name);
                for lane in ["facts", "manner", "practices"] {
                    let in_lane: Vec<_> = memories.iter().filter(|m| m.lane == lane).collect();
                    if in_lane.is_empty() {
                        continue;
                    }
                    out.push_str(&format!("<{lane}>\n"));
                    for m in in_lane {
                        let theirs = if m.author == "human" { " [theirs]" } else { "" };
                        out.push_str(&format!("- (#{}){} {}\n", m.id, theirs, m.body));
                    }
                    out.push_str(&format!("</{lane}>\n"));
                }
                out.push_str("</memory>");
                Some(out)
            }),
        },
        // The computed present — clock, place, today's spine, calendar,
        // recent people (with entity ids), live threads, last night's sleep,
        // narrated recent days, connected sources. Deterministic, SQL-only,
        // budgeted by fixed caps; the quarter-hour clock is computed ONCE and
        // every line derives from the same instant. Replaces the old
        // <datetime> + <user_context> pair (formula slice 4).
        Block {
            meta: BlockMeta { tag: "circumstances", author: Author::Computed, mood: Mood::Declarative, rung: 30, cadence: Cadence::Quantized },
            body: Box::pin(async move {
                let now = Utc::now();
                let floored = now
                    - chrono::Duration::minutes(i64::from(
                        now.format("%M").to_string().parse::<u32>().unwrap_or(0) % 15,
                    ));
                crate::api::circumstances::build_circumstances(pool, timezone, floored).await
            }),
        },
        // The active Notebook (room) as a salience lens: its name, catch-up
        // memo, and member URLs. This is the room the chat lives in.
        Block {
            meta: BlockMeta { tag: "active_notebook", author: Author::Ui, mood: Mood::Declarative, rung: 45, cadence: Cadence::Session },
            body: Box::pin(async move {
                match notebook_id {
                    Some(id) => build_notebook_context(pool, id).await,
                    None => None,
                }
            }),
        },
        // The open page's live content (Yjs is the source of truth).
        Block {
            meta: BlockMeta { tag: "active_context", author: Author::Ui, mood: Mood::Declarative, rung: 45, cadence: Cadence::PerTurn },
            body: Box::pin(async move { build_active_page_block(active_page) }),
        },
        // The enforceable half — LAST, nearest the conversation (moved
        // 2026-08-28; RULES_PROMPT's own placement note predicted exactly
        // this lever). Constraint adherence tracks recency, and rules are
        // the one block that must hold at 1-in-1000. Sitting after the
        // volatile tail also means they are never cached and never bust
        // anything — a few hundred deliberately re-sent tokens. Absent
        // entirely when there are no rules: an empty <rules> block would
        // teach the model that the section is usually noise.
        Block {
            meta: BlockMeta { tag: "rules", author: Author::User, mood: Mood::Imperative, rung: 100, cadence: Cadence::Slow },
            body: Box::pin(async move {
                let rules = build_rules(pool).await;
                (!rules.is_empty()).then(|| {
                    crate::agent::prompt::RULES_PROMPT
                        .replace("{user_name}", user_name)
                        .replace("{rules}", &rules)
                })
            }),
        },
    ];

    assemble(blocks).await
}

/// Render the open-page section, if a page is open.
fn build_active_page_block(active_page: Option<&ActivePageContext>) -> Option<String> {
    let ctx = active_page?;
    let page_id = ctx.page_id.as_ref()?;
    let title = ctx.page_title.as_deref().unwrap_or("Untitled");

    Some(match &ctx.content {
        Some(content) => {
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

            format!(
                "\n\n<active_context>\nThe user has \"{}\" (id: {}) open for editing.\n\n<current_content>\n{}\n</current_content>\n\nUse the edit_page tool to make changes. The 'find' parameter locates text, 'replace' provides the new text. For a full rewrite, set find to empty string. Edits are applied immediately via real-time sync.{}\n</active_context>",
                title, page_id, content_display, truncation_note
            )
        }
        None => format!(
            "\n\n<active_context>\nThe user has \"{}\" (id: {}) open for editing. Use get_page_content to read it first, then edit_page to make changes.\n</active_context>",
            title, page_id
        ),
    })
}

/// Test-only view of the assembled prompt, so audits in other modules can
/// assert on what the model is actually sent rather than re-deriving it.
#[cfg(test)]
pub(crate) async fn build_system_prompt_for_audit(pool: &PgPool) -> String {
    build_system_prompt(pool, None, Some("America/Chicago"), "default", "default", None).await
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
    let shown: Vec<_> = detail
        .items
        .iter()
        .take(MAX_NOTEBOOK_ITEMS_INLINED)
        .collect();

    // Resolve every member in one batch before writing the block. A bare URL
    // is unreadable: the model cannot tell a screenshot from a spreadsheet
    // without spending tool calls to find out, and the room it is standing in
    // should not require an investigation.
    let urls: Vec<String> = shown.iter().map(|i| i.url.clone()).collect();
    let resolved = crate::api::refs::resolve_refs(pool, &urls).await;

    for item in shown {
        out.push_str(&format!("\n  <member url=\"{}\"", escape_attr(&item.url)));
        if let Some(r) = resolved.get(&item.url) {
            out.push_str(&format!(" title=\"{}\"", escape_attr(&r.title)));
            out.push_str(&format!(" kind=\"{}\"", escape_attr(&r.kind)));
            if let Some(mime) = r.mime.as_deref() {
                out.push_str(&format!(" mime=\"{}\"", escape_attr(mime)));
            }
            // Only meaningful for files: says whether semantic_search can see
            // this member's contents, so a dead end is known up front rather
            // than discovered three tool calls in.
            if let Some(text) = r.text.as_deref() {
                out.push_str(&format!(" text=\"{}\"", escape_attr(text)));
            }
        }
        // `library` grounds chat; `manuscript` is the user's own draft, kept
        // out of retrieval so it is never cited back at them; `pin` is
        // navigation. Without this the three were indistinguishable here, and
        // a draft read exactly like a source.
        out.push_str(&format!(" role=\"{}\"/>", escape_attr(&item.role)));
    }
    if total > MAX_NOTEBOOK_ITEMS_INLINED {
        out.push_str(&format!(
            "\n  <!-- {} more members not shown -->",
            total - MAX_NOTEBOOK_ITEMS_INLINED
        ));
    }

    out.push_str("\n</active_notebook>");

    let preamble = "\n\n<active_notebook_preamble>\nThis chat lives in the Notebook (room) below — a collection the user returns to (a project, pet, hobby, goal, or topic). Treat its members as high-salience: they are the user's actively curated focus for this room. <instructions>, if present, are standing directions for how you should behave in this notebook — follow them. <memo>, if present, is a catch-up note about the notebook's current state. Members are also boosted in semantic search while this notebook is active.\n\nThe member list below IS the notebook's contents — it is already complete (up to the cap noted at its end). When the user refers to something \"in this notebook,\" match it here first; do not go looking for the notebook's contents with other tools.\n\nEach member carries what it is: `title`, `kind`, and `role`. `role=\"library\"` grounds this chat; `role=\"manuscript\"` is the user's own draft, deliberately excluded from retrieval — never cite it back at them as a source; `role=\"pin\"` is navigation only. Files also carry `text`: `indexed` means its contents are searchable, `pending` means extraction has not finished yet, and `none` means no text was extracted — searching for its contents will find nothing, so say so plainly rather than reporting an empty search as an absence of the thing.\n</active_notebook_preamble>";

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
    State(live_turns): State<LiveTurns>,
    user: AuthUser,
    Json(request): Json<ChatRequest>,
) -> Response {
    // One turn, one key. Nothing persists a turn id — a turn is a request, not
    // a row — so two turns in the same chat are otherwise indistinguishable in
    // the log, which is exactly what you are reading the log to tell apart.
    // The span nests inside the request span, so a line carries both this and
    // the `request_id` the client was handed back.
    //
    // `.instrument()` on the future, not `.entered()` in the body: this is an
    // async handler, and the guard `entered()` returns is not `Send`, so
    // holding one across an await makes the handler future non-`Send` — which
    // axum rejects, confusingly, as "not a Handler".
    let span = crate::observe::turn_span(&request.chat_id, &crate::observe::new_turn_id());
    chat_handler_inner(
        State(pool),
        State(yjs_state),
        State(cancel_state),
        State(live_turns),
        user,
        Json(request),
    )
    .instrument(span)
    .await
}

async fn chat_handler_inner(
    State(pool): State<PgPool>,
    State(yjs_state): State<YjsState>,
    State(cancel_state): State<ChatCancellationState>,
    State(live_turns): State<LiveTurns>,
    _user: AuthUser,
    Json(mut request): Json<ChatRequest>,
) -> Response {
    // The narrative interview is a MODE OF THE CHAT, decided by the chat id —
    // never by what the client sent. Any surface that opens this conversation
    // gets the interviewer (its standalone prompt, zero tools); no client can
    // opt the interview into tools by sending a different agentMode.
    if request.chat_id == crate::api::narrative_draft::INTERVIEW_CHAT_ID {
        request.agent_mode = "interview".to_string();
    }
    // Getting started is the same kind of room: its mode is the chat id's —
    // and, once the interview has begun inside it, the interviewer's.
    if request.chat_id == crate::api::getting_started::GETTING_STARTED_CHAT_ID {
        request.agent_mode = match crate::api::getting_started::compute(&pool).await {
            Ok(s) => s.agent_mode().to_string(),
            Err(e) => {
                tracing::warn!(error = %e, "getting-started state unavailable; setup mode");
                crate::api::getting_started::AGENT_MODE.to_string()
            }
        };
    }

    // No model, no turn — said in one sentence here, not as whatever the
    // gateway call fails with. Any room: the composer is inert while locked,
    // but no client can be trusted to be.
    if !crate::api::getting_started::ai_connected(&pool).await {
        return (
            StatusCode::CONFLICT,
            Json(ChatError {
                error: "AI is not connected".to_string(),
                details: Some(
                    "This server has nothing to answer with yet. Connect a Virtues subscription or your own AI endpoint in Getting started."
                        .to_string(),
                ),
            }),
        )
            .into_response();
    }

    // Which model answers. One door — the box decides, from the mode and the
    // owner's pin; the request's `model` is a per-turn override and nothing
    // else. See `model_choice` for why the interview refuses that override,
    // and why an empty string means "I did not choose".
    let model = match crate::api::model_choice::resolve_turn_model(
        &pool,
        request.model.as_deref(),
        &request.agent_mode,
    )
    .await
    {
        Ok(m) => m,
        // Name the id we rejected. The version of this that listed 244
        // allowed ids and never the offending one cost a day of debugging.
        Err(crate::error::Error::InvalidInput(detail)) => {
            tracing::warn!(
                requested = ?request.model,
                agent_mode = %request.agent_mode,
                "rejected per-turn model"
            );
            return (
                StatusCode::BAD_REQUEST,
                Json(ChatError {
                    error: "Invalid model".to_string(),
                    details: Some(detail),
                }),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to resolve the model for this turn");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ChatError {
                    error: "Failed to resolve model".to_string(),
                    details: Some(e.to_string()),
                }),
            )
                .into_response();
        }
    };
    // Deliberately NOT written back onto `request`: the resolved address has
    // one home from here down, threaded as an argument. `request.model` stays
    // what the client sent, which is the only thing it ever meant.


    // Use client-provided message ID for idempotency, or generate one
    let msg_id = request
        .message_id
        .clone()
        .unwrap_or_else(|| format!("msg_{}", generate_id()));

    // The June-era chat onboarding is DELETED (2026-09-01) — onboarding is
    // the founder's letter + Home's getting-started page, and the assistant's
    // first conversation is an ordinary one. onboarding_status still gates
    // those surfaces; the chat no longer reads it.

    // Ensure chat exists - use ON CONFLICT DO NOTHING to handle race conditions
    let chat_id_str = request.chat_id.clone();
    let title = {
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

    // A ghost chat has no row. Everything below that reads or writes
    // app_chats / app_chat_messages / app_chat_usage branches on this.
    let temporary = request.temporary;

    // Use ON CONFLICT DO NOTHING to handle concurrent requests for same chat
    // Returns rows_affected = 1 if inserted, 0 if already exists
    let chat_was_created = if temporary {
        false
    } else {
        match sqlx::query("INSERT INTO app_chats (id, title, message_count) VALUES ($1, $2, 0) ON CONFLICT (id) DO NOTHING")
            .bind(&chat_id_str)
            .bind(&title)
            .execute(&pool)
            .await
        {
            Ok(result) => result.rows_affected() > 0,
            Err(e) => {
                tracing::error!("Failed to create chat: {}", e);
                false
            }
        }
    };

    if chat_was_created {
        if let Err(e) =
            crate::api::notebooks::set_chat_notebook(&pool, &chat_id_str, request.notebook_id.as_deref()).await
        {
            tracing::warn!("Failed to set chat notebook: {}", e);
        }
    }

    // Regenerate: the client has dropped its last assistant message and is
    // asking for the last user turn to be answered again. Drop the box's copy
    // too, or the model answers with its previous reply in front of it.
    let regenerating = matches!(
        request.trigger.as_deref(),
        Some("regenerate-message") | Some("regenerate-assistant-message")
    );
    if regenerating && !temporary {
        if let Err(e) = sqlx::query(
            "DELETE FROM app_chat_messages \
             WHERE chat_id = $1 AND role = 'assistant' \
               AND sequence_num > COALESCE((SELECT MAX(sequence_num) FROM app_chat_messages \
                                            WHERE chat_id = $1 AND role = 'user'), 0)",
        )
        .bind(&chat_id_str)
        .execute(&pool)
        .await
        {
            tracing::error!(chat_id = %chat_id_str, error = %e, "regenerate: could not drop the previous answer");
        }
    }

    // Save the last user message to the chat. Not on regenerate: there is no
    // new user turn, and a client that still sends the full history would
    // otherwise re-append the last one.
    if let Some(last_user_msg) = request.messages.iter().rev().filter(|_| !regenerating).find(|m| m.role == "user") {
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
            reasoning_details: None,
        };

        if temporary {
            // Ghost: the message lives in the client's tab and nowhere else.
        } else if let Err(e) = append_message(&pool, request.chat_id.clone(), user_message).await {
            tracing::error!("Failed to save user message: {}", e);
        }
    }

    // Check if compaction is needed before sending to LLM. A ghost chat has
    // no usage row to read and no summary to write, so it never compacts.
    let compaction_needed = if temporary {
        false
    } else {
        let compaction_status =
            crate::api::chat_usage::check_compaction_needed(&pool, request.chat_id.clone(), &model)
                .await;
        // Pass compaction_needed flag to stream - compaction will run inside stream
        // and emit a checkpoint event for real-time UI updates
        matches!(compaction_status, Ok(ContextStatus::Critical))
    };

    use sqlx::Row;

    // Load chat from DB and build context with compaction summary
    let (conversation_summary, summary_up_to_index): (Option<String>, i64) = if temporary {
        (None, 0)
    } else {
        match sqlx::query(
            r#"SELECT conversation_summary, summary_up_to_index
               FROM app_chats WHERE id = $1"#,
        )
        .bind(&chat_id_str)
        .fetch_one(&pool)
        .await
        {
            Ok(row) => (row.get("conversation_summary"), row.get("summary_up_to_index")),
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
        }
    };

    // The transcript: the box's rows, or for a ghost chat the client's copy.
    let messages: Vec<ChatMessage> = if temporary {
        ghost_history(&request.messages)
    } else {
        // Load messages from normalized table
        let message_rows = match sqlx::query(
            r#"
            SELECT
                id, role, content, created_at, model, provider, agent_id,
                reasoning, tool_calls, intent, subject, reasoning_details, parts
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
        message_rows
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
                let reasoning_details: Option<serde_json::Value> = msg.get("reasoning_details");
                let parts_raw: Option<serde_json::Value> = msg.get("parts");

                // Parse JSON fields. A shape these no longer understand is a
                // turn that silently loses its tool calls or its order and
                // falls back to flat text — the kind of thing that reads as
                // "this message had no tools" rather than as a bug, so it says
                // so out loud.
                let tool_calls = tool_calls_raw.and_then(|t| {
                    serde_json::from_value(t)
                        .map_err(|e| tracing::warn!(msg_id = %id, error = %e, "tool_calls did not parse"))
                        .ok()
                });
                let intent = intent_raw.and_then(|i| serde_json::from_value(i).ok());
                let parts = parts_raw.and_then(|p| parts_from_jsonb(p, &id));

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
                    reasoning_details,
                }
            })
            .collect()
    };

    // Resolve the chat's room from the persisted row (single source of truth) so
    // the active-notebook context always matches the binding, even if a stale client
    // sends a different per-message notebookId. The create path above already bound a
    // new chat from request.notebook_id, so the row is current by now.
    // Decoded as `Option<String>` on purpose: `app_chats.notebook_id` is nullable
    // (and the FK is ON DELETE SET NULL), so an unbound chat legitimately reads
    // NULL. Scalar-typing it as `String` would make that NULL a decode *error* —
    // which is what the old `.ok()` was really swallowing, alongside every real
    // query failure. A swallow here is not cosmetic: None reads as "not in a
    // notebook", so a broken query silently unscopes a scoped chat — retrieval
    // stops being hard-filtered and the answer contract below is dropped.
    // A ghost chat has no row to read it from; the request is the binding.
    let effective_notebook_id: Option<String> = if temporary {
        request.notebook_id.clone()
    } else {
        match sqlx::query_scalar::<_, Option<String>>(
        r#"SELECT notebook_id FROM app_chats WHERE id = $1"#,
    )
    .bind(&chat_id_str)
    .fetch_optional(&pool)
    .await
    {
        // Outer None = no such row, inner None = bound to no notebook.
        Ok(notebook_id) => notebook_id.flatten(),
        Err(e) => {
            tracing::error!("Failed to resolve notebook for chat {}: {}", chat_id_str, e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ChatError {
                    error: "Failed to resolve chat notebook".to_string(),
                    details: Some(e.to_string()),
                }),
            )
                .into_response();
        }
        }
    };

    // Build system prompt with active page context, timezone, personalization, and agent mode
    let mut system_prompt = build_system_prompt(&pool, request.active_page.as_ref(), request.timezone.as_deref(), &request.agent_mode, &request.persona, effective_notebook_id.as_deref()).await;
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

    // Build context using compaction summary if available
    let api_messages = build_context_for_llm(
        &messages,
        conversation_summary.as_deref(),
        summary_up_to_index as usize,
        Some(&system_prompt),
    );

    // The turn is driven by its own task and outlives this request: a tab
    // switched or a phone locked used to drop the response, the loop with
    // it, and the assistant row was never written (VIR-323). This response
    // is one watcher on the turn; `GET /api/chat/{id}/stream` is another.
    let turn = live_turns.start(&chat_id_str);
    let agent_stream = create_agent_stream(
        pool,
        yjs_state,
        cancel_state.clone(),
        request,
        model,
        api_messages,
        msg_id,
        compaction_needed,
    );
    {
        let turn = turn.clone();
        let live_turns = live_turns.clone();
        let chat_id = chat_id_str.clone();
        tokio::spawn(async move {
            // The drive loop runs inside a catch, because `finish` is what
            // ends the turn for every watcher and a panic used to skip it:
            // `done` stayed false, `watch` blocked on a Notify that never
            // fired, and the SSE keep-alive held the socket open, so the
            // person got a thinking mark that spun until they gave up — no
            // error, no [DONE], and every later rejoin attached to the same
            // dead turn. A panic in here has to end the turn like any other
            // ending, or one bad decode wedges the chat.
            let drive = {
                let turn = turn.clone();
                let chat_id = chat_id.clone();
                async move {
                    let mut agent_stream = agent_stream;
                    while let Some(data) = agent_stream.next().await {
                        turn.push(data);
                        // Nobody watching for the cap: stop spending on a reply no
                        // one will read. The loop sees the token at its next step
                        // and the row is saved as a stop, like the button.
                        if turn.unattended_past(live_turn::UNATTENDED_CAP) {
                            tracing::info!(chat_id = %chat_id, cap_secs = live_turn::UNATTENDED_CAP.as_secs(), "turn unattended past the cap; cancelling");
                            cancel_state.cancel(&chat_id);
                        }
                    }
                }
            };
            if std::panic::AssertUnwindSafe(drive).catch_unwind().await.is_err() {
                tracing::error!(chat_id = %chat_id, "the turn's driver panicked; ending the turn");
                turn.push(serialize_event(&StreamEvent::Error {
                    error_text: "the box failed while writing this reply".to_string(),
                }));
                turn.push("[DONE]".to_string());
            }
            // After the stream's own tail (row written, usage recorded), so
            // a watcher that sees the end can reload and find the row.
            live_turns.finish(&chat_id, &turn);
        });
    }

    ui_stream_response(live_turn::watch(turn))
}

/// GET /api/chat/{id}/stream — the turn still running for this chat, from
/// its first event: everything said so far, then the rest as it happens.
/// `204 No Content` when nothing is running, which is what the AI SDK's
/// `resumeStream` expects for "nothing to resume".
pub async fn live_turn_stream_handler(
    State(live_turns): State<LiveTurns>,
    _user: AuthUser,
    Path(chat_id): Path<String>,
) -> Response {
    match live_turns.get(&chat_id) {
        Some(turn) => ui_stream_response(live_turn::watch(turn)),
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

/// An SSE response in the AI SDK's UI Message Stream protocol (the header
/// is what the SDK keys on).
fn ui_stream_response<S>(stream: S) -> Response
where
    S: Stream<Item = Result<SseEvent, Infallible>> + Send + 'static,
{
    let mut response = Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::new())
        .into_response();
    response.headers_mut().insert(
        axum::http::header::HeaderName::from_static("x-vercel-ai-ui-message-stream"),
        axum::http::HeaderValue::from_static("v1"),
    );
    response
}

/// Create the SSE stream using the AgentLoop for tool execution
fn create_agent_stream(
    pool: PgPool,
    yjs_state: YjsState,
    cancel_state: ChatCancellationState,
    request: ChatRequest,
    // Already resolved by `model_choice::resolve_turn_model` — passed in
    // rather than re-read off the request so the stream cannot disagree with
    // what the handler decided, or fall back to a default of its own.
    model: String,
    api_messages: Vec<serde_json::Value>,
    msg_id: String,
    compaction_needed: bool,
) -> Pin<Box<dyn Stream<Item = String> + Send>> {
    let chat_id = request.chat_id.clone();
    // Copied out for the stream block below, which reads `request` for a
    // few fields and must not persist a ghost turn either.
    let temporary = request.temporary;
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
                        yield (serialize_event(&checkpoint_event));
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

        let tools = crate::tools::get_tools_for_agent_mode(&request.agent_mode);

        // The message opens, then its first step. Text and reasoning parts
        // open lazily inside a step and close with it: the SDK forgets its
        // open parts at every finish-step, so a part that spans steps is a
        // delta with no home. (The turn used to stream as one text part
        // for its whole length, which is why it could never emit steps.)
        yield (serialize_event(&StreamEvent::Start { message_id: msg_id.clone() }));
        yield (serialize_event(&StreamEvent::StartStep));

        // Track accumulated content
        let mut full_content = String::new();
        let mut reasoning_content = String::new();
        let mut in_reasoning = false;
        let mut text_open = false;
        // The row stores the turn's text as one string, so text that resumes
        // after a tool call gets a paragraph break IN THE ROW ("…exact
        // text.The earlier edit…" otherwise). On the wire each step's text is
        // its own part and needs none.
        let mut needs_text_break = false;
        // ONE PART PER TEXT RUN, and a distinct id for each.
        //
        // The comment above has always said "on the wire each step's text is
        // its own part" — and the events were right, but every one of them
        // carried `msg_id`. The AI SDK keys a part by its id, so a second
        // `text-start` with the id it just closed REOPENS the first part and
        // appends to it. Every run of text in a turn merged into one block,
        // which is why the line the model writes before reaching for a tool
        // ("Checking what he sent in August") is indistinguishable, on the
        // client, from the answer it writes at the end.
        //
        // They are not the same thing. The first is scaffolding — it says what
        // is about to happen and is worth reading WHILE it happens; the second
        // is the reply. Giving each run its own id is what lets the view put
        // the scaffolding in the thinking block and leave the answer in the
        // transcript.
        let mut text_seq = 0usize;
        let mut text_part_id = msg_id.clone();
        let mut text_segments: Vec<String> = Vec::new();
        // The turn in order, so the stored `parts` can interleave text with the
        // tool calls it ran between — a reload then sees what the stream saw.
        let mut turn_slots: Vec<TurnSlot> = Vec::new();
        // Set by any error event mid-turn: the reply on screen is partial,
        // and the row must say so or a reload shows the stub as the answer.
        let mut interrupted = false;
        // How the last LLM step ended, and how the loop ended: together they
        // are the `finish` event's reason and the row's subject.
        let mut last_step_reason: Option<StepReason> = None;
        let mut loop_finish: Option<FinishReason> = None;
        // The gateway's reasoning blocks across the turn's steps, for the row.
        let mut reasoning_details: Vec<serde_json::Value> = Vec::new();

        // Token usage tracking
        let mut total_input_tokens: u32 = 0;
        let mut total_output_tokens: u32 = 0;
        let mut total_reasoning_tokens: u32 = 0;
        let mut total_cache_read_tokens: u32 = 0;
        // Authoritative spend for this turn (sum of gateway-reported usage.cost
        // across every step), captured into app_ai_calls for the Usage tab.
        let mut total_cost_micros: i64 = 0;

        // Tool call tracking for persistence
        let mut all_tool_calls: Vec<ToolCall> = Vec::new();
        let mut failed_tools: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Run the agent loop with cancellation support
        let mut agent_stream = agent.run(
            model.clone(),
            api_messages.clone(),
            tools,
            context,
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
                yield (serialize_event(&ev));
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
                        yield (serialize_event(&event));
                    }
                    if !text_open {
                        text_open = true;
                        text_seq += 1;
                        text_part_id = format!("{msg_id}:t{text_seq}");
                        turn_slots.push(TurnSlot::Text(text_segments.len()));
                        text_segments.push(String::new());
                        yield (serialize_event(&StreamEvent::TextStart { id: text_part_id.clone() }));
                    }
                    // Text resuming after a tool call: break the paragraph in
                    // the stored string so it doesn't butt against the previous
                    // segment's final sentence.
                    if needs_text_break && !full_content.is_empty() && !full_content.ends_with('\n') {
                        full_content.push_str("\n\n");
                    }
                    needs_text_break = false;
                    full_content.push_str(&content);
                    if let Some(seg) = text_segments.last_mut() {
                        seg.push_str(&content);
                    }
                    let event = StreamEvent::TextDelta {
                        id: text_part_id.clone(),
                        delta: content,
                    };
                    yield (serialize_event(&event));
                }

                AgentEvent::ReasoningDelta { content } => {
                    if !in_reasoning {
                        in_reasoning = true;
                        let event = StreamEvent::ReasoningStart { id: msg_id.clone() };
                        yield (serialize_event(&event));
                    }
                    reasoning_content.push_str(&content);
                    let event = StreamEvent::ReasoningDelta {
                        id: msg_id.clone(),
                        delta: content,
                    };
                    yield (serialize_event(&event));
                }

                AgentEvent::ToolCallStart { id, name, args } => {
                    // Any text that resumes after this tool call starts a new
                    // paragraph (see needs_text_break).
                    needs_text_break = true;
                    turn_slots.push(TurnSlot::Tool(id.clone()));
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
                    yield (serialize_event(&event));
                }

                AgentEvent::ToolCallArgsPartial { id, args_delta } => {
                    // AI SDK v6: tool-input-delta event
                    let event = StreamEvent::ToolInputDelta {
                        tool_call_id: id,
                        input_text_delta: args_delta,
                    };
                    yield (serialize_event(&event));
                }

                AgentEvent::ToolCallArgsComplete { id, args } => {
                    // AI SDK v6: tool-input-available event (args parsing complete)
                    // This is where the arguments become known: ToolCallStart
                    // fires as soon as the tool has a name, and the args are
                    // still streaming in then, so the tracked call is holding
                    // `Null`. Writing them back here is what puts them in the
                    // persisted row — without it a reload showed a call with
                    // no input, and the replayed turn carried none either.
                    let tool_name = all_tool_calls.iter_mut()
                        .find(|tc| tc.tool_call_id.as_deref() == Some(&id))
                        .map(|tc| {
                            tc.arguments = args.clone();
                            tc.tool_name.clone()
                        })
                        .unwrap_or_default();
                    let event = StreamEvent::ToolInputAvailable {
                        tool_call_id: id,
                        tool_name,
                        input: args,
                    };
                    yield (serialize_event(&event));
                }

                AgentEvent::ToolCallResult { id, result, success: false, error } => {
                    // A failed tool is a tool error on the wire, not an output
                    // with an error inside it. The model still sees the
                    // failure text (executor::to_llm_content); the row keeps
                    // it as the result so a reload shows the same.
                    let error_text = error
                        .or_else(|| result.get("error").and_then(|e| e.as_str()).map(str::to_string))
                        .unwrap_or_else(|| "the tool reported a failure".to_string());
                    if let Some(tc) = all_tool_calls.iter_mut().find(|tc| tc.tool_call_id.as_deref() == Some(&id)) {
                        tc.result = Some(serde_json::json!({ "error": error_text }));
                    }
                    // Which calls failed is only known here. `{"error": …}` in
                    // the result is not the same claim — a tool may answer with
                    // an `error` key of its own — so the ids are kept.
                    failed_tools.insert(id.clone());
                    let event = StreamEvent::ToolOutputError { tool_call_id: id, error_text };
                    yield (serialize_event(&event));
                }

                AgentEvent::ToolCallResult { id, result, success: true, error: _ } => {
                    // Update the tracked tool call with the result
                    if let Some(tc) = all_tool_calls.iter_mut().find(|tc| tc.tool_call_id.as_deref() == Some(&id)) {
                        tc.result = Some(result.clone());
                    }
                    // The interview's finisher names the page the client
                    // should open beside the chat (see NarrativeDocumentReady).
                    let narrative_page = all_tool_calls
                        .iter()
                        .find(|tc| tc.tool_call_id.as_deref() == Some(&id))
                        .filter(|tc| tc.tool_name == "write_it_up")
                        .and_then(|_| result.get("document_page_id"))
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
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
                    yield (serialize_event(&event));
                    if let Some(page_id) = narrative_page {
                        let event = StreamEvent::NarrativeDocumentReady { page_id };
                        yield (serialize_event(&event));
                    }
                }

                AgentEvent::Usage { prompt_tokens, completion_tokens, total_tokens: _, reasoning_tokens, cache_read_tokens, cost_micros } => {
                    total_input_tokens += prompt_tokens;
                    total_output_tokens += completion_tokens;
                    if let Some(r) = reasoning_tokens {
                        total_reasoning_tokens += r;
                    }
                    if let Some(c) = cache_read_tokens {
                        total_cache_read_tokens += c;
                    }
                    if let Some(c) = cost_micros {
                        total_cost_micros += c;
                    }
                }

                AgentEvent::ReasoningDetails { details } => {
                    reasoning_details.extend(details);
                }

                AgentEvent::Error { message, code: _, recoverable: _ } => {
                    interrupted = true;
                    let event = StreamEvent::Error { error_text: message };
                    yield (serialize_event(&event));
                }

                // A step ended. Close it on the wire; when the model asked for
                // tools, the next LLM call is a new step and opens one.
                AgentEvent::StepComplete { reason, .. } => {
                    last_step_reason = Some(reason);
                    if in_reasoning {
                        in_reasoning = false;
                        yield (serialize_event(&StreamEvent::ReasoningEnd { id: msg_id.clone() }));
                    }
                    if text_open {
                        text_open = false;
                        yield (serialize_event(&StreamEvent::TextEnd { id: text_part_id.clone() }));
                    }
                    yield (serialize_event(&StreamEvent::FinishStep));
                    if reason == StepReason::ToolCalls {
                        yield (serialize_event(&StreamEvent::StartStep));
                    }
                }

                AgentEvent::Done { finish_reason, .. } => {
                    loop_finish = Some(finish_reason);
                }

                // Events we don't need to forward to client
                AgentEvent::LoopStarted { .. } |
                AgentEvent::MessageId { .. } => {}
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
            yield (serialize_event(&ev));
        }

        // End reasoning if we were in it
        if in_reasoning {
            let event = StreamEvent::ReasoningEnd { id: msg_id.clone() };
            yield (serialize_event(&event));
        }

        // Close a text part a step left open (an error or a stop mid-step).
        if text_open {
            yield (serialize_event(&StreamEvent::TextEnd { id: text_part_id.clone() }));
        }

        // How it ended, in the SDK's words. A person's stop is an abort, not
        // a finish. Otherwise the loop's verdict wins over the last step's,
        // and the last step's over "stop".
        let was_cancelled = cancel_token.is_cancelled();
        let cut_short = last_step_reason == Some(StepReason::MaxTokens);
        if was_cancelled || loop_finish == Some(FinishReason::Cancelled) {
            yield (serialize_event(&StreamEvent::Abort { reason: Some("stopped".to_string()) }));
        } else {
            let finish_reason = match (loop_finish, last_step_reason) {
                (Some(FinishReason::Error), _) => "error",
                (Some(FinishReason::MaxSteps), _) | (Some(FinishReason::AwaitingUser), _) => "other",
                (_, Some(StepReason::MaxTokens)) => "length",
                (_, Some(StepReason::ContentFilter)) => "content-filter",
                (_, Some(StepReason::ToolCalls)) => "tool-calls",
                _ if interrupted => "error",
                _ => "stop",
            };
            yield (serialize_event(&StreamEvent::Finish { finish_reason: finish_reason.to_string() }));
        }

        // Send [DONE] marker
        yield ("[DONE]".to_string());

        // Save assistant message to chat.
        //
        // Text is not the only thing a turn produces. One that called tools and
        // was stopped — or hit the step ceiling — before it wrote a word left
        // NOTHING on disk, so a reload erased calls the person had watched run,
        // and the "cancelled"/"interrupted" notice below had no row to hang on.
        // A turn that produced neither text nor a call is still not a turn.
        if !full_content.is_empty() || !all_tool_calls.is_empty() {
            let provider = model.split('/').next().unwrap_or("unknown").to_string();
            // The row says how the turn ended so a reload shows the same
            // notice: the person's stop, the output cap, or an interruption.
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
                // "interrupted": the stream or the model stopped before the
                // reply was finished (VIR-334). The UI reads both on reload
                // and shows a notice under the stub; a person's stop wins.
                subject: if was_cancelled {
                    Some("cancelled".to_string())
                } else if cut_short {
                    Some("length".to_string())
                } else if interrupted {
                    Some("interrupted".to_string())
                } else {
                    None
                },
                reasoning_details: if reasoning_details.is_empty() {
                    None
                } else {
                    Some(serde_json::Value::Array(reasoning_details.clone()))
                },
                // THE TURN IN ORDER — the column was written as `None` by every
                // caller and was null in every row on every box, while the
                // client rebuilt an approximation from `content` + `tool_calls`
                // that could only ever produce ONE text part. That is what made
                // a reopened chat unable to tell the model's "checking his
                // messages now" from its actual answer: the distinction was
                // never on disk to begin with.
                //
                // `content` still holds the whole turn joined, because that is
                // what the model is shown as its own history and what every
                // older row has. This is additive: a row without `parts` falls
                // back to the legacy reconstruction exactly as before.
                parts: {
                    let parts = build_turn_parts(
                        &turn_slots,
                        &text_segments,
                        &all_tool_calls,
                        &failed_tools,
                        &reasoning_content,
                    );
                    if parts.is_empty() { None } else { Some(parts) }
                },
            };

            if temporary {
                // Ghost: nothing written. The client keeps the turn in its tab.
            } else if let Err(e) = append_message(&pool, chat_id.clone(), assistant_message).await {
                tracing::error!("Failed to save assistant message: {}", e);
            } else if chat_id == crate::api::getting_started::GETTING_STARTED_CHAT_ID {
                /* THE ROOM SPEAKS AFTER THE TURN, NOT THIRTY SECONDS LATER.
                 *
                 * `narrate` used to run only from the GET and the two POST
                 * handlers, and no TOOL called it — so a step settled by
                 * `record_introductions` or `write_it_up` left the room
                 * silent until the client's 30-second poll. Because narrate
                 * only ever appends, the line then landed at the BOTTOM of
                 * the transcript rather than where it belonged in the walk.
                 *
                 * Measured on the dev box: introductions were recorded at
                 * message 4 and "Introductions are made." arrived at message
                 * 28 — under the person's "whats next?", together with the
                 * other three settled lines and the graduated line, as a
                 * backlog. Worse, the MODEL had already answered the question
                 * at 27 in its own words, so the authored ending landed
                 * underneath a thinner version of itself.
                 *
                 * Here is the right instant: the assistant's turn is on disk,
                 * so the coda appends directly beneath the sentence that
                 * earned it. Best-effort — a room that cannot speak must
                 * never fail a turn that already succeeded.
                 */
                match crate::api::getting_started::compute(&pool).await {
                    Ok(state) => {
                        if let Err(e) = crate::api::getting_started::narrate(&pool, &state).await {
                            tracing::warn!(error = %e, "the room could not speak after a turn");
                        }
                    }
                    Err(e) => tracing::warn!(error = %e, "getting-started state after a turn"),
                }
            }

            // Record token usage. `cost_micros` is the gateway's authoritative
            // figure — the same one recorded in app_ai_calls below, and the one
            // the wallet was actually debited for. No estimating.
            // These were literal zeros while the same totals were handed to
            // `app_ai_calls` one call below — so the per-chat panel read 0
            // reasoning tokens on every box while the number sat in the next
            // table over.
            let usage_data = UsageData {
                input_tokens: total_input_tokens as i64,
                output_tokens: total_output_tokens as i64,
                reasoning_tokens: total_reasoning_tokens as i64,
                cache_read_tokens: total_cache_read_tokens as i64,
                cache_write_tokens: 0,
                cost_micros: Some(total_cost_micros),
            };

            if temporary {
                // Ghost: app_chat_usage keys on a chat row that does not exist.
                // app_ai_calls below still records cost and counts, no content.
            } else if let Err(e) = record_chat_usage(&pool, chat_id.clone(), &model, usage_data).await {
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
                    // `stream()` diverts to the user's endpoint when BYO is
                    // set, and no upstream but our own gateway sends a cost
                    // trailer — so `total_cost_micros` is 0-as-unknown there,
                    // and the Usage tab must show tokens instead of "$0.00".
                    route: if crate::api::settings_byo::byo_is_active(&pool).await {
                        crate::api::ai_calls::Route::Byo
                    } else {
                        crate::api::ai_calls::Route::Wallet
                    },
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

    fn tool(id: &str, name: &str, result: Option<serde_json::Value>) -> crate::api::chats::ToolCall {
        crate::api::chats::ToolCall {
            tool_name: name.to_string(),
            tool_call_id: Some(id.to_string()),
            arguments: serde_json::json!({}),
            result,
            timestamp: "2026-09-16T00:00:00Z".to_string(),
        }
    }

    fn kinds(parts: &[UIPart]) -> Vec<String> {
        parts
            .iter()
            .map(|p| match p {
                UIPart::Text { text } => format!("text:{}", text.trim()),
                UIPart::ToolInvocation { tool_name, state, .. } => {
                    format!("tool:{tool_name}:{state}")
                }
                _ => "other".to_string(),
            })
            .collect()
    }

    /// The order is the whole point. Without it a reload can say what was said
    /// and what was called, but never which was said BEFORE which call — and
    /// that distinction is what separates the model narrating its way to an
    /// answer from the answer itself.
    #[test]
    fn a_turn_keeps_the_order_its_text_and_tools_happened_in() {
        let slots = vec![
            TurnSlot::Text(0),
            TurnSlot::Tool("c1".into()),
            TurnSlot::Text(1),
            TurnSlot::Tool("c2".into()),
            TurnSlot::Text(2),
        ];
        let segments = vec![
            "Checking his messages.".to_string(),
            "Nothing in August. Looking at September.".to_string(),
            "He sent it on the 3rd.".to_string(),
        ];
        let calls = vec![
            tool("c1", "sql_query", Some(serde_json::json!({"rows": []}))),
            tool("c2", "semantic_search", Some(serde_json::json!({"rows": []}))),
        ];

        assert_eq!(
            kinds(&build_turn_parts(&slots, &segments, &calls, &Default::default(), "")),
            [
                "text:Checking his messages.",
                "tool:sql_query:output-available",
                "text:Nothing in August. Looking at September.",
                "tool:semantic_search:output-available",
                "text:He sent it on the 3rd.",
            ]
        );
    }

    /// A turn that ends on a tool call has no reply yet. The view reads "text
    /// with a tool after it" as narration, so an empty trailing run must not be
    /// written — it would present itself as an answer of nothing.
    #[test]
    fn empty_runs_and_unrecorded_tools_are_left_out() {
        let slots = vec![
            TurnSlot::Text(0),
            TurnSlot::Tool("c1".into()),
            TurnSlot::Text(1),
            // Started, never recorded: the stream ended in between.
            TurnSlot::Tool("c_ghost".into()),
        ];
        let segments = vec!["Looking it up.".to_string(), "   \n ".to_string()];
        let calls = vec![tool("c1", "sql_query", None)];

        assert_eq!(
            kinds(&build_turn_parts(&slots, &segments, &calls, &Default::default(), "")),
            ["text:Looking it up.", "tool:sql_query:input-available"],
            "a tool that never returned still shows, as awaiting output"
        );
    }

    /// The overwhelmingly common turn: a question, an answer, no tools. It must
    /// come out as one text part, or every plain reply would look like narration.
    #[test]
    fn a_turn_with_no_tools_is_all_reply() {
        let parts = build_turn_parts(&[TurnSlot::Text(0)], &["Yes.".to_string()], &[], &Default::default(), "");
        assert_eq!(kinds(&parts), ["text:Yes."]);
    }

    /// The rules block is the one place where a silent failure means the box
    /// raises a subject someone asked it never to raise. These tests exist
    /// because that failure is invisible from the outside: the prompt still
    /// builds, chat still answers, and nothing looks wrong.
    #[sqlx::test]
    async fn rules_are_grouped_by_kind(pool: PgPool) {
        sqlx::query(
            "INSERT INTO wiki_rules (id, rule, kind) VALUES
                ('r1', 'my father', 'avoid'),
                ('r2', 'the morning pages', 'defend'),
                ('r3', 'drinking', 'avoid')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let out = build_rules(&pool).await;

        // Each kind under its own heading, or the prompt cannot give them the
        // opposite handling they need.
        let avoid_at = out.find("Never raise").expect("avoid heading");
        let defend_at = out.find("Help them hold").expect("defend heading");
        assert!(avoid_at < defend_at, "avoid group comes first:\n{out}");
        assert!(out.contains("- my father"));
        assert!(out.contains("- drinking"));
        assert!(out.contains("- the morning pages"));

        // The defend rule must not be filed under avoid.
        let defend_block = &out[defend_at..];
        assert!(!defend_block.contains("my father"), "kinds bled:\n{out}");
    }

    /// No rules must render NOTHING, not an empty heading — a `<rules>` block
    /// that is usually empty teaches a model to skim past it.
    #[sqlx::test]
    async fn no_rules_renders_nothing(pool: PgPool) {
        assert_eq!(build_rules(&pool).await, "");
    }

    /// Inactive rules are withdrawn, not merely hidden from the review screen.
    #[sqlx::test]
    async fn inactive_rules_are_not_enforced(pool: PgPool) {
        sqlx::query("INSERT INTO wiki_rules (id, rule, kind, active) VALUES ('r1', 'my father', 'avoid', false)")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(build_rules(&pool).await, "");
    }

    /// THE TEST THAT WOULD HAVE CAUGHT THIS.
    ///
    /// `build_rules` passing proves the grouping; only assembling the whole
    /// prompt proves the block is actually wired in. From 0101 until 2026-08-17
    /// every unit around this was fine and the rule still never reached a model,
    /// because nothing asserted on the finished prompt.
    #[sqlx::test]
    async fn rules_reach_the_assembled_prompt(pool: PgPool) {
        sqlx::query("INSERT INTO wiki_rules (id, rule, kind) VALUES ('r1', 'my brother Tom', 'avoid')")
            .execute(&pool)
            .await
            .unwrap();

        let prompt = build_system_prompt_for_audit(&pool).await;
        assert!(prompt.contains("<rules>"), "rules block never reached the prompt");
        assert!(prompt.contains("my brother Tom"), "the rule text is missing:\n{prompt}");
    }

    /// And the block is absent when there is nothing to say.
    #[sqlx::test]
    async fn empty_rules_leave_no_block(pool: PgPool) {
        let prompt = build_system_prompt_for_audit(&pool).await;
        // The <precedence> block legitimately NAMES <rules>; what must be
        // absent is the rules block's own body.
        assert!(
            !prompt.contains("has marked some things as rules"),
            "empty rules block was rendered"
        );
    }

    /// The registry IS the render order. This pins the current order so a
    /// future edit to the block list is a deliberate, test-visible act — the
    /// reorder slice (rules last, per the formula doc) flips this assertion
    /// on purpose, and nothing reorders by accident.
    #[sqlx::test]
    async fn prompt_blocks_render_in_registry_order(pool: PgPool) {
        sqlx::query("INSERT INTO wiki_rules (id, kind, rule) VALUES ('rule_t1', 'avoid', 'x')")
            .execute(&pool)
            .await
            .unwrap();
        let (prompt, rendered) = build_system_prompt_blocks(
            &pool, None, Some("America/Chicago"), "default", "default", None, "Ari",
            "Adam",
        )
        .await;

        let tags: Vec<&str> = rendered.iter().map(|r| r.tag).collect();
        // Blocks with no data render nothing; the ones that DO render must
        // appear in registry order, with the fused head first.
        assert_eq!(tags.first(), Some(&"base"), "the fused head must render first");
        assert_eq!(tags.last(), Some(&"rules"), "rules must render last — nearest the conversation");
        assert!(tags.contains(&"rules"), "seeded rule missing from the assembly");
        assert!(tags.contains(&"circumstances"));
        let expected = ["base", "precedence", "new_user", "memory", "circumstances", "active_notebook", "active_context", "rules"];
        let mut last = 0usize;
        for t in &tags {
            let pos = expected.iter().position(|e| e == t).expect("unknown block tag");
            assert!(pos >= last, "block {t} rendered out of registry order: {tags:?}");
            last = pos;
        }
        // And the assembly is still one contiguous prompt, not fragments.
        assert!(prompt.contains("<rules>") && prompt.contains("<circumstances>"));
    }

    #[test]
    fn test_generate_id() {
        let id1 = generate_id();
        let id2 = generate_id();
        assert_ne!(id1, id2);
        assert_eq!(id1.len(), 16); // 8 bytes = 16 hex chars
    }
}


#[cfg(test)]
mod ni_budget_tests {
    use super::{truncate_to_budget, NI_BUDGET_CHARS};

    #[test]
    fn short_documents_are_untouched() {
        let t = "I value patience.\n\nI want to finish the boat.";
        assert_eq!(truncate_to_budget(t, NI_BUDGET_CHARS), t);
    }

    /// The point of the budget is to stop somewhere a reader would stop.
    #[test]
    fn cuts_at_a_paragraph_break_when_there_is_one() {
        let t = "First para.\n\nSecond para is long and would be cut.";
        assert_eq!(truncate_to_budget(t, 25), "First para.");
    }

    #[test]
    fn falls_back_to_a_sentence_end() {
        let t = "One sentence here. Then a much longer second sentence follows.";
        assert_eq!(truncate_to_budget(t, 30), "One sentence here.");
    }

    /// Never mid-word: a document that stops mid-sentence reads as a thought
    /// the model should finish, and this one is about who a person is.
    #[test]
    fn never_splits_a_word() {
        let t = "aaaa bbbb cccc dddddddddddddddddddd";
        let out = truncate_to_budget(t, 14);
        assert!(!out.ends_with("cc"), "cut inside a word: {out:?}");
        assert_eq!(out, "aaaa bbbb");
    }
}

/// Renders the real active-notebook block against a dev database, so the text
/// the model actually receives is inspected rather than assumed. Ignored by
/// default — CI has no box database.
///   cargo test -p virtues --lib api::chat::live_notebook -- --ignored --nocapture
#[cfg(test)]
mod live_notebook {
    use sqlx::PgPool;

    #[tokio::test]
    #[ignore]
    async fn the_block_names_every_member() {
        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://virtues:virtues@localhost:5432/virtues".to_string());
        let pool = PgPool::connect(&url).await.expect("dev database");

        let id: Option<String> = sqlx::query_scalar(
            "SELECT notebook_id FROM app_notebook_items
             GROUP BY notebook_id ORDER BY count(*) DESC LIMIT 1",
        )
        .fetch_optional(&pool)
        .await
        .expect("query");
        let Some(id) = id else {
            println!("no notebooks with members; nothing to render");
            return;
        };

        let block = super::build_notebook_context(&pool, &id)
            .await
            .expect("a context block");
        println!("\n{block}\n");

        // A member line carrying only a url is the bug this replaced.
        for line in block.lines().filter(|l| l.trim_start().starts_with("<member")) {
            assert!(line.contains("role="), "member without role: {line}");
            assert!(
                line.contains("title=") || line.contains("/home"),
                "member reached the prompt as a bare url: {line}"
            );
        }
    }
}

/// Measures the real system prompt against a dev database. Not an assertion of
/// correctness — an audit instrument, so the prompt's size and composition are
/// observed rather than estimated.
///   cargo test -p virtues --lib api::live_prompt_audit -- --ignored --nocapture
#[cfg(test)]
mod live_prompt_audit {
    use sqlx::PgPool;

    #[tokio::test]
    #[ignore]
    async fn measure_the_assembled_system_prompt() {
        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://virtues:virtues@localhost:5432/virtues".to_string());
        let pool = PgPool::connect(&url).await.expect("dev database");

        let notebook: Option<String> = sqlx::query_scalar(
            "SELECT notebook_id FROM app_notebook_items
             GROUP BY notebook_id ORDER BY count(*) DESC LIMIT 1",
        )
        .fetch_optional(&pool)
        .await
        .expect("query");

        for (label, nb) in [("no notebook", None), ("in a notebook", notebook.as_deref())] {
            let p = super::build_system_prompt(
                &pool,
                None,
                Some("America/Chicago"),
                "default",
                "default",
                nb,
            )
            .await;

            // ~4 chars/token is the usual English approximation; this is an
            // order-of-magnitude reading, not a billing figure.
            println!(
                "\n=== {label} ===\n{} chars  (~{} tokens)",
                p.len(),
                p.len() / 4
            );
            for tag in [
                "<persona",
                "<narrative_identity",
                "<memory>",
                "<datetime>",
                "<user_context>",
                "<identity>",
                "<recent_days>",
                "<connected_sources>",
                "<active_notebook",
                "<active_context>",
            ] {
                if let Some(i) = p.find(tag) {
                    // Crude section sizing: distance to the next top-level tag.
                    let rest = &p[i + tag.len()..];
                    let end = rest.find("\n\n<").map(|e| e + tag.len()).unwrap_or(p.len() - i);
                    println!("  {:<22} {:>6} chars", tag, end);
                }
            }
        }
    }
}


#[cfg(test)]
mod parts_column_tests {
    use super::*;

    /// The client matches an exact type per tool — `tool-create_page`,
    /// `tool-generate_image` — and has no branch for anything else, so a part
    /// stored under the variant's own name renders as nothing on reload.
    #[test]
    fn a_tool_part_is_stored_under_its_tool_name() {
        let parts = vec![
            UIPart::Text { text: "Making the page.".to_string() },
            UIPart::ToolInvocation {
                tool_call_id: "call-0".to_string(),
                tool_name: "create_page".to_string(),
                input: serde_json::json!({ "title": "Notes" }),
                state: "output-available".to_string(),
                output: Some(serde_json::json!({ "page_id": "page_1" })),
                error_text: None,
            },
        ];

        let stored = parts_to_jsonb(&parts);
        let types: Vec<&str> =
            stored.as_array().unwrap().iter().map(|p| p["type"].as_str().unwrap()).collect();
        assert_eq!(types, ["text", "tool-create_page"]);
        assert_eq!(stored[1]["toolName"], "create_page");

        let back = parts_from_jsonb(stored, "msg_1").expect("reads back");
        assert!(matches!(&back[1], UIPart::ToolInvocation { tool_name, .. } if tool_name == "create_page"));
    }

    /// Rows written before the rename, and the AI SDK's own `tool-web_search`,
    /// both still have to read.
    #[test]
    fn the_older_spellings_still_read() {
        let stored = serde_json::json!([
            { "type": "tool-invocation", "toolCallId": "c0", "toolName": "sql_query",
              "input": {}, "state": "output-available", "output": {"rows": 1} },
            { "type": "tool-web_search", "toolCallId": "c1",
              "input": {}, "state": "output-available", "output": {} },
        ]);
        let back = parts_from_jsonb(stored, "msg_1").expect("reads back");
        assert_eq!(back.len(), 2);
        // The name is recovered from the type when the part does not carry one.
        assert!(matches!(&back[1], UIPart::ToolInvocation { tool_name, .. } if tool_name == "web_search"));
    }

    /// A failed tool is its own state, or the red block never renders and the
    /// replay hands the model an error object as though it were an answer.
    #[test]
    fn a_failed_tool_keeps_its_failure() {
        let mut failed = std::collections::HashSet::new();
        failed.insert("call-0".to_string());
        let calls = vec![ToolCall {
            tool_name: "create_page".to_string(),
            tool_call_id: Some("call-0".to_string()),
            arguments: serde_json::json!({}),
            result: Some(serde_json::json!({ "error": "the page already exists" })),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        }];
        let parts = build_turn_parts(
            &[TurnSlot::Tool("call-0".to_string())],
            &[],
            &calls,
            &failed,
            "",
        );
        match &parts[0] {
            UIPart::ToolInvocation { state, error_text, .. } => {
                assert_eq!(state, "output-error");
                assert_eq!(error_text.as_deref(), Some("the page already exists"));
            }
            other => panic!("expected a tool part, got {other:?}"),
        }
    }

    /// The thinking has its own column, but the client stopped reading it the
    /// moment `parts` existed.
    #[test]
    fn a_turn_that_thought_keeps_its_thinking() {
        let parts = build_turn_parts(
            &[TurnSlot::Text(0)],
            &["Yes.".to_string()],
            &[],
            &Default::default(),
            "Weighing it up.",
        );
        assert!(matches!(&parts[0], UIPart::Reasoning { text } if text == "Weighing it up."));
        assert!(matches!(&parts[1], UIPart::Text { .. }));
    }
}

#[cfg(test)]
mod checkpoint_tests {
    use super::*;
    use sqlx::PgPool;

    /// `parts` is jsonb and `created_at` is timestamptz. Reading either as a
    /// String panics rather than returning a wrong value, and this runs inside
    /// the turn's own task — so auto-compaction killed the reply it had just
    /// made room for. A pure test cannot catch it; only a real column can.
    #[sqlx::test]
    async fn a_checkpoint_row_decodes_into_its_event(pool: PgPool) {
        sqlx::query("INSERT INTO app_chats (id, title, message_count) VALUES ($1, $2, 0)")
            .bind("chat_cp")
            .bind("compacted")
            .execute(&pool)
            .await
            .unwrap();
        let parts = serde_json::json!([{
            "type": "checkpoint",
            "version": 3,
            "messages_summarized": 12,
            "summary": "<context>the earlier conversation</context>",
            "timestamp": "",
        }]);
        sqlx::query(
            "INSERT INTO app_chat_messages (id, chat_id, role, content, sequence_num, parts)
             VALUES ($1, $2, 'checkpoint', 'Checkpoint v3', 1, $3)",
        )
        .bind("msg_cp")
        .bind("chat_cp")
        .bind(&parts)
        .execute(&pool)
        .await
        .unwrap();

        let event = get_latest_checkpoint(&pool, "chat_cp")
            .await
            .expect("the checkpoint comes back as an event");
        match event {
            StreamEvent::Checkpoint { id, version, messages_summarized, timestamp, .. } => {
                assert_eq!(id, "msg_cp");
                assert_eq!(version, 3);
                assert_eq!(messages_summarized, 12);
                // Empty in the part, so the row's own created_at stands in.
                assert!(!timestamp.is_empty(), "falls back to created_at");
            }
            other => panic!("expected a checkpoint event, got {other:?}"),
        }
    }

    #[sqlx::test]
    async fn a_chat_with_no_checkpoint_has_none(pool: PgPool) {
        sqlx::query("INSERT INTO app_chats (id, title, message_count) VALUES ($1, $2, 0)")
            .bind("chat_plain")
            .bind("plain")
            .execute(&pool)
            .await
            .unwrap();
        assert!(get_latest_checkpoint(&pool, "chat_plain").await.is_none());
    }
}

#[cfg(test)]
mod ghost_tests {
    use super::*;

    fn ui(role: &str, text: Option<&str>, parts: Option<Vec<UIPart>>) -> UIMessage {
        UIMessage { id: None, role: role.into(), parts, content: text.map(str::to_string) }
    }

    /// A ghost chat's transcript is exactly what the client sent: user and
    /// assistant turns, text drawn from parts when there is no content, and
    /// nothing else (no system rows, no empty rows).
    #[test]
    fn ghost_history_is_the_wire_and_only_the_wire() {
        let history = ghost_history(&[
            ui("system", Some("ignored"), None),
            ui("user", None, Some(vec![UIPart::Text { text: "hello".into() }, UIPart::Text { text: "there".into() }])),
            ui("assistant", Some("hi"), None),
            ui("user", None, None),
        ]);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].role, "user");
        assert_eq!(history[0].content, "hello\nthere");
        assert_eq!(history[1].content, "hi");
    }
}

/// The UI-message stream, as the box emits it, recorded for the frontend.
///
/// Two fixtures under `apps/web/src/lib/ai/fixtures/` are the exact JSON
/// lines a browser receives for a canonical turn (two steps, a tool result,
/// a tool error, reasoning, a data part, `finish`) and for a stopped turn
/// (`abort`). A vitest there feeds them through the AI SDK's own transport
/// and parser. This test fails when the fixture no longer matches what
/// `serialize_event` produces, so a changed event shape cannot ship unseen;
/// regenerate with `UPDATE_FIXTURES=1 cargo test -p virtues --lib ui_stream_fixture`
/// and run `pnpm test:unit` in apps/web to see whether the SDK still parses it.
#[cfg(test)]
mod ui_stream_fixture {
    use super::*;

    fn canonical_turn() -> Vec<StreamEvent> {
        let id = "msg_fixture".to_string();
        // Each run of text carries its OWN id (`{msg}:t{n}`) — see the stream
        // loop. The step boundary is what actually ends a part, so the ids were
        // cosmetic until the view began telling the runs apart; they are here so
        // the fixture is what the box sends, not a simplification of it.
        let t1 = format!("{id}:t1");
        let t2 = format!("{id}:t2");
        vec![
            StreamEvent::Start { message_id: id.clone() },
            StreamEvent::StartStep,
            StreamEvent::TextStart { id: t1.clone() },
            StreamEvent::ReasoningStart { id: id.clone() },
            StreamEvent::ReasoningDelta { id: id.clone(), delta: "weighing the ask".into() },
            StreamEvent::ReasoningEnd { id: id.clone() },
            StreamEvent::TextDelta { id: t1.clone(), delta: "Hello".into() },
            StreamEvent::ToolInputStart { tool_call_id: "call_1".into(), tool_name: "web_search".into() },
            StreamEvent::ToolInputDelta { tool_call_id: "call_1".into(), input_text_delta: "{\"query\":\"x\"}".into() },
            StreamEvent::ToolInputAvailable {
                tool_call_id: "call_1".into(),
                tool_name: "web_search".into(),
                input: serde_json::json!({"query": "x"}),
            },
            StreamEvent::ToolOutputAvailable {
                tool_call_id: "call_1".into(),
                output: serde_json::json!({"results": []}),
            },
            StreamEvent::TextEnd { id: t1.clone() },
            StreamEvent::FinishStep,
            StreamEvent::StartStep,
            StreamEvent::TextStart { id: t2.clone() },
            StreamEvent::TextDelta { id: t2.clone(), delta: " world".into() },
            StreamEvent::ToolInputStart { tool_call_id: "call_2".into(), tool_name: "create_page".into() },
            StreamEvent::ToolInputAvailable {
                tool_call_id: "call_2".into(),
                tool_name: "create_page".into(),
                input: serde_json::json!({"title": "t"}),
            },
            StreamEvent::ToolOutputError {
                tool_call_id: "call_2".into(),
                error_text: "the page could not be written".into(),
            },
            StreamEvent::NarrativeDocumentReady { page_id: "page_fixture".into() },
            StreamEvent::TextEnd { id: t2.clone() },
            StreamEvent::FinishStep,
            StreamEvent::Finish { finish_reason: "stop".into() },
        ]
    }

    fn stopped_turn() -> Vec<StreamEvent> {
        let id = "msg_fixture".to_string();
        let t1 = format!("{id}:t1");
        vec![
            StreamEvent::Start { message_id: id.clone() },
            StreamEvent::StartStep,
            StreamEvent::TextStart { id: t1.clone() },
            StreamEvent::TextDelta { id: t1.clone(), delta: "Partial".into() },
            StreamEvent::TextEnd { id: t1.clone() },
            StreamEvent::Abort { reason: Some("stopped".into()) },
        ]
    }

    fn check(name: &str, events: Vec<StreamEvent>) {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../apps/web/src/lib/ai/fixtures")
            .join(name);
        let want: String = events.iter().map(|e| serialize_event(e) + "\n").collect();
        if std::env::var("UPDATE_FIXTURES").is_ok() {
            std::fs::write(&path, &want).expect("write fixture");
            return;
        }
        let have = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: {e} (run with UPDATE_FIXTURES=1 to write it)", path.display()));
        assert_eq!(
            have, want,
            "{} is stale: the box's event shapes changed. Regenerate with UPDATE_FIXTURES=1 and run the vitest.",
            path.display()
        );
    }

    #[test]
    fn the_canonical_turn_fixture_is_current() {
        check("box-ui-stream.jsonl", canonical_turn());
    }

    #[test]
    fn the_stopped_turn_fixture_is_current() {
        check("box-ui-stream-abort.jsonl", stopped_turn());
    }
}

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

use crate::agent::{AgentConfig, AgentLoop};
use crate::api::chat_mode::ChatMode;
use crate::api::chat_usage::{record_chat_usage, UsageData};
use crate::api::chats::{append_message, ChatMessage};
use crate::api::turn_recorder::{TurnRecorder, TurnUsage};
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

/// One registered turn: its token, and whether the cap stopped it.
#[derive(Clone)]
struct Registration {
    token: CancellationToken,
    /// Set when the unattended cap cancelled this turn, so the row and the
    /// notice can say the box stopped it rather than telling the person they
    /// stopped a reply they never touched.
    unattended: bool,
}

/// Shared state for tracking active chat requests that can be cancelled
#[derive(Clone, Default)]
pub struct ChatCancellationState {
    /// Map of chat_id -> the registration for the turn running now
    tokens: Arc<std::sync::RwLock<std::collections::HashMap<String, Registration>>>,
}

impl ChatCancellationState {
    pub fn new() -> Self {
        Self {
            tokens: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Register a new chat request and get its cancellation token.
    ///
    /// A chat can have two turns in flight — `LiveTurns` says so explicitly —
    /// so the token that lands here belongs to whichever turn started last,
    /// and everything below is written to survive that. Nothing may act on an
    /// entry it did not put there.
    pub fn register(&self, chat_id: &str) -> CancellationToken {
        let token = CancellationToken::new();
        // Recover from poisoned lock - the data is still valid
        let mut guard = self.tokens.write().unwrap_or_else(|e| e.into_inner());
        guard.insert(chat_id.to_string(), Registration { token: token.clone(), unattended: false });
        token
    }

    /// Cancel an active chat request — the person's Stop button.
    pub fn cancel(&self, chat_id: &str) -> bool {
        // Recover from poisoned lock - the data is still valid
        let guard = self.tokens.read().unwrap_or_else(|e| e.into_inner());
        if let Some(reg) = guard.get(chat_id) {
            reg.token.cancel();
            true
        } else {
            false
        }
    }

    /// Cancel a turn nobody is watching, and say that is what happened.
    ///
    /// `token` is the caller's OWN token: an old turn hitting the unattended
    /// cap used to cancel by chat id, which meant cancelling whatever was
    /// registered — usually the reply the person had come back and asked for.
    pub fn cancel_unattended(&self, chat_id: &str, token: &CancellationToken) {
        let mut guard = self.tokens.write().unwrap_or_else(|e| e.into_inner());
        match guard.get_mut(chat_id) {
            Some(reg) if reg.token.eq(token) => {
                reg.unattended = true;
                reg.token.cancel();
            }
            // Someone else's turn holds the slot now; ours is stale. Cancel our
            // own token so our loop still stops spending.
            _ => token.cancel(),
        }
    }

    /// Was this chat's current turn stopped by the cap rather than by a person?
    /// Whether the turn holding this chat's slot has been told to stop. It
    /// may still be running out a tool call; for "is a turn in progress"
    /// purposes it is over.
    pub fn is_cancelled(&self, chat_id: &str) -> bool {
        let guard = self.tokens.read().unwrap_or_else(|e| e.into_inner());
        guard.get(chat_id).is_some_and(|reg| reg.token.is_cancelled())
    }

    pub fn was_unattended(&self, chat_id: &str, token: &CancellationToken) -> bool {
        let guard = self.tokens.read().unwrap_or_else(|e| e.into_inner());
        guard.get(chat_id).is_some_and(|reg| reg.token.eq(token) && reg.unattended)
    }

    /// Remove a chat request (called when stream completes).
    ///
    /// Only if it is still ours. A turn that finished used to remove by chat
    /// id, so a turn started in the window between its last write and here had
    /// its token deleted — and the Stop button then reported "no active
    /// request" while a reply was visibly streaming.
    pub fn remove(&self, chat_id: &str, token: &CancellationToken) {
        // Recover from poisoned lock - the data is still valid
        let mut guard = self.tokens.write().unwrap_or_else(|e| e.into_inner());
        if guard.get(chat_id).is_some_and(|reg| reg.token.eq(token)) {
            guard.remove(chat_id);
        }
    }

    /// Check if a chat has an active request
    #[allow(dead_code)]
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
    /// A markdown page's text, from the client's live document. A block
    /// page's is not read from here: the server reads its tree.
    pub content: Option<String>,
}

/// The open page as the prompt shows it, resolved by the server
/// (`resolve_active_page`).
#[derive(Debug, Clone)]
enum ActivePage {
    /// A markdown page, with the text the client sent.
    Markdown { id: String, title: String, content: Option<String> },
    /// A block page, read by the server, with the base its next edit names,
    /// and whether what it shows holds a suggestion its owner has not decided.
    Tree { id: String, title: String, view: TreeView, suggestions: bool },
    /// A block page the server could not read this turn: the model reads it
    /// with get_page_content.
    Unreadable { id: String, title: String },
}

/// How a block page is shown: its HTML with block ids, or, when that is
/// longer than the prompt holds, its markdown export with each top-level
/// block's id in a comment above it, whole blocks up to what the prompt
/// holds. `next` is the block a read of the rest starts after, when the
/// page goes on past what is shown.
#[derive(Debug, Clone)]
enum TreeView {
    Html { html: String, base: String },
    Long { markdown: String, base: String, next: Option<String> },
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
    /// The Project (room) this chat lives in. Stored on the chat and inlined into
    /// the system prompt as a salience lens (name + memo + member URLs).
    #[serde(rename = "projectId", alias = "notebookId", default)]
    pub project_id: Option<String>,
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
    /// Agent mode controlling tool availability (agent, chat, research)
    #[serde(rename = "agentMode", default = "default_agent_mode")]
    pub agent_mode: String,
    /// Retrieval scope for project chats: "open" (default — whole graph,
    /// project items up-weighted) or "scoped" (grounded — items only).
    /// Ignored without a project_id.
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
    /// Local mode only: whether the small model reasons before answering.
    /// Every other mode's thinking is the mode's own (`ChatMode::limits`).
    #[serde(default)]
    pub think: bool,
}

/// A ghost chat's history, as the client sent it. The box stores nothing for
/// a temporary chat, so the wire is the only transcript. Text parts become
/// content; tool parts ride along in `parts` for the converter downstream.
fn ghost_history(messages: &[UIMessage]) -> Vec<ChatMessage> {
    messages
        .iter()
        .filter(|m| m.role == "user" || m.role == "assistant")
        .map(|m| {
            ChatMessage {
                id: m.id.clone(),
                role: m.role.clone(),
                content: m.text(),
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

fn default_chat_mode() -> String {
    "open".to_string()
}

fn default_agent() -> String {
    "auto".to_string()
}

fn default_agent_mode() -> String {
    "chat".to_string()
}

/// UI Message format (AI SDK v6)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UIMessage {
    pub id: Option<String>,
    pub role: String,
    #[serde(default, with = "wire_parts")]
    pub parts: Option<Vec<UIPart>>,
    // Legacy format support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

impl UIMessage {
    /// What the person said, as it is stored: the legacy `content` if the
    /// client sent one, else the text parts joined by newlines.
    pub fn text(&self) -> String {
        self.content.clone().unwrap_or_else(|| {
            self.parts
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
        })
    }
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
    /// sent as `tool-invocation` renders as nothing at all. `parts_to_jsonb`
    /// and `parts_from_jsonb` are the translation, and they are the ONLY way
    /// parts should be written or read — for the column, and for every API
    /// field through `wire_parts`. The derive alone serializes this variant
    /// as `tool-invocation`, which is what reloaded chats were served. A single hardcoded
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
        /// The id of the LAST message this summary covers.
        ///
        /// The count above is a position frozen at compaction time, and the
        /// list it indexes keeps changing — regenerate deletes rows, and so
        /// does the getting-started room's own sweep — so a count used as a
        /// position silently drops one verbatim message per row deleted below
        /// it. An id still points at the same message afterwards. Optional so
        /// a checkpoint written before this reads; the count is the fallback.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        last_message_id: Option<String>,
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

/// `parts` as the client speaks it, for any serde field that crosses the API.
///
/// `#[serde(with = "wire_parts")]` on an `Option<Vec<UIPart>>`: tool parts go
/// out as `tool-<toolName>` and come in from either spelling, through the same
/// two functions the column uses.
pub mod wire_parts {
    use super::{parts_from_jsonb, parts_to_jsonb, UIPart};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(
        parts: &Option<Vec<UIPart>>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        parts.as_deref().map(parts_to_jsonb).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<Option<Vec<UIPart>>, D::Error> {
        let raw = Option::<serde_json::Value>::deserialize(d)?;
        Ok(raw.and_then(|v| parts_from_jsonb(v, "request")))
    }
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

    // A local turn's own measurements, for the line under the reply.
    #[serde(rename = "local-stats")]
    LocalStats(crate::local_model::Stats),

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

impl From<crate::tools::SubagentUpdate> for StreamEvent {
    fn from(update: crate::tools::SubagentUpdate) -> Self {
        StreamEvent::SubagentStatus {
            dispatch_id: update.dispatch_id,
            subagent_id: update.id as u32,
            title: update.title,
            model: update.model,
            status: update.status.as_str().to_string(),
            tokens: update.tokens,
        }
    }
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
            ..
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
/// Custom events (checkpoint, narrative document, subagent status) are wrapped
/// in the AI SDK v6 data-* format
pub(crate) fn serialize_event(event: &StreamEvent) -> String {
    match event {
        // Checkpoint persists in the message parts, so it is not transient.
        StreamEvent::Checkpoint { id, version, messages_summarized, summary, timestamp } => data_event(
            "data-checkpoint",
            Some(id.clone()),
            CheckpointData {
                version: *version,
                messages_summarized: *messages_summarized,
                summary: summary.clone(),
                timestamp: timestamp.clone(),
            },
            false,
        ),
        // Transient: a reload reconstructs the document from its page, and
        // replaying the auto-open on old chats would be wrong.
        StreamEvent::NarrativeDocumentReady { page_id } => data_event(
            "data-narrative-document",
            None,
            NarrativeDocumentData { page_id: page_id.clone() },
            true,
        ),
        // Transient: the live panel only.
        StreamEvent::SubagentStatus { dispatch_id, subagent_id, title, model, status, tokens } => data_event(
            "data-subagent",
            None,
            SubagentStatusData {
                dispatch_id: *dispatch_id,
                subagent_id: *subagent_id,
                title: title.clone(),
                model: model.clone(),
                status: status.clone(),
                tokens: *tokens,
            },
            true,
        ),
        // Transient: a local chat is never stored, so there is nothing to
        // reload it into.
        StreamEvent::LocalStats(stats) => data_event("data-local-stats", None, stats, true),
        // All other events use standard serde serialization
        _ => serde_json::to_string(event).unwrap_or_else(|e| {
            tracing::error!("Failed to serialize stream event: {}", e);
            r#"{"type":"error","errorText":"Internal serialization error"}"#.to_string()
        }),
    }
}

/// One custom event in the AI SDK v6 `data-*` wrapper.
fn data_event<T: Serialize>(event_type: &str, id: Option<String>, data: T, transient: bool) -> String {
    let wrapper = DataEvent { event_type: event_type.to_string(), id, data, transient };
    serde_json::to_string(&wrapper).unwrap_or_else(|e| {
        tracing::error!("Failed to serialize {event_type} event: {e}");
        r#"{"type":"error","errorText":"Serialization error"}"#.to_string()
    })
}

/// Maximum characters for page content in system prompt
/// ~10K chars ≈ 2.5K tokens, leaving room for rest of context
const MAX_PAGE_CONTENT_CHARS: usize = 10_000;

/// Above the tag guide, beside an open block page in either view: the HTML
/// an edit writes, which a full read would have carried. The open page
/// stands for that read (the model edits a page shown as HTML with no read
/// first), so without it a guessed callout was written as a plain quote.
const OPEN_PAGE_TAGS: &str = "The tags edit_page takes:";

/// Beside an open block page: what an edit's html is, as a full read says.
const OPEN_PAGE_HTML: &str = "html is whole blocks; leave data-id off blocks you add.";

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
/// Truncation happens at a PARAGRAPH boundary: a cut mid-word feeds the model a
/// sentence that stops in the middle and invites it to complete the thought.
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
        "SELECT kind, rule FROM wiki_rules WHERE active ORDER BY created_at, id",
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
/// Assembles the block registry in `build_system_prompt_blocks`: base
/// (identity + character + style notes + narrative identity + tools + mode) → precedence →
/// memory → circumstances → coverage → active_project → active_context →
/// rules; the cache split falls before the first per-turn block..
/// Loads user name, assistant name, style notes, and narrative identity from profiles.
async fn build_system_prompt(
    pool: &PgPool,
    chat_id: Option<&str>,
    active_page: Option<&ActivePage>,
    timezone: Option<&str>,
    mode: &ChatMode,
    project_id: Option<&str>,
) -> SystemPrompt {
    use crate::api::assistant_profile::get_assistant_name;
    use crate::api::profile::get_display_name;

    // Load personalization from profiles (with fallbacks)
    let assistant_name = get_assistant_name(pool).await.unwrap_or_else(|_| "Ari".to_string());
    let user_name = get_display_name(pool).await.unwrap_or_else(|_| "there".to_string());

    // The narrative interview is a different room entirely: no tools, no
    // persona, no data context, no narrative-identity injection (the document
    // this conversation exists to create). Its prompt stands alone.
    if matches!(mode, ChatMode::Interview) {
        // The person's reply count is the one fact about progress the box can
        // vouch for; the prompt reads it as a floor on what can be covered.
        let their_replies = crate::api::narrative_draft::their_reply_count(pool)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(error = %e, "interview reply count unavailable; prompt says 0");
                0
            });
        // Whole-prompt stable: it changes only when the reply count does.
        return SystemPrompt::whole(
            "interview",
            crate::agent::prompt::build_interview_prompt(&assistant_name, &user_name, their_replies),
        );
    }

    // Getting started: its own prompt plus the derived state, regenerated per
    // turn so the model never holds a step done that the rows say is open.
    if matches!(mode, ChatMode::GettingStarted) {
        let block = match crate::api::getting_started::compute(pool).await {
            Ok(s) => s.render_prompt_block(),
            Err(e) => {
                tracing::warn!(error = %e, "getting-started state unavailable for the prompt");
                "<getting_started>\n(state unavailable this turn; say so if asked, never guess)\n</getting_started>".to_string()
            }
        };
        // Regenerated per turn, but its BYTES change only when a step does, so
        // it is stable for caching: it busts once when the state moves on.
        return SystemPrompt::whole(
            "getting_started",
            crate::agent::prompt::build_getting_started_prompt(&assistant_name, &user_name, &block),
        );
    }

    let (stable, tail, blocks) = build_system_prompt_blocks(
        pool,
        chat_id,
        active_page,
        timezone,
        mode,
        project_id,
        &assistant_name,
        &user_name,
    )
    .await;
    SystemPrompt { stable, tail, blocks }
}

/// The system prompt a turn is sent, split at the cache breakpoint: `stable`
/// carries the marker, `tail` is the per-turn rest. `blocks` are the sections
/// both were concatenated from, in order, which the Context panel lists.
pub(crate) struct SystemPrompt {
    pub stable: String,
    pub tail: String,
    pub blocks: Vec<crate::agent::prompt_blocks::RenderedBlock>,
}

impl SystemPrompt {
    /// A room whose prompt is one piece (the interview, getting started).
    fn whole(tag: &'static str, text: String) -> Self {
        let blocks = vec![crate::agent::prompt_blocks::RenderedBlock { tag, text: text.clone(), in_tail: false }];
        Self { stable: text, tail: String::new(), blocks }
    }
}

/// The answer contract of a scoped (grounded) chat: retrieval is hard-filtered
/// to the project's items (ScopeMode::Exclusive in ToolContext), and this
/// line says so. Only meaningful inside a project.
const SCOPED_CHAT_LINE: &str = "\n\nSCOPED CHAT: this conversation is grounded in the current project's \
     materials only. Retrieval is restricted to them. Answer ONLY from what \
     retrieval returns, citing each load-bearing claim with its ref link. If \
     the materials don't cover the question, say so plainly — do not answer \
     from general knowledge.";

/// What a turn's first model call carries besides its tools: the system
/// prompt and the conversation as it replays. The send and the Context
/// panel's preview both build it here, so the preview is the request rather
/// than a second guess at it.
pub(crate) struct TurnRequest {
    pub system: SystemPrompt,
    pub messages: Vec<serde_json::Value>,
}

async fn assemble_turn_request(
    pool: &PgPool,
    chat_id: &str,
    mode: &ChatMode,
    active_page: Option<&ActivePage>,
    timezone: Option<&str>,
    scoped: bool,
    history: &History,
) -> TurnRequest {
    let mut system =
        build_system_prompt(pool, Some(chat_id), active_page, timezone, mode, history.project_id.as_deref()).await;
    if scoped && history.project_id.is_some() {
        system.stable.push_str(SCOPED_CHAT_LINE);
        let at = system.blocks.iter().position(|b| b.in_tail).unwrap_or(system.blocks.len());
        system.blocks.insert(
            at,
            crate::agent::prompt_blocks::RenderedBlock { tag: "scoped", text: SCOPED_CHAT_LINE.to_string(), in_tail: false },
        );
    }
    let messages = build_context_for_llm(
        &history.messages,
        history.conversation_summary.as_deref(),
        history.summary_up_to_index as usize,
        Some(&system.stable),
        Some(&system.tail),
        // The model's "now" is the newest message's send time; the system
        // prompt carries no clock (see `prompt_blocks::held_per_chat`).
        Some(timezone.unwrap_or("UTC")),
    );
    TurnRequest { system, messages }
}

/// What the Context panel asks with: the inputs the next send will carry,
/// read by the client from the same getters its send reads.
#[derive(Debug, Deserialize)]
pub struct NextTurnRequest {
    #[serde(rename = "agentMode", default = "default_agent_mode")]
    pub agent_mode: String,
    #[serde(rename = "chatMode", default = "default_chat_mode")]
    pub chat_mode: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(rename = "activePage", default)]
    pub active_page: Option<ActivePageContext>,
}

/// The next message's request before the message itself, as the Context
/// panel lists it, and what the chat's latest calls read from cache.
#[derive(Debug, Serialize)]
pub struct NextTurnPreview {
    pub model: String,
    pub mode: String,
    /// The system prompt's sections in order. `cached` is the side of the
    /// cache breakpoint the section is on.
    pub sections: Vec<PreviewPart>,
    pub tools: Vec<PreviewPart>,
    /// The conversation as it replays, oldest first.
    pub messages: Vec<PreviewPart>,
    pub recent_calls: Vec<crate::agent::cache_watch::CallReading>,
}

/// One piece of the request: a prompt section, a tool, or a message.
/// `tokens` is the usual four-characters-a-token estimate.
#[derive(Debug, Serialize)]
pub struct PreviewPart {
    pub name: String,
    pub tokens: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached: Option<bool>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub text: String,
}

/// POST /api/chats/:id/context. Built by the send's own code
/// (`assemble_turn_request`), so what it shows is what goes. It writes
/// nothing, though it may take the chat's held present, which the next send
/// would have taken anyway.
pub async fn next_turn_preview(
    pool: &PgPool,
    yjs: &YjsState,
    chat_id: &str,
    request: NextTurnRequest,
) -> Result<NextTurnPreview, Response> {
    let mode = ChatMode::resolve(pool, chat_id, &request.agent_mode).await;
    if matches!(mode, ChatMode::Local) {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ChatError {
                error: "Nothing goes to a provider".to_string(),
                details: Some("This chat runs on your server's own model.".to_string()),
            }),
        )
            .into_response());
    }
    let model = match crate::api::model_choice::resolve_turn_model(pool, request.model.as_deref(), &mode).await {
        Ok(m) => m,
        Err(crate::error::Error::InvalidInput(detail)) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ChatError { error: "Invalid model".to_string(), details: Some(detail) }),
            )
                .into_response());
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to resolve the model for a context preview");
            return Err(internal("Failed to resolve model"));
        }
    };
    let history = load_saved_history(pool, chat_id).await?;
    let active_page = match request.active_page.as_ref() {
        Some(ctx) => resolve_active_page(pool, yjs, ctx).await,
        None => None,
    };
    let TurnRequest { system, messages } = assemble_turn_request(
        pool,
        chat_id,
        &mode,
        active_page.as_ref(),
        request.timezone.as_deref(),
        request.chat_mode == "scoped",
        &history,
    )
    .await;

    let part = |name: &str, cached: Option<bool>, text: String| PreviewPart {
        name: name.to_string(),
        tokens: crate::api::token_estimation::estimate_tokens(&text),
        cached,
        text,
    };
    let mut sections: Vec<PreviewPart> =
        system.blocks.iter().map(|b| part(b.tag, Some(!b.in_tail), b.text.clone())).collect();
    // A compaction summary rides at the end of the cached system message,
    // appended by `build_context_for_llm` rather than rendered as a block.
    let first_system = messages.first().map(preview_text).unwrap_or_default();
    let summary = first_system
        .strip_prefix(system.stable.as_str())
        .map(|rest| rest.strip_suffix(system.tail.as_str()).unwrap_or(rest).trim().to_string())
        .unwrap_or_default();
    if !summary.is_empty() {
        let at = system.blocks.iter().position(|b| b.in_tail).unwrap_or(system.blocks.len());
        sections.insert(at, part("compacted_conversation", Some(true), summary));
    }
    let tools = mode
        .tools()
        .iter()
        .map(|tool| {
            let name = tool["function"]["name"].as_str().unwrap_or("?").to_string();
            PreviewPart {
                tokens: crate::api::token_estimation::estimate_tokens(&tool.to_string()),
                name,
                cached: None,
                text: String::new(),
            }
        })
        .collect();
    let replayed = messages
        .iter()
        .skip_while(|m| m["role"] == "system")
        .map(|m| part(m["role"].as_str().unwrap_or("?"), None, preview_text(m)))
        .collect();

    Ok(NextTurnPreview {
        model,
        mode: mode.wire_name().to_string(),
        sections,
        tools,
        messages: replayed,
        recent_calls: crate::agent::cache_watch::recent_calls(chat_id),
    })
}

/// A request message as text: its text blocks, a placeholder for each
/// attachment (the bytes are no use on screen), then its tool calls.
fn preview_text(message: &serde_json::Value) -> String {
    let mut out = match &message["content"] {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| match b["type"].as_str() {
                Some("text") => b["text"].as_str().map(str::to_string),
                Some("image_url") => Some("[image]".to_string()),
                Some("file") => Some(format!("[file: {}]", b["file"]["filename"].as_str().unwrap_or("file"))),
                Some("input_audio") => Some("[audio]".to_string()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    };
    for call in message["tool_calls"].as_array().into_iter().flatten() {
        out.push_str(&format!(
            "\n→ {}({})",
            call["function"]["name"].as_str().unwrap_or("?"),
            call["function"]["arguments"].as_str().unwrap_or("")
        ));
    }
    out.trim().to_string()
}

/// The registry: every prompt section as a named block, rendered in list
/// order by `prompt_blocks::assemble`. This is the current order verbatim —
/// the formula's reorder (rules last, quantized clock, cache breakpoint) is
/// a deliberate later slice, and it happens by editing THIS list.
#[allow(clippy::too_many_arguments)]
async fn build_system_prompt_blocks(
    pool: &PgPool,
    chat_id: Option<&str>,
    active_page: Option<&ActivePage>,
    timezone: Option<&str>,
    mode: &ChatMode,
    project_id: Option<&str>,
    assistant_name: &str,
    user_name: &str,
) -> (String, String, Vec<crate::agent::prompt_blocks::RenderedBlock>) {
    use crate::agent::prompt::build_personalized_prompt;
    use crate::agent::prompt_blocks::{assemble, Author, Block, BlockMeta, Cadence, Mood};

    let blocks: Vec<Block<'_>> = vec![
        // The fused head: base identity + persona + narrative identity + tool
        // guidance + mode, still one string from build_personalized_prompt.
        // Splits into <character>/<narrative_identity>/<tools> in the reorder
        // slice.
        Block {
            meta: BlockMeta { tag: "base", author: Author::System, mood: Mood::Declarative, rung: 40, cadence: Cadence::Slow },
            body: Box::pin(async move {
                // The owner's style notes, beneath the character. A failed read
                // is logged and the turn goes on without them: the character
                // alone is the product's voice, and a chat must not fail over
                // a paragraph of preferences.
                let style_notes = match crate::api::assistant_profile::get_assistant_profile(pool).await {
                    Ok(profile) => profile.style_notes,
                    Err(e) => {
                        tracing::warn!(error = %e, "style notes unavailable; prompt carries the character alone");
                        None
                    }
                };
                let narrative_identity = build_narrative_identity(pool).await;
                Some(build_personalized_prompt(
                    assistant_name,
                    user_name,
                    style_notes.as_deref(),
                    mode,
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
        // them. Three lanes per agents/build/narrative-identity.md: facts of their
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
        // The computed present — as-of time, place, today's spine, calendar,
        // recent people (with entity ids), live threads, last night's sleep,
        // narrated recent days, connected sources. Deterministic, SQL-only,
        // budgeted by fixed caps; every line derives from one quarter-hour
        // instant. Held for the chat (`held_per_chat`): it moves with every
        // ingest, and a system message that changes costs the whole cache.
        Block {
            meta: BlockMeta { tag: "circumstances", author: Author::Computed, mood: Mood::Declarative, rung: 30, cadence: Cadence::Session },
            body: Box::pin(async move {
                crate::agent::prompt_blocks::held_per_chat(chat_id, "circumstances", timezone, |now| async move {
                    use chrono::DurationRound;
                    let floored = now.duration_trunc(chrono::TimeDelta::minutes(15)).unwrap_or(now);
                    crate::api::circumstances::build_circumstances(pool, timezone, floored).await
                })
                .await
            }),
        },
        // What the record holds, per table, as a date range — the fact that
        // lets the model tell "the record is silent" from "there was
        // nothing". Day-floored, and held for the chat like the present.
        Block {
            meta: BlockMeta { tag: "coverage", author: Author::Computed, mood: Mood::Declarative, rung: 30, cadence: Cadence::Session },
            body: Box::pin(async move {
                crate::agent::prompt_blocks::held_per_chat(chat_id, "coverage", timezone, |now| async move {
                    let today = match timezone.and_then(|t| t.parse::<chrono_tz::Tz>().ok()) {
                        Some(tz) => now.with_timezone(&tz).date_naive(),
                        None => now.date_naive(),
                    };
                    crate::api::coverage::build_coverage(pool, today).await
                })
                .await
            }),
        },
        // The active Project (room) as a salience lens: its name, catch-up
        // memo, and member URLs. This is the room the chat lives in.
        Block {
            meta: BlockMeta { tag: "active_project", author: Author::Ui, mood: Mood::Declarative, rung: 45, cadence: Cadence::Session },
            body: Box::pin(async move {
                match project_id {
                    Some(id) => build_project_context(pool, id).await,
                    None => None,
                }
            }),
        },
        // The skill this chat is running, if any — its file's body, in the
        // TAIL: PerTurn cadence puts it (and everything after) outside the
        // cached prefix, so the prefix is the same whatever the chat does
        // and switching skills rewrites a few KB, not the cache.
        Block {
            meta: BlockMeta { tag: "skill", author: Author::System, mood: Mood::Imperative, rung: 60, cadence: Cadence::PerTurn },
            body: Box::pin(async move {
                match mode {
                    ChatMode::Skill(s) => Some(format!("\n\n<skill name=\"{}\">\n{}\n</skill>", s.name, s.body)),
                    _ => None,
                }
            }),
        },
        // The open page's live content: a markdown page's text as the
        // client sent it, a block page's tree as the server read it.
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

/// The open page, as the prompt shows it. A markdown page keeps the text
/// the client sent: an older box reads only that, so the app still sends it.
/// A block page is read here, from the live tree, whatever the client sent,
/// and the blocks it lists, and only those, are kept as a read base, so the
/// model can edit them from the prompt with no read of its own, and a block
/// past what the prompt holds is read before it is replaced. A block listed
/// by id alone, too long to show, is in the base as it stands. An unchanged
/// page keeps the same base each turn.
async fn resolve_active_page(
    pool: &PgPool,
    yjs: &YjsState,
    ctx: &ActivePageContext,
) -> Option<ActivePage> {
    let id = ctx.page_id.clone()?;
    let title = ctx.page_title.clone().unwrap_or_else(|| "Untitled".to_string());
    let format = match crate::api::pages::page_format(pool, &id).await {
        Ok(format) => format,
        // A page whose format the server can't read is shown as the
        // client sent it: its text, or the line that has the model read it.
        Err(e) => {
            tracing::warn!(page = %id, error = %e, "open page's format unknown; using the client's text");
            crate::api::pages::PageFormat::Markdown
        }
    };
    if format == crate::api::pages::PageFormat::Markdown {
        return Some(ActivePage::Markdown { id, title, content: ctx.content.clone() });
    }
    let tree = match yjs.read_tree(&id).await {
        Ok(tree) => tree,
        Err(e) => {
            tracing::warn!(page = %id, error = %e, "open block page unreadable for the prompt");
            return Some(ActivePage::Unreadable { id, title });
        }
    };
    let html = virtues_document::to_html(&tree, true);
    let (seen, long) = if html.chars().count() <= MAX_PAGE_CONTENT_CHARS {
        (crate::api::page_reads::ReadBase::of(tree), None)
    } else {
        let window = crate::api::page_reads::markdown_window(
            &tree,
            0,
            MAX_PAGE_CONTENT_CHARS,
            |s| s.chars().count(),
            crate::tools::fits_one_read,
        );
        (window.read_base(), Some((window.markdown, window.next)))
    };
    let shown = &seen.tree;
    let base = match crate::api::page_reads::keep_read_base(pool, &id, &seen).await {
        Ok(base) => base,
        Err(e) => {
            tracing::warn!(page = %id, error = %e, "open block page's base not kept; the model reads it instead");
            return Some(ActivePage::Unreadable { id, title });
        }
    };
    let suggestions = crate::tools::holds_proposal(shown);
    let view = match long {
        None => TreeView::Html { html, base },
        Some((markdown, next)) => TreeView::Long { markdown, base, next },
    };
    Some(ActivePage::Tree { id, title, view, suggestions })
}

/// The sentence about the owner's undecided suggestions, after a space,
/// when the page as shown holds one.
fn suggestions_line(suggestions: bool) -> String {
    if suggestions {
        format!(" {}", crate::tools::SUGGESTIONS_NOTE)
    } else {
        String::new()
    }
}

/// The open page's section of the prompt, as a turn with `ctx` open sends
/// it: for the page edit eval, which measures the chat's own page guidance.
#[cfg(test)]
pub(crate) async fn open_page_section(
    pool: &PgPool,
    yjs: &YjsState,
    ctx: &ActivePageContext,
) -> Option<String> {
    let page = resolve_active_page(pool, yjs, ctx).await?;
    build_active_page_block(Some(&page))
}

/// Render the open-page section, if a page is open.
fn build_active_page_block(active_page: Option<&ActivePage>) -> Option<String> {
    Some(match active_page? {
        ActivePage::Markdown { id, title, content: Some(content) } => {
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
                title, id, content_display, truncation_note
            )
        }
        ActivePage::Markdown { id, title, content: None } | ActivePage::Unreadable { id, title } => format!(
            "\n\n<active_context>\nThe user has \"{}\" (id: {}) open for editing. Use get_page_content to read it first, then edit_page to make changes.\n</active_context>",
            title, id
        ),
        ActivePage::Tree { id, title, view: TreeView::Html { html, base }, suggestions } => format!(
            "\n\n<active_context>\nThe user has \"{title}\" (id: {id}) open. It is a block page.\n\n<current_content base=\"{base}\">\n{html}\n</current_content>\n\nEdit it with edit_page: base \"{base}\" and ops on these data-ids. Your edits appear in their editor as you make them. {OPEN_PAGE_HTML} {OPEN_PAGE_TAGS}\n{}{}\n</active_context>",
            virtues_document::tag_guide(),
            suggestions_line(*suggestions)
        ),
        ActivePage::Tree { id, title, view: TreeView::Long { markdown, base, next }, suggestions } => {
            let what = match next {
                Some(next) => format!(
                    "This is the start of the page as markdown, each block's data-id in the comment above it. \
                     Read on with get_page_content, after \"{next}\" and base \"{base}\"; a block not shown here \
                     is read before it is replaced."
                ),
                None => "This is the page as markdown, each block's data-id in the comment above it.".to_string(),
            };
            format!(
                "\n\n<active_context>\nThe user has \"{title}\" (id: {id}) open. It is a block page.\n\n<current_content base=\"{base}\" view=\"markdown\">\n{}\n</current_content>\n\nEdit it with edit_page: base \"{base}\" and ops on these data-ids, in HTML. Your edits appear in their editor as you make them. {what} Markdown does not say what a block's HTML is (a mention reads as a link, a callout as a quote): read a block with get_page_content, ids and base \"{base}\" before you replace it. {OPEN_PAGE_HTML} {OPEN_PAGE_TAGS}\n{}{}\n</active_context>",
                markdown.trim_end(),
                virtues_document::tag_guide(),
                suggestions_line(*suggestions)
            )
        }
    })
}

/// Test-only view of the assembled prompt, so audits in other modules can
/// assert on what the model is actually sent rather than re-deriving it.
#[cfg(test)]
pub(crate) async fn build_system_prompt_for_audit(pool: &PgPool) -> String {
    let prompt = build_system_prompt(pool, None, None, Some("America/Chicago"), &ChatMode::Chat, None).await;
    format!("{}{}", prompt.stable, prompt.tail)
}

/// Maximum member URLs to inline for a Project before truncating.
const MAX_PROJECT_ITEMS_INLINED: usize = 100;

/// Build a context block for the active Project (room) the chat lives in.
/// Returns None if the Project can't be loaded.
async fn build_project_context(pool: &PgPool, project_id: &str) -> Option<String> {
    let detail = match crate::api::projects::get_project(pool, project_id).await {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("[chat] failed to load active project {}: {}", project_id, e);
            return None;
        }
    };

    let mut out = String::new();
    out.push_str(&format!(
        "\n\n<active_project name=\"{}\">",
        escape_attr(&detail.project.name),
    ));

    if let Some(instr) = detail.project.instructions.as_deref() {
        if !instr.is_empty() {
            out.push_str(&format!(
                "\n  <instructions>{}</instructions>",
                escape_attr(instr)
            ));
        }
    }

    let total = detail.items.len();
    let shown: Vec<_> = detail
        .items
        .iter()
        .take(MAX_PROJECT_ITEMS_INLINED)
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
    if total > MAX_PROJECT_ITEMS_INLINED {
        out.push_str(&format!(
            "\n  <!-- {} more members not shown -->",
            total - MAX_PROJECT_ITEMS_INLINED
        ));
    }

    out.push_str("\n</active_project>");

    let preamble = "\n\n<active_project_preamble>\nThis chat lives in the Project (room) below — a collection the user returns to (an undertaking, a pet, a hobby, a goal, or a topic). Treat its members as high-salience: they are the user's actively curated focus for this room. <instructions>, if present, are standing directions for how you should behave in this project — follow them. Members are also boosted in semantic search while this project is active.\n\nThe member list below IS the project's contents — it is already complete (up to the cap noted at its end). When the user refers to something \"in this project,\" match it here first; do not go looking for the project's contents with other tools.\n\nEach member carries what it is: `title`, `kind`, and `role`. `role=\"library\"` grounds this chat; `role=\"manuscript\"` is the user's own draft, deliberately excluded from retrieval - never cite it back at them as a source; `role=\"pin\"` is navigation only. Files also carry `text`: `indexed` means its contents are searchable, `pending` means extraction has not finished yet, and `none` means no text was extracted — searching for its contents will find nothing, so say so plainly rather than reporting an empty search as an absence of the thing.\n</active_project_preamble>";

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
    State(ghost_permissions): State<crate::api::chat_permissions::GhostPermissions>,
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
        State(ghost_permissions),
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
    State(ghost_permissions): State<crate::api::chat_permissions::GhostPermissions>,
    _user: AuthUser,
    Json(request): Json<ChatRequest>,
) -> Response {
    // Where the time before the first model call goes, one log line per turn
    // ("turn prepared"). Everything here is paid before the first token.
    let started = std::time::Instant::now();
    let ms = |since: std::time::Instant| since.elapsed().as_millis() as u64;
    // The same moment, kept with the reply: its "Worked for" counts from here.
    let turn_started_at = crate::types::Timestamp::now();

    // The turn's mode, decided once. Some chats are a mode by id (the
    // interview, the getting-started room), whatever the client sent.
    let mode = ChatMode::resolve(&pool, &request.chat_id, &request.agent_mode).await;

    // A local turn has no cloud step, so it leaves before the first one: no
    // model choice and no readiness check for a gateway it never calls.
    if matches!(mode, ChatMode::Local) {
        return crate::api::local_chat::run(pool, request, live_turns, cancel_state).await;
    }

    if let Some(refusal) = reject_if_unready(&pool, &request, &live_turns, &cancel_state).await {
        return refusal;
    }
    let t_ready = ms(started);

    // Which model answers. One door — the box decides, from the mode and the
    // owner's pin; the request's `model` is a per-turn override and nothing
    // else. See `model_choice` for why the interview refuses that override,
    // and why an empty string means "I did not choose".
    let model = match crate::api::model_choice::resolve_turn_model(
        &pool,
        request.model.as_deref(),
        &mode,
    )
    .await
    {
        Ok(m) => m,
        // Name the id we rejected: a list of the allowed ids without the
        // offending one is no help to whoever reads the error.
        Err(crate::error::Error::InvalidInput(detail)) => {
            tracing::warn!(
                requested = ?request.model,
                agent_mode = %mode.wire_name(),
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
            return internal("Failed to resolve model");
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

    let chat_id_str = request.chat_id.clone();
    // A ghost chat has no row. Everything below that reads or writes
    // app_chats / app_chat_messages / app_chat_usage branches on this.
    let temporary = request.temporary;

    store_user_turn(&pool, &request).await;

    // A ghost chat has no usage row to read and no summary to write, so it
    // never compacts.
    let t_stored = ms(started);
    let checkpoint_event = if temporary {
        None
    } else {
        compact_if_critical(&pool, &chat_id_str, &model).await
    };
    let t_compacted = ms(started);

    let history = match load_history(&pool, &request).await {
        Ok(history) => history,
        Err(response) => return response,
    };

    // The system prompt, split at the cache breakpoint (`stable` gets the
    // marker; the per-turn tail stays outside it), and the conversation as it
    // replays. The Context panel's preview builds the same thing.
    let t_history = ms(started);
    let prompt_started = std::time::Instant::now();
    let active_page = match request.active_page.as_ref() {
        Some(ctx) => resolve_active_page(&pool, &yjs_state, ctx).await,
        None => None,
    };
    let TurnRequest { system, messages: api_messages } = assemble_turn_request(
        &pool,
        &chat_id_str,
        &mode,
        active_page.as_ref(),
        request.timezone.as_deref(),
        request.chat_mode == "scoped",
        &history,
    )
    .await;
    let prompt_ms = ms(prompt_started);
    let prompt_tokens = crate::api::token_estimation::estimate_tokens(&system.stable)
        + crate::api::token_estimation::estimate_tokens(&system.tail);

    // Tell the gauge how big the prompt actually is. It cannot rebuild it, so
    // without this the biggest single part of the request is invisible to both
    // the context ring the user reads and the threshold that fires compaction.
    crate::api::token_estimation::record_system_prompt_tokens(prompt_tokens);

    // The turn is driven by its own task and outlives this request, so a tab
    // switched or a phone locked does not drop the loop or the assistant row
    // (VIR-323). This response is one watcher on the turn;
    // `GET /api/chat/{id}/stream` is another.
    tracing::info!(
        chat_id = %chat_id_str,
        mode = mode.wire_name(),
        ready_ms = t_ready,
        stored_ms = t_stored - t_ready,
        compaction_ms = t_compacted - t_stored,
        history_ms = t_history - t_compacted,
        prompt_ms,
        prompt_tokens,
        total_ms = ms(started),
        "turn prepared"
    );

    let turn = live_turns.start(&chat_id_str);
    // Registered here, not inside the stream, so the driver holds the same
    // token and can tell its own turn from whichever one holds the slot.
    let turn_token = cancel_state.register(&chat_id_str);
    let agent_stream = create_agent_stream(
        pool,
        yjs_state,
        cancel_state.clone(),
        request,
        mode,
        model,
        history.project_id,
        api_messages,
        msg_id,
        checkpoint_event,
        turn_token.clone(),
        ghost_permissions,
        turn_started_at,
    );
    spawn_turn_driver(agent_stream, turn.clone(), turn_token, live_turns, cancel_state, chat_id_str);

    ui_stream_response(live_turn::watch(turn))
}

/// A 500 whose cause the caller has already logged. The raw error does not
/// travel to the browser: `sqlx::Error`'s Display carries the Postgres
/// message and often the column or constraint name.
fn internal(error: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ChatError { error: error.to_string(), details: None }),
    )
        .into_response()
}

/// The refusals that come before any work: no model to answer with, or a
/// turn already running in this chat. Both are 409s the client keys on.
async fn reject_if_unready(
    pool: &PgPool,
    request: &ChatRequest,
    live_turns: &LiveTurns,
    cancel_state: &ChatCancellationState,
) -> Option<Response> {
    // No model, no turn — said in one sentence here, not as whatever the
    // gateway call fails with. Any room: the composer is inert while locked,
    // but no client can be trusted to be.
    if !crate::api::getting_started::ai_connected(pool).await {
        return Some(
            (
                StatusCode::CONFLICT,
                Json(ChatError {
                    error: "AI is not connected".to_string(),
                    details: Some(
                        "Your server has nothing to answer with yet. Connect a Virtues subscription, or point it at your own models, in Settings under Billing."
                            .to_string(),
                    ),
                }),
            )
                .into_response(),
        );
    }

    // One turn per chat at a time. A turn outlives its request (live_turn.rs),
    // so a client that lost the wire and asks again would otherwise start a
    // SECOND turn while the first is still running: `LiveTurns::start`
    // replaces the entry and lets the old task finish, both write an
    // assistant row, and the transcript shows two replies to one question,
    // both billed. The client's Try again rejoins first and only asks anew
    // when nothing is live; this is the boundary for a client that does not,
    // or a double tap. 409, with the name the client keys on.
    // A turn Stop has cancelled still holds the entry until its tool returns
    // (the loop checks the token between deltas, not inside a tool), so
    // without the second clause a Stop during a long tool would answer every
    // send with this 409 for up to the tool's timeout.
    if !request.temporary
        && live_turns.get(&request.chat_id).is_some()
        && !cancel_state.is_cancelled(&request.chat_id)
    {
        return Some(
            (
                StatusCode::CONFLICT,
                Json(ChatError {
                    error: "turn_in_progress".to_string(),
                    details: Some("Your assistant is still writing a reply to this chat.".to_string()),
                }),
            )
                .into_response(),
        );
    }

    None
}

/// A new chat's title: the first user message's legacy `content`, else its
/// FIRST text part — not `UIMessage::text`, which joins them all — cut to
/// fifty characters. A message with no text titles the chat
/// "New conversation" rather than nothing.
fn chat_title(messages: &[UIMessage]) -> String {
    let raw_title = messages
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
}

/// Create the chat's row if it has none. True when this call created it.
///
/// ON CONFLICT DO NOTHING, so two requests racing to open the same chat both
/// succeed and exactly one of them reports the creation. A failed insert is
/// logged and reads as "not created"; the chat row read that follows is what
/// turns a missing row into an error.
/// Store the turn's user side: the chat's row on its first turn (bound to its
/// project), and the user's message. On regenerate the previous answer is
/// dropped instead and no message is added. A ghost chat stores nothing.
pub(crate) async fn store_user_turn(pool: &PgPool, request: &ChatRequest) {
    if !request.temporary && ensure_chat_row(pool, request).await {
        if let Err(e) =
            crate::api::projects::set_chat_project(pool, &request.chat_id, request.project_id.as_deref()).await
        {
            tracing::warn!("Failed to set chat project: {}", e);
        }
    }

    let regenerating = drop_previous_answer_if_regenerating(pool, request).await;

    // Save the last user message to the chat. Not on regenerate: there is no
    // new user turn, and a client that still sends the full history would
    // otherwise re-append the last one.
    let last_user_msg = request.messages.iter().rev().find(|m| m.role == "user");
    if let Some(last_user_msg) = last_user_msg.filter(|_| !regenerating) {
        let user_message = ChatMessage {
            // The client's id, so the row can be recognized again: by a
            // regenerate, and by a retried POST — `append_message` does
            // nothing on an id it has, so a send that reached the box and
            // lost the reply does not save the question twice.
            id: last_user_msg.id.clone().filter(|id| !id.trim().is_empty()),
            role: "user".to_string(),
            content: last_user_msg.text(),
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

        if request.temporary {
            // Ghost: the message lives in the client's tab and nowhere else.
        } else if let Err(e) = append_message(pool, request.chat_id.clone(), user_message).await {
            tracing::error!("Failed to save user message: {}", e);
        }
    }
}

async fn ensure_chat_row(pool: &PgPool, request: &ChatRequest) -> bool {
    match sqlx::query("INSERT INTO app_chats (id, title, message_count) VALUES ($1, $2, 0) ON CONFLICT (id) DO NOTHING")
        .bind(&request.chat_id)
        .bind(chat_title(&request.messages))
        .execute(pool)
        .await
    {
        Ok(result) => result.rows_affected() > 0,
        Err(e) => {
            tracing::error!("Failed to create chat: {}", e);
            false
        }
    }
}

/// Whether this request is a regenerate, and if it is, drop the box's copy of
/// the answer being replaced — the client has already dropped its own, and
/// the model must not answer with its previous reply in front of it.
///
/// ONLY if the user turn being answered again is on disk. The client offers
/// Try again on every error card, and some of those errors happened before
/// anything was written: 409 not connected, 400 invalid model, 413, a request
/// that never arrived. The message is then still only in the client, and
/// treating it as a regenerate would delete the previous good answer and
/// re-answer the question before it — on a new chat, an empty transcript. So
/// the client sends its last user message on regenerate too, and if it is not
/// the last user row here, this is a first send of it (false).
///
/// Matched by id: the user row is stored under the id the client gave it, so
/// a loaded transcript and a message sent this session both carry the row's
/// id. Not by text: a short repeated message — "ok", "thanks", "continue" —
/// would match the wrong turn, which is exactly the case this exists to stop.
///
/// The room's own lines (`gs:` subjects) are not "the previous answer" and
/// stay, or regenerate in the getting-started room deletes the step the room
/// has just narrated. A ghost chat has nothing on disk to drop.
async fn drop_previous_answer_if_regenerating(pool: &PgPool, request: &ChatRequest) -> bool {
    let regenerating = matches!(
        request.trigger.as_deref(),
        Some("regenerate-message") | Some("regenerate-assistant-message")
    );
    if !regenerating || request.temporary {
        return regenerating;
    }
    let chat_id = &request.chat_id;
    let last_user_msg = request.messages.iter().rev().find(|m| m.role == "user");
    let answered_turn_on_disk = match last_user_msg {
        // An older client sends nothing on regenerate; nothing to compare.
        None => true,
        Some(m) => {
            match sqlx::query_as::<_, (String, String)>(
                "SELECT id, content FROM app_chat_messages \
                 WHERE chat_id = $1 AND role = 'user' \
                 ORDER BY sequence_num DESC LIMIT 1",
            )
            .bind(chat_id)
            .fetch_optional(pool)
            .await
            {
                Ok(Some((id, content))) => match m.id.as_deref() {
                    Some(client_id) => client_id == id,
                    // A client that sends no id: the text is all there is.
                    None => content == m.text(),
                },
                Ok(None) => false,
                Err(e) => {
                    // Unknown. Deleting on a guess is the failure this
                    // exists to stop; answering the message as new is
                    // the harmless side.
                    tracing::error!(chat_id = %chat_id, error = %e, "regenerate: could not read the last user turn");
                    false
                }
            }
        }
    };
    if !answered_turn_on_disk {
        tracing::info!(chat_id = %chat_id, "regenerate asked for a turn that was never saved; answering it as new");
        return false;
    }
    if let Err(e) = sqlx::query(
        "DELETE FROM app_chat_messages \
         WHERE chat_id = $1 AND role = 'assistant' \
           AND COALESCE(subject, '') NOT LIKE 'gs:%' \
           AND sequence_num > COALESCE((SELECT MAX(sequence_num) FROM app_chat_messages \
                                        WHERE chat_id = $1 AND role = 'user'), 0)",
    )
    .bind(chat_id)
    .execute(pool)
    .await
    {
        tracing::error!(chat_id = %chat_id, error = %e, "regenerate: could not drop the previous answer");
    }
    true
}

/// Compact the chat if its context is critical, and return the checkpoint
/// event to announce it.
///
/// This runs before the context is built, not inside the stream: the turn
/// that triggers compaction is the turn whose context is too big, and a
/// summary written after `build_context_for_llm` has run does nothing for it.
/// The checkpoint event still reaches the client first thing in the stream.
async fn compact_if_critical(pool: &PgPool, chat_id: &str, model: &str) -> Option<StreamEvent> {
    let compaction_status =
        crate::api::chat_usage::check_compaction_needed(pool, chat_id.to_string(), model).await;
    if !matches!(compaction_status, Ok(ContextStatus::Critical)) {
        return None;
    }
    tracing::info!(chat_id = %chat_id, "context critical, compacting before the turn");
    let options = CompactionOptions {
        model_id: Some(model.to_string()),
        ..Default::default()
    };
    match compact_chat(pool, chat_id.to_string(), options).await {
        Ok(_) => get_latest_checkpoint(pool, chat_id).await,
        Err(e) => {
            // A chat that cannot compact still gets its answer; it is
            // just a large one.
            tracing::warn!(chat_id = %chat_id, error = %e, "auto-compaction failed; continuing with the full context");
            None
        }
    }
}

/// What the turn is answering from: the chat's compaction summary, its room,
/// and its transcript.
struct History {
    conversation_summary: Option<String>,
    summary_up_to_index: i64,
    project_id: Option<String>,
    messages: Vec<ChatMessage>,
}

/// Read the chat's row and transcript, or for a ghost chat take both from
/// the request. Err is the response to send.
async fn load_history(pool: &PgPool, request: &ChatRequest) -> Result<History, Response> {
    if request.temporary {
        // The box holds nothing for a ghost chat: the wire is the transcript
        // and the request is the room's binding.
        return Ok(History {
            conversation_summary: None,
            summary_up_to_index: 0,
            project_id: request.project_id.clone(),
            messages: ghost_history(&request.messages),
        });
    }
    load_saved_history(pool, &request.chat_id).await
}

/// A saved chat's row and transcript. Err is the response to send.
async fn load_saved_history(pool: &PgPool, chat_id: &str) -> Result<History, Response> {
    // The room is read from the persisted row (single source of truth) so the
    // active-project context always matches the binding, even if a stale
    // client sends a different per-message projectId; the create path has
    // already bound a new chat from request.project_id, so the row is current.
    //
    // `project_id` decodes as `Option<String>` on purpose: the column is
    // nullable (and the FK is ON DELETE SET NULL), so an unbound chat
    // legitimately reads NULL. A failed read is a 500, never a None: None
    // reads as "not in a project", so a swallowed error would silently unscope
    // a scoped chat — retrieval stops being hard-filtered and the answer
    // contract is dropped.
    let (conversation_summary, summary_up_to_index, project_id) =
        match sqlx::query_as::<_, (Option<String>, i64, Option<String>)>(
            "SELECT conversation_summary, summary_up_to_index, project_id FROM app_chats WHERE id = $1",
        )
        .bind(chat_id)
        .fetch_one(pool)
        .await
        {
            Ok(row) => row,
            Err(e) => {
                tracing::error!("Failed to load chat: {}", e);
                return Err(internal("Failed to load chat"));
            }
        };

    let messages = match crate::api::chats::load_messages(pool, chat_id).await {
        Ok(messages) => messages,
        Err(e) => {
            tracing::error!("Failed to load messages for chat {}: {}", chat_id, e);
            return Err(internal("Failed to load messages"));
        }
    };

    Ok(History { conversation_summary, summary_up_to_index, project_id, messages })
}

/// Drive the turn's stream in its own task, pushing each line to the live
/// turn's watchers, and end the turn for all of them however the stream ends.
pub(crate) fn spawn_turn_driver(
    agent_stream: Pin<Box<dyn Stream<Item = String> + Send>>,
    turn: Arc<live_turn::LiveTurn>,
    turn_token: CancellationToken,
    live_turns: LiveTurns,
    cancel_state: ChatCancellationState,
    chat_id: String,
) {
    tokio::spawn(async move {
        // The drive loop runs inside a catch, because `finish` is what ends
        // the turn for every watcher: a panic that skipped it would leave
        // `done` false, `watch` blocked on a Notify that never fires, and the
        // SSE keep-alive holding the socket open — a thinking mark that spins
        // until the person gives up, and every later rejoin attached to the
        // same dead turn. A panic in here has to end the turn like any other
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
                        cancel_state.cancel_unattended(&chat_id, &turn_token);
                    }
                }
            }
        };
        if std::panic::AssertUnwindSafe(drive).catch_unwind().await.is_err() {
            tracing::error!(chat_id = %chat_id, "the turn's driver panicked; ending the turn");
            turn.push(serialize_event(&StreamEvent::Error {
                error_text: "Your server failed while writing this reply.".to_string(),
            }));
            turn.push("[DONE]".to_string());
        }
        // After the stream's own tail (row written, usage recorded), so
        // a watcher that sees the end can reload and find the row.
        live_turns.finish(&chat_id, &turn);
    });
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
pub(crate) fn ui_stream_response<S>(stream: S) -> Response
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
    // Resolved once by the handler (`ChatMode::resolve`); never re-read off
    // `request.agent_mode`.
    mode: ChatMode,
    // Already resolved by `model_choice::resolve_turn_model` — passed in
    // rather than re-read off the request so the stream cannot disagree with
    // what the handler decided, or fall back to a default of its own.
    model: String,
    // The chat's project as the handler resolved it — from the row, not the
    // request — so the tools search the same project the prompt describes.
    project_id: Option<String>,
    api_messages: Vec<serde_json::Value>,
    msg_id: String,
    // Emitted first when this turn compacted the chat before it started.
    checkpoint_event: Option<StreamEvent>,
    cancel_token: CancellationToken,
    ghost_permissions: crate::api::chat_permissions::GhostPermissions,
    // When the box received the request, before history and compaction.
    started_at: crate::types::Timestamp,
) -> Pin<Box<dyn Stream<Item = String> + Send>> {
    let chat_id = request.chat_id.clone();
    // Copied out for the stream block below, which reads `request` for a
    // few fields and must not persist a ghost turn either.
    let temporary = request.temporary;
    let agent_id = request.agent_id.clone();

    Box::pin(async_stream::stream! {
        // The token was registered by the handler; this is the same one the
        // driver watches.
        let cancel_token = cancel_token;

        // The turn's ceilings: steps, dollars, wall-clock, effort, tool
        // timeout. See `ChatMode::limits`.
        let limits = mode.limits();

        // Create AgentLoop with YjsState for real-time page editing
        let agent = AgentLoop::new_with_yjs(pool.clone(), yjs_state)
        .with_config(AgentConfig {
            max_steps: limits.max_steps,
            tool_timeout: limits.tool_timeout,
            parallel_tools: true,
            thinking: limits.thinking,
        })
        .with_budget(limits.budget);

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
            project_id,
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
            temporary,
            ghost_permissions: Some(ghost_permissions.clone()),
            sudo: mode.is_sudo(),
            timezone: request.timezone.clone(),
        };

        let tools = mode.tools();

        // The message opens, then its first step; the recorder opens and
        // closes the parts inside each step, and the steps after this one.
        yield (serialize_event(&StreamEvent::Start { message_id: msg_id.clone() }));
        // Compaction already happened, in the handler, before the context was
        // built from the compacted history. All that is left is to say so —
        // AFTER `start`, because a data part that arrives before the message
        // exists is pushed onto it as an invisible one and shown again as the
        // synthetic checkpoint the client inserts from `onData`.
        if let Some(event) = &checkpoint_event {
            yield (serialize_event(event));
        }
        yield (serialize_event(&StreamEvent::StartStep));

        let mut recorder = TurnRecorder::new(msg_id.clone());

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
                yield (serialize_event(&StreamEvent::from(update)));
            }
            maybe_event = agent_stream.next() => {
                let Some(event) = maybe_event else { break };
                for ev in recorder.on_event(event) {
                    yield (serialize_event(&ev));
                }
            }
          }
        }

        // Drain any subagent updates buffered after the agent loop ended.
        while let Ok(update) = subagent_rx.try_recv() {
            yield (serialize_event(&StreamEvent::from(update)));
        }

        let was_cancelled = cancel_token.is_cancelled();
        // The cap and the Stop button are the same cancellation; only the
        // registration knows which. Read before `remove` below.
        let was_unattended = was_cancelled && cancel_state.was_unattended(&chat_id, &cancel_token);
        for ev in recorder.close(was_cancelled, was_unattended) {
            yield (serialize_event(&ev));
        }

        // Send [DONE] marker
        yield ("[DONE]".to_string());

        let usage = recorder.usage();
        let subject = recorder.subject(was_cancelled, was_unattended);
        let message = recorder.into_message(&model, agent_id, subject);
        persist_turn(&pool, &chat_id, &model, mode.wire_name(), temporary, message, usage, Some(started_at)).await;

        // Clean up cancellation token when stream ends
        cancel_state.remove(&chat_id, &cancel_token);
    })
}

/// Keep what the turn produced: its row, if it has one, and its usage and
/// cost, which it always has. A ghost chat writes neither the row nor the
/// per-chat usage; its cost still goes to `app_ai_calls`, with no content
/// and no chat id.
async fn persist_turn(
    pool: &PgPool,
    chat_id: &str,
    model: &str,
    agent_mode: &str,
    temporary: bool,
    message: Option<ChatMessage>,
    usage: TurnUsage,
    started_at: Option<crate::types::Timestamp>,
) {
    if let Some(assistant_message) = message {
        // WHAT THE REPLY LINKED TO, CHECKED AFTER THE FACT.
        //
        // The wiki REFUSES a bad link (`wiki_editor::check_links`), because
        // an article is a stored artifact and nothing has been shown yet.
        // A chat reply has already streamed past the person by the time
        // anything could object, so the same finding is reported instead —
        // it cannot be enforced here without rewriting text somebody has
        // read, and a link that silently vanishes on reload is its own kind
        // of lie. This is also how we find out whether the prompt's citation
        // guidance is enough to stop invented links.
        match crate::api::wiki_editor::dead_links(pool, &assistant_message.content).await {
            Ok(problems) if !problems.is_empty() => {
                for problem in &problems {
                    tracing::warn!(chat_id = %chat_id, model = %model, "the reply {problem}");
                }
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(chat_id = %chat_id, error = %e, "could not check the reply's links"),
        }

        if temporary {
            // Ghost: nothing written. The client keeps the turn in its tab.
        } else {
            match append_message(pool, chat_id.to_string(), assistant_message).await {
                Ok(msg_id) => {
                    // The turn's span. Its own statement so the shared insert
                    // paths stay as they are; a failure here costs the
                    // duration on reload, nothing else.
                    if let Some(started_at) = started_at {
                        if let Err(e) = sqlx::query(
                            "UPDATE app_chat_messages SET started_at = $1, ended_at = created_at WHERE id = $2",
                        )
                        .bind(started_at)
                        .bind(&msg_id)
                        .execute(pool)
                        .await
                        {
                            tracing::warn!(chat_id = %chat_id, error = %e, "could not record the turn's span");
                        }
                    }
                }
                Err(e) => tracing::error!("Failed to save assistant message: {}", e),
            }
        }
    }

    // Usage is recorded whether or not there is a message worth keeping:
    // that is not the same question as whether the wallet was debited. A
    // model that spends its whole output budget thinking and returns no
    // content is billed in full.
    //
    // `cost_micros` is the gateway's authoritative figure — the same one
    // recorded in app_ai_calls below, and the one the wallet was actually
    // debited for. No estimating. Cache writes are 0 on every turn today
    // because nothing reports one, not because this assumes it.
    let usage_data = UsageData {
        input_tokens: usage.input_tokens as i64,
        output_tokens: usage.output_tokens as i64,
        reasoning_tokens: usage.reasoning_tokens as i64,
        cache_read_tokens: usage.cache_read_tokens as i64,
        cache_write_tokens: usage.cache_write_tokens as i64,
        cost_micros: Some(usage.cost_micros),
    };

    // Read once and shared with the cost log below, so the two cannot
    // disagree about which route paid for this turn.
    //
    // The CHECKED form, because this value decides whether the box prices
    // the turn from its own catalog: a swallowed error would read as "not
    // BYO" and write real dollars into estimated_cost_usd for tokens the
    // owner had already paid their own provider for.
    //
    // Unknown is treated as BYO, i.e. do not price it ourselves. Of the two
    // ways to be wrong, under-reporting a wallet turn is a gap in a display
    // figure that app_ai_calls still records properly, while over-reporting
    // invents a charge the person never incurred. A fabricated number is
    // worse than a missing one.
    let byo = match crate::api::settings_byo::byo_is_active_checked(pool).await {
        Ok(active) => active,
        Err(e) => {
            tracing::warn!(
                chat_id = %chat_id,
                error = %e,
                "BYO status unreadable; not pricing this turn rather than guessing a cost"
            );
            true
        }
    };
    if temporary {
        // Ghost: app_chat_usage keys on a chat row that does not exist.
        // app_ai_calls below still records cost and counts, no content.
    } else if let Err(e) = record_chat_usage(pool, chat_id.to_string(), model, usage_data, byo).await {
        tracing::warn!(
            chat_id = %chat_id,
            error = %e,
            "Failed to record chat usage"
        );
    }

    // Box-local per-call cost log (authoritative gateway cost) for the
    // Usage/Telemetry tabs. Best-effort — never break the response.
    if let Err(e) = crate::api::ai_calls::record_ai_call(
        pool,
        &crate::api::ai_calls::AiCall {
            // Real feature bucket: the mode's wire name (every mode shares
            // this handler), so spend attributes correctly.
            feature: agent_mode.to_string(),
            model: model.to_string(),
            prompt_tokens: usage.input_tokens as i64,
            completion_tokens: usage.output_tokens as i64,
            reasoning_tokens: usage.reasoning_tokens as i64,
            cost_micros: usage.cost_micros,
            // `stream()` diverts to the user's endpoint when BYO is set, and
            // no upstream but our own gateway sends a cost trailer — so the
            // cost is 0-as-unknown there, and the Usage tab must show tokens
            // instead of "$0.00".
            route: if byo {
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

/// Generate a random ID for messages
pub(crate) fn generate_id() -> String {
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
    /// A chat's system prompt holds still while the record moves under it.
    /// A page edited between turns reorders "Live threads", and grok serves
    /// nothing from cache past a changed byte, so the present is held for the
    /// chat. Without a chat nothing is held and the edit shows at once.
    #[sqlx::test]
    async fn a_chats_system_prompt_holds_still_while_the_record_moves(pool: PgPool) {
        let chat = Some("chat_prompt_holds_still");
        let zone = Some("America/Chicago");
        let (first, _, _) =
            build_system_prompt_blocks(&pool, chat, None, zone, &ChatMode::Chat, None, "Ari", "Adam").await;
        sqlx::query("INSERT INTO app_pages (id, title) VALUES ('page_moved', 'Edited between turns')")
            .execute(&pool)
            .await
            .unwrap();

        let (next, _, _) =
            build_system_prompt_blocks(&pool, chat, None, zone, &ChatMode::Chat, None, "Ari", "Adam").await;
        assert_eq!(first, next, "the next turn's system prompt is byte-identical");

        let (unheld, _, _) =
            build_system_prompt_blocks(&pool, None, None, zone, &ChatMode::Chat, None, "Ari", "Adam").await;
        assert!(unheld.contains("Edited between turns"), "the edit is in the record:\n{unheld}");
        assert!(!unheld.contains("Now:"), "the clock is not in the prompt");
    }

    /// The Context panel's preview is the send's own request: the system
    /// sections in order with the breakpoint marked, the tools, and the
    /// conversation as it replays, stamped, with its tool calls.
    #[sqlx::test]
    async fn the_context_preview_lists_what_the_next_send_carries(pool: PgPool) {
        sqlx::query("INSERT INTO app_chats (id, title, message_count) VALUES ('chat_preview', 't', 0)")
            .execute(&pool)
            .await
            .unwrap();
        let message = |id: &str, role: &str, parts: Vec<UIPart>| crate::api::chats::ChatMessage {
            id: Some(id.into()),
            role: role.into(),
            content: String::new(),
            timestamp: crate::types::Timestamp::parse("2026-10-09T23:52:00Z").unwrap(),
            model: None,
            provider: None,
            agent_id: None,
            tool_calls: None,
            reasoning: None,
            intent: None,
            subject: None,
            reasoning_details: None,
            parts: Some(parts),
        };
        let call = UIPart::ToolInvocation {
            tool_call_id: "c1".into(),
            tool_name: "sql_query".into(),
            input: serde_json::json!({ "sql": "SELECT 1" }),
            state: "output-available".into(),
            output: Some(serde_json::json!({ "rows": [] })),
            error_text: None,
        };
        for m in [
            message("m1", "user", vec![UIPart::Text { text: "How did I sleep?".into() }]),
            message("m2", "assistant", vec![UIPart::Text { text: "Looking.".into() }, call]),
        ] {
            crate::api::chats::append_message(&pool, "chat_preview".into(), m).await.unwrap();
        }

        let request = NextTurnRequest {
            agent_mode: "chat".into(),
            chat_mode: "open".into(),
            model: None,
            timezone: Some("America/Chicago".into()),
            active_page: None,
        };
        let preview = next_turn_preview(&pool, &YjsState::new(pool.clone()), "chat_preview", request)
            .await
            .unwrap_or_else(|_| panic!("a preview"));

        let tags: Vec<&str> = preview.sections.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(tags.first(), Some(&"base"), "{tags:?}");
        assert!(
            preview.sections.iter().all(|s| s.cached == Some(true)),
            "no page open and no rules: every section is in the cached prefix: {tags:?}"
        );
        assert!(preview.tools.iter().any(|t| t.name == "sql_query" && t.tokens > 100));
        assert_eq!(preview.mode, "chat");

        let roles: Vec<&str> = preview.messages.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(roles, ["user", "assistant", "tool"]);
        assert!(preview.messages[0].text.starts_with("[Sent Friday, October 9, 2026 at 6:52 PM CDT]"));
        assert!(preview.messages[1].text.contains(r#"→ sql_query({"sql":"SELECT 1"})"#), "{}", preview.messages[1].text);
    }

    /// A skill's body is in the tail, never the cached prefix: the prefix
    /// must be the same whatever the chat is doing, or switching skills
    /// rewrites the cache. And ordinary chat carries no skill block at all.
    #[sqlx::test]
    async fn a_skill_rides_in_the_tail_and_chat_carries_none(pool: PgPool) {
        let (stable, volatile, rendered) = build_system_prompt_blocks(
            &pool, None, None, Some("America/Chicago"), &crate::api::chat_mode::tests::fixture_skill(), None, "Ari", "Adam",
        )
        .await;
        assert!(rendered.iter().any(|r| r.tag == "skill"), "a skill renders its skill block");
        assert!(!stable.contains("<skill name=\"fixture\">"), "the skill body must not be in the cached prefix");
        assert!(volatile.contains("<skill name=\"fixture\">"), "the skill body is in the tail");
        assert!(volatile.contains("<fixture>"), "the body is the file's body");
        assert!(!stable.contains("<page_tools>"), "a skill without page tools gets no page guidance");

        let (stable, volatile, rendered) = build_system_prompt_blocks(
            &pool, None, None, Some("America/Chicago"), &ChatMode::Chat, None, "Ari", "Adam",
        )
        .await;
        assert!(!rendered.iter().any(|r| r.tag == "skill"));
        assert!(!format!("{stable}{volatile}").contains("<skill"));
        assert!(stable.contains("<mode>chat</mode>"));
    }

    #[sqlx::test]
    async fn prompt_blocks_render_in_registry_order(pool: PgPool) {
        sqlx::query("INSERT INTO wiki_rules (id, kind, rule) VALUES ('rule_t1', 'avoid', 'x')")
            .execute(&pool)
            .await
            .unwrap();
        let (stable, volatile, rendered) = build_system_prompt_blocks(
            &pool, None, None, Some("America/Chicago"), &ChatMode::Chat, None, "Ari",
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
        let expected = ["base", "precedence", "new_user", "memory", "circumstances", "coverage", "active_project", "skill", "active_context", "rules"];
        let mut last = 0usize;
        for t in &tags {
            let pos = expected.iter().position(|e| e == t).expect("unknown block tag");
            assert!(pos >= last, "block {t} rendered out of registry order: {tags:?}");
            last = pos;
        }
        // And the assembly is still the whole prompt across the cache split.
        let prompt = format!("{stable}{volatile}");
        assert!(prompt.contains("<rules>") && prompt.contains("<circumstances>"));
        // The split is the point of the split: the per-turn tail (and the rules
        // that deliberately sit behind it) must be on the uncached side, or a
        // bound page's live text invalidates the prefix on every keystroke.
        assert!(
            volatile.contains("<rules>"),
            "rules sit behind the per-turn tail, so they are outside the cached prefix"
        );
        assert!(
            stable.contains("<circumstances>"),
            "the quantized clock belongs to the stable prefix"
        );
        // As the audit above notes, <precedence> legitimately NAMES <rules>,
        // and <precedence> is stable. What must not be in the cached half is
        // the rules block's own BODY.
        assert!(
            !stable.contains("has marked some things as rules"),
            "nothing behind the per-turn boundary may land in the cached prefix"
        );
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

/// Renders the real active-project block against a dev database, so the text
/// the model actually receives is inspected rather than assumed. Ignored by
/// default — CI has no box database.
///   cargo test -p virtues --lib api::chat::live_project -- --ignored --nocapture
#[cfg(test)]
mod live_project {
    use sqlx::PgPool;

    #[tokio::test]
    #[ignore]
    async fn the_block_names_every_member() {
        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://virtues:virtues@localhost:5432/virtues".to_string());
        let pool = PgPool::connect(&url).await.expect("dev database");

        let id: Option<String> = sqlx::query_scalar(
            "SELECT project_id FROM app_project_items
             GROUP BY project_id ORDER BY count(*) DESC LIMIT 1",
        )
        .fetch_optional(&pool)
        .await
        .expect("query");
        let Some(id) = id else {
            println!("no projects with members; nothing to render");
            return;
        };

        let block = super::build_project_context(&pool, &id)
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

        let project: Option<String> = sqlx::query_scalar(
            "SELECT project_id FROM app_project_items
             GROUP BY project_id ORDER BY count(*) DESC LIMIT 1",
        )
        .fetch_optional(&pool)
        .await
        .expect("query");

        for (label, nb) in [("no project", None), ("in a project", project.as_deref())] {
            let p = super::build_system_prompt(
                &pool,
                None,
                None,
                Some("America/Chicago"),
                &super::ChatMode::Chat,
                nb,
            )
            .await;
            // Measured across the cache split: this reads the whole prompt.
            let p = format!("{}{}", p.stable, p.tail);

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
                "<active_project",
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

    /// The API speaks the column's spelling both ways. The derive alone sent
    /// every reloaded tool part as `tool-invocation`, and the client's
    /// per-tool cards (pages, applets, images) matched none of them.
    #[test]
    fn api_messages_carry_the_per_tool_type() {
        let incoming = serde_json::json!({
            "id": "m1", "role": "assistant",
            "parts": [{ "type": "tool-create_page", "toolCallId": "c0",
                        "input": {}, "state": "output-available", "output": {} }],
        });
        let msg: UIMessage = serde_json::from_value(incoming).unwrap();
        let parts = msg.parts.as_deref().unwrap();
        assert!(matches!(&parts[0], UIPart::ToolInvocation { tool_name, .. } if tool_name == "create_page"));

        let out = serde_json::to_value(&msg).unwrap();
        assert_eq!(out["parts"][0]["type"], "tool-create_page");
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
                error_text: "Your server couldn't write the page.".into(),
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

    /// The three data-wrapped events, byte for byte: the client matches on
    /// `data-*` types and field names, and on `transient` being absent for a
    /// checkpoint (it persists in the message) and `true` for the others.
    #[test]
    fn data_events_serialize_exactly() {
        assert_eq!(
            serialize_event(&StreamEvent::Checkpoint {
                id: "msg_cp".into(),
                version: 2,
                messages_summarized: 7,
                summary: "<s/>".into(),
                timestamp: "2026-09-28T00:00:00Z".into(),
            }),
            r#"{"type":"data-checkpoint","id":"msg_cp","data":{"version":2,"messagesSummarized":7,"summary":"<s/>","timestamp":"2026-09-28T00:00:00Z"}}"#
        );
        assert_eq!(
            serialize_event(&StreamEvent::NarrativeDocumentReady { page_id: "page_1".into() }),
            r#"{"type":"data-narrative-document","data":{"pageId":"page_1"},"transient":true}"#
        );
        assert_eq!(
            serialize_event(&StreamEvent::SubagentStatus {
                dispatch_id: 3,
                subagent_id: 1,
                title: "Prices".into(),
                model: "m".into(),
                status: "thinking".into(),
                tokens: 42,
            }),
            r#"{"type":"data-subagent","data":{"dispatchId":3,"subagentId":1,"title":"Prices","model":"m","status":"thinking","tokens":42},"transient":true}"#
        );
    }
}

/// `persist_turn`: what a finished turn leaves behind — its row, its per-chat
/// usage and its ai_call — for a normal turn, a temporary one, and an empty one.
#[cfg(test)]
mod persist_turn_tests {
    use super::*;

    /// A turn as the stream would record it: `text` said (if any), one
    /// step's usage reported.
    fn recorded_turn(text: &str) -> (Option<ChatMessage>, TurnUsage) {
        let mut r = TurnRecorder::new("msg_t".into());
        if !text.is_empty() {
            r.on_event(crate::agent::AgentEvent::TextDelta { content: text.into() });
        }
        r.on_event(crate::agent::AgentEvent::Usage {
            prompt_tokens: 1_200,
            completion_tokens: 80,
            total_tokens: Some(1_280),
            reasoning_tokens: Some(30),
            cache_read_tokens: Some(1_000),
            cache_write_tokens: None,
            cost_micros: Some(4_321),
        });
        let usage = r.usage();
        (r.into_message("anthropic/claude-x", "auto".into(), None), usage)
    }

    async fn count(pool: &PgPool, sql: &str, chat_id: &str) -> i64 {
        sqlx::query_scalar(sql).bind(chat_id).fetch_one(pool).await.unwrap()
    }

    const MESSAGES: &str = "SELECT count(*) FROM app_chat_messages WHERE chat_id = $1";
    const USAGE: &str = "SELECT count(*) FROM app_chat_usage WHERE chat_id = $1";
    const AI_CALLS: &str = "SELECT count(*) FROM app_ai_calls WHERE feature = $1";

    /// A normal turn writes its row, its per-chat usage and its ai_call, each
    /// carrying the recorder's figures.
    #[sqlx::test]
    async fn persist_turn_writes_the_row_the_usage_and_the_call(pool: PgPool) {
        sqlx::query("INSERT INTO app_chats (id, title, message_count) VALUES ('chat_p', 't', 0)")
            .execute(&pool)
            .await
            .unwrap();
        let began = crate::types::Timestamp::now();
        let (message, usage) = recorded_turn("Hello.");
        persist_turn(&pool, "chat_p", "anthropic/claude-x", "chat", false, message, usage, Some(began)).await;

        let (role, content, model): (String, String, Option<String>) =
            sqlx::query_as("SELECT role, content, model FROM app_chat_messages WHERE chat_id = 'chat_p'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!((role.as_str(), content.as_str(), model.as_deref()), ("assistant", "Hello.", Some("anthropic/claude-x")));
        let spanned: bool = sqlx::query_scalar(
            "SELECT started_at IS NOT NULL AND ended_at >= started_at FROM app_chat_messages WHERE chat_id = 'chat_p'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(spanned, "the row keeps the turn's span");
        let n: i64 = sqlx::query_scalar("SELECT message_count::bigint FROM app_chats WHERE id = 'chat_p'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n, 1, "the chat's count follows the row");

        let tokens: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT input_tokens::bigint, output_tokens::bigint, reasoning_tokens::bigint, cache_read_tokens::bigint
             FROM app_chat_usage WHERE chat_id = 'chat_p' AND model = 'anthropic/claude-x'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(tokens, (1_200, 80, 30, 1_000));

        let call: (String, i64, i64, i64, i64) = sqlx::query_as(
            "SELECT model, prompt_tokens::bigint, completion_tokens::bigint, reasoning_tokens::bigint, cost_micros::bigint
             FROM app_ai_calls WHERE feature = 'chat'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(call, ("anthropic/claude-x".to_string(), 1_200, 80, 30, 4_321));
    }

    /// A temporary (ghost) chat has no chat row: nothing goes to
    /// app_chat_messages or app_chat_usage, but the spend still reaches
    /// app_ai_calls.
    #[sqlx::test]
    async fn a_temporary_turn_writes_only_its_ai_call(pool: PgPool) {
        let (message, usage) = recorded_turn("Off the record.");
        assert!(message.is_some(), "the turn said something; only `temporary` keeps it out");
        persist_turn(&pool, "chat_ghost", "anthropic/claude-x", "sudo", true, message, usage, None).await;

        assert_eq!(count(&pool, MESSAGES, "chat_ghost").await, 0);
        assert_eq!(count(&pool, USAGE, "chat_ghost").await, 0);
        assert_eq!(count(&pool, "SELECT count(*) FROM app_chats WHERE id = $1", "chat_ghost").await, 0);
        let cost: i64 = sqlx::query_scalar("SELECT cost_micros::bigint FROM app_ai_calls WHERE feature = 'sudo'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(cost, 4_321);
    }

    /// Nothing said and no tool called: no row — but the tokens were spent,
    /// so usage and the ai_call are still written.
    #[sqlx::test]
    async fn an_empty_turn_writes_no_row_but_still_its_usage(pool: PgPool) {
        sqlx::query("INSERT INTO app_chats (id, title, message_count) VALUES ('chat_e', 't', 0)")
            .execute(&pool)
            .await
            .unwrap();
        let (message, usage) = recorded_turn("");
        assert!(message.is_none(), "no text and no tools is no message");
        persist_turn(&pool, "chat_e", "anthropic/claude-x", "deep_research", false, message, usage, None).await;

        assert_eq!(count(&pool, MESSAGES, "chat_e").await, 0);
        assert_eq!(count(&pool, USAGE, "chat_e").await, 1);
        assert_eq!(count(&pool, AI_CALLS, "deep_research").await, 1);
    }
}


#[cfg(test)]
mod active_page_tests {
    use super::*;
    use crate::api::page_reads;
    use crate::api::pages::{self, PageFormat};

    async fn page(pool: &PgPool, markdown: &str, format: PageFormat) -> String {
        pages::create_page_as(
            pool,
            pages::CreatePageRequest {
                title: "Trip".into(),
                content: markdown.into(),
                project_id: None,
                icon: None,
                icon_color: None,
                cover_url: None,
                tags: None,
                format: None,
            },
            format,
        )
        .await
        .unwrap()
        .page
        .id
    }

    fn open(id: &str, content: Option<&str>) -> ActivePageContext {
        ActivePageContext {
            page_id: Some(id.to_string()),
            page_title: Some("Trip".to_string()),
            content: content.map(str::to_string),
        }
    }

    /// A block page is read by the server: the client's text is not used,
    /// and the base the prompt names is kept, once for an unchanged page.
    #[sqlx::test]
    async fn a_block_page_is_shown_as_html_with_its_base(pool: PgPool) {
        let id = page(&pool, "## Plan\n\nLunch with [@Nick](/person/person_1).\n", PageFormat::Tree).await;
        let yjs = YjsState::new(pool.clone());
        let ctx = open(&id, Some("The client's own copy."));
        let resolved = resolve_active_page(&pool, &yjs, &ctx).await.unwrap();
        let block = build_active_page_block(Some(&resolved)).unwrap();

        let tree = yjs.read_tree(&id).await.unwrap();
        let base = page_reads::tree_hash(&tree);
        assert_eq!(
            block,
            format!(
                "\n\n<active_context>\nThe user has \"Trip\" (id: {id}) open. It is a block page.\n\n\
                 <current_content base=\"{base}\">\n{}\n</current_content>\n\n\
                 Edit it with edit_page: base \"{base}\" and ops on these data-ids. \
                 Your edits appear in their editor as you make them. {OPEN_PAGE_HTML} {OPEN_PAGE_TAGS}\n{}\n</active_context>",
                virtues_document::to_html(&tree, true),
                virtues_document::tag_guide()
            )
        );
        // The model edits this view with no read first: a callout it never
        // saw on the page is in the guide, not guessed as a quote.
        assert!(block.contains("<aside data-tone"), "{block}");
        assert!(block.contains("<virtues-mention to=\"/person/person_1\" label=\"Nick\"></virtues-mention>"));
        assert!(!block.contains("client's own copy"));
        assert_eq!(page_reads::read_base(&pool, &id, &base).await.unwrap(), Some(page_reads::ReadBase::of(tree)));

        resolve_active_page(&pool, &yjs, &ctx).await.unwrap();
        let kept: i64 =
            sqlx::query_scalar("SELECT count(*) FROM app_page_read_bases WHERE page_id = $1")
                .bind(&id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(kept, 1, "an unchanged page keeps one base");
    }

    /// A long page is shown as whole blocks, as many as the prompt holds,
    /// and the base the prompt names holds those blocks alone.
    #[sqlx::test]
    async fn a_long_block_page_is_cut_with_the_way_to_read_the_rest(pool: PgPool) {
        let markdown: String = (0..400)
            .map(|i| format!("Paragraph {i} of a long page about a trip to the coast.\n\n"))
            .collect();
        let id = page(&pool, &markdown, PageFormat::Tree).await;
        let yjs = YjsState::new(pool.clone());
        let resolved = resolve_active_page(&pool, &yjs, &open(&id, None)).await.unwrap();
        let block = build_active_page_block(Some(&resolved)).unwrap();

        let tree = yjs.read_tree(&id).await.unwrap();
        let k = tree
            .iter()
            .position(|n| !block.contains(&format!("<!-- {} -->", n.id().unwrap())))
            .unwrap();
        assert!(k > 0, "nothing shown");
        let ActivePage::Tree { view: TreeView::Long { base, .. }, .. } = &resolved else {
            panic!("a long view: {resolved:?}");
        };
        let base = base.clone();
        let kept = page_reads::read_base(&pool, &id, &base).await.unwrap().unwrap();
        assert_eq!(kept.tree, tree[..k].to_vec(), "the base is what was shown");
        // Shown as markdown only: a replace of any of them waits for a read
        // of its HTML.
        assert_eq!(
            kept.markdown_only,
            tree[..k].iter().map(|n| n.id().unwrap().to_string()).collect(),
        );
        let (first, last) = (tree[0].id().unwrap(), tree[k - 1].id().unwrap());
        assert!(block.contains(&format!(
            "<current_content base=\"{base}\" view=\"markdown\">\n<!-- {first} -->\nParagraph 0 of"
        )));
        // The last block shown is whole.
        assert!(block.contains(&format!(
            "<!-- {last} -->\nParagraph {} of a long page about a trip to the coast.\n</current_content>",
            k - 1
        )));
        assert!(block.contains(&format!(
            "This is the start of the page as markdown, each block's data-id in the comment above it. \
             Read on with get_page_content, after \"{last}\" and base \"{base}\"; a block not shown here \
             is read before it is replaced. \
             Markdown does not say what a block's HTML is (a mention reads as a link, a callout as a quote): \
             read a block with get_page_content, ids and base \"{base}\" before you replace it."
        )));
        // The HTML an edit writes, which no read of this view has carried.
        assert!(block.contains(&format!("{OPEN_PAGE_TAGS}\n{}", virtues_document::tag_guide())), "{block}");
        assert!(!block.contains("Paragraph 399"), "the page was not cut");
        assert!(block.chars().count() < MAX_PAGE_CONTENT_CHARS + 2_000);
    }

    /// A block too long for the prompt's view of the page, which a read
    /// shows whole, is said to be read, not to be beyond any read, and is
    /// not in the prompt's base: a replace of it unread is refused, not
    /// written over what the model never saw.
    #[sqlx::test]
    async fn a_block_too_long_for_the_prompt_is_read_before_it_is_replaced(pool: PgPool) {
        use crate::tools::{PageEditorTool, ToolContext};
        use serde_json::json;

        let lines: String = (0..320).map(|i| format!("  \"key{i}\": \"value {i} of the config\",\n")).collect();
        let markdown = format!("Intro.\n\n```json\n{{\n{lines}}}\n```\n\nOutro.\n");
        let id = page(&pool, &markdown, PageFormat::Tree).await;
        let yjs = YjsState::new(pool.clone());
        let tree = yjs.read_tree(&id).await.unwrap();
        let code = tree[1].id().unwrap().to_string();
        assert!(crate::tools::fits_one_read(&tree[1]), "a read shows the block whole");
        let resolved = resolve_active_page(&pool, &yjs, &open(&id, None)).await.unwrap();
        let ActivePage::Tree { view: TreeView::Long { base, markdown: shown, .. }, .. } = &resolved else {
            panic!("a long view: {resolved:?}");
        };
        assert!(
            shown.contains(&format!(
                "<!-- {code} -->\n<!-- too long to show here; read it with get_page_content, ids and this base -->"
            )),
            "{shown}"
        );
        assert!(!shown.contains("any read"), "{shown}");

        let tool = PageEditorTool::new(std::sync::Arc::new(pool.clone()), Some(yjs.clone()));
        let refused = tool
            .edit_page(
                json!({ "page_id": id, "base": base, "ops": [
                    { "op": "replace", "id": code, "html": "<pre><code>{}</code></pre>" }
                ] }),
                &ToolContext::default(),
            )
            .await
            .unwrap();
        assert_eq!(refused.data["applied"], false, "{}", refused.data);
        assert!(refused.data["error"]
            .as_str()
            .unwrap()
            .contains(&format!("the block `{code}` is not in the read that base names")));
        assert_eq!(yjs.read_tree(&id).await.unwrap()[1], tree[1]);
    }

    /// The model knows a block past what the prompt holds, from a read in an
    /// earlier turn, and the owner then types in it. A replace sent with the
    /// prompt's base is refused, not written over the owner's words, since
    /// the prompt never showed that block; a block it did show is edited
    /// from it.
    #[sqlx::test]
    async fn a_block_past_the_cut_is_read_before_it_is_replaced(pool: PgPool) {
        use crate::tools::{PageEditorTool, ToolContext};
        use serde_json::json;

        let markdown: String = (0..12)
            .map(|i| format!("{}\n\n", format!("Day {i} of the trip, the coast road. ").repeat(28)))
            .collect();
        let id = page(&pool, &markdown, PageFormat::Tree).await;
        let yjs = YjsState::new(pool.clone());
        let tree = yjs.read_tree(&id).await.unwrap();
        let resolved = resolve_active_page(&pool, &yjs, &open(&id, None)).await.unwrap();
        let ActivePage::Tree { view: TreeView::Long { base, markdown: shown, next }, .. } = &resolved else {
            panic!("a long view: {resolved:?}");
        };
        // The cut falls between blocks: the first block not shown is not
        // shown at all.
        let k = tree
            .iter()
            .position(|n| !shown.contains(n.id().unwrap()))
            .unwrap();
        assert!(k > 0 && k < tree.len(), "{k}");
        assert_eq!(next.as_deref(), tree[k - 1].id());
        assert!(shown.contains(&tree[k - 1].text_content()));
        assert!(!shown.contains(&format!("Day {k} of the trip")));

        let cut = tree[k].id().unwrap().to_string();
        yjs.edit_tree(
            &id,
            None,
            &[virtues_document::Op::Replace {
                id: cut.clone(),
                html: "<p>Budget: $800, confirmed with Nick.</p>".into(),
            }],
        )
        .await
        .unwrap()
        .after
        .unwrap();

        let tool = PageEditorTool::new(std::sync::Arc::new(pool.clone()), Some(yjs.clone()));
        let refused = tool
            .edit_page(
                json!({ "page_id": id, "base": base, "ops": [
                    { "op": "replace", "id": cut, "html": "<p>Budget: $500, hotels $200.</p>" }
                ] }),
                &ToolContext::default(),
            )
            .await
            .unwrap();
        assert_eq!(refused.data["applied"], false, "{}", refused.data);
        assert!(refused.data["error"]
            .as_str()
            .unwrap()
            .contains(&format!("the block `{cut}` is not in the read that base names")));
        let now = yjs.read_tree(&id).await.unwrap();
        assert_eq!(now[k].text_content(), "Budget: $800, confirmed with Nick.");

        // A block the prompt showed as markdown waits for its HTML, which
        // the refusal brings, with a base that has seen it.
        let first = tree[0].id().unwrap();
        let replace = |base: &str| {
            json!({ "page_id": id, "base": base, "ops": [
                { "op": "replace", "id": first, "html": "<p>Day 0, by train.</p>" }
            ] })
        };
        let refused = tool.edit_page(replace(base), &ToolContext::default()).await.unwrap();
        assert_eq!(refused.data["applied"], false, "{}", refused.data);
        assert!(refused.data["blocks"].as_str().unwrap().contains(&format!("data-id=\"{first}\"")));
        let base = refused.data["base"].as_str().unwrap().to_string();
        let applied = tool.edit_page(replace(&base), &ToolContext::default()).await.unwrap();
        assert_eq!(applied.data["applied"], true, "{}", applied.data);
        assert_eq!(yjs.read_tree(&id).await.unwrap()[0].text_content(), "Day 0, by train.");
    }

    /// A markdown page's block is today's, word for word, from the text the
    /// client sent.
    #[sqlx::test]
    async fn a_markdown_page_keeps_its_block_word_for_word(pool: PgPool) {
        let id = page(&pool, "Coffee.\n", PageFormat::Markdown).await;
        let yjs = YjsState::new(pool.clone());
        let resolved = resolve_active_page(&pool, &yjs, &open(&id, Some("Coffee and tea.\n"))).await.unwrap();
        assert_eq!(
            build_active_page_block(Some(&resolved)).unwrap(),
            format!(
                "\n\n<active_context>\nThe user has \"Trip\" (id: {id}) open for editing.\n\n\
                 <current_content>\nCoffee and tea.\n\n</current_content>\n\n\
                 Use the edit_page tool to make changes. The 'find' parameter locates text, 'replace' \
                 provides the new text. For a full rewrite, set find to empty string. Edits are applied \
                 immediately via real-time sync.\n</active_context>"
            )
        );

        let unsent = resolve_active_page(&pool, &yjs, &open(&id, None)).await.unwrap();
        assert_eq!(
            build_active_page_block(Some(&unsent)).unwrap(),
            format!(
                "\n\n<active_context>\nThe user has \"Trip\" (id: {id}) open for editing. \
                 Use get_page_content to read it first, then edit_page to make changes.\n</active_context>"
            )
        );
        assert!(build_active_page_block(None).is_none());
    }
}

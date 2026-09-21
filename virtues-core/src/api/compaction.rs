//! Conversation Compaction Module
//!
//! Implements hierarchical summarization for context management.
//! Compresses older messages into a rolling summary while keeping
//! recent exchanges verbatim.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::time::Duration;
use tokio::time::timeout;

use crate::api::chat::UIPart;
use crate::api::chats::ChatMessage;
use crate::api::token_estimation::{estimate_session_context, ContextStatus};
use crate::types::Timestamp;
use crate::error::Result;

// ============================================================================
// Constants
// ============================================================================

// Note: Summarization model comes from the Lite slot (assistant_profile::get_background_model)

/// Number of recent exchanges to keep verbatim (user + assistant pairs)
/// Lower value = more aggressive compaction, but less recent context preserved
const DEFAULT_KEEP_RECENT_EXCHANGES: usize = 4;

/// Maximum tokens for summary generation

/// Temperature for summary generation (lower = more deterministic)
const SUMMARY_TEMPERATURE: f32 = 0.3;

// ============================================================================
// Types
// ============================================================================

/// Result of a compaction operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionResult {
    pub success: bool,
    pub messages_summarized: i32,
    pub messages_kept_verbatim: i32,
    pub summary_tokens: i64,
    pub new_usage_percentage: f64,
    pub previous_usage_percentage: f64,
    pub summary_version: i32,
}

/// Options for compaction
#[derive(Debug, Clone, Deserialize)]
pub struct CompactionOptions {
    /// Number of recent exchanges to keep verbatim (default: 4)
    #[serde(default = "default_keep_recent")]
    pub keep_recent_exchanges: usize,
    /// Force compaction even if under threshold
    #[serde(default)]
    pub force: bool,
    /// Model ID for context window lookup (uses default model if not specified)
    #[serde(default)]
    pub model_id: Option<String>,
}

fn default_keep_recent() -> usize {
    DEFAULT_KEEP_RECENT_EXCHANGES
}

impl Default for CompactionOptions {
    fn default() -> Self {
        Self {
            keep_recent_exchanges: DEFAULT_KEEP_RECENT_EXCHANGES,
            force: false,
            model_id: None,
        }
    }
}

// ============================================================================
// Summary Generation
// ============================================================================

/// System prompt for summary generation - outputs structured XML
const SUMMARY_SYSTEM_PROMPT: &str = r#"You are a conversation summarizer creating a checkpoint for conversation continuity.

Output your summary in this EXACT XML structure:

<context>
<!-- What the user is trying to accomplish - their primary goals -->
- [Goal 1]
- [Goal 2]
</context>

<decisions>
<!-- All technical decisions, preferences, and constraints established -->
- [Decision 1]
- [Decision 2]
</decisions>

<files>
<!-- Files, paths, and code locations discussed or modified -->
- [file/path 1]
- [file/path 2]
</files>

<progress>
<!-- What has been accomplished so far -->
- [Completed item 1]
- [Completed item 2]
</progress>

<pending>
<!-- Unresolved questions, pending tasks, or next steps -->
- [Pending item 1]
- [Pending item 2]
</pending>

<errors>
<!-- Key errors encountered and how they were resolved (if any) -->
- [Error and resolution]
</errors>

RULES:
- Keep total summary under 800 tokens
- Be factual and specific with concrete details (names, paths, values)
- Include ALL relevant technical details - this is a handoff document
- Omit empty sections (e.g., if no errors, omit <errors>)
- Do NOT include pleasantries, meta-commentary, or step-by-step recreation
- This summary will be injected as prior context for a new conversation instance"#;

/// Generate a summary of messages using the LLM (device bearer → virtues-api).
///
/// Through the shared background helper on the Lite slot, thinking off and
/// no cap. A 1000-token cap sat here for a summary the prompt itself bounds
/// in words; on a Lite pin that thinks (the default glm-4.7-flash lists a
/// toggle) it was spent thinking.
/// The most history one summarizer call is handed, in bytes (~60k tokens).
///
/// Compaction fires at 85% of the CHAT model's window, and the whole
/// uncompacted history used to go to the Lite model in ONE call. On a 500k
/// window that is a 480k-token request, which no Lite model takes: the call
/// failed, compaction failed, and the turn went out uncompacted — the exact
/// turn that was too big. The history is summarized in batches instead, each
/// folding the previous summary in, which is the shape the prompt already
/// describes ("previous summary" + "new messages").
const SUMMARY_INPUT_BYTES: usize = 240_000;
/// One message larger than this (a pasted document, a tool result) is cut
/// in the summarizer's input. The transcript keeps the whole thing.
const SUMMARY_MESSAGE_BYTES: usize = 60_000;
/// Per summarizer call, not per compaction: a long history is several calls.
const SUMMARY_CALL_TIMEOUT: Duration = Duration::from_secs(60);
/// The most calls one compaction makes. The turn's response does not open
/// until compaction returns, so this bounds what a person waits through:
/// four calls is ~1MB of history and a few minutes at worst. Older batches
/// beyond it are not read; the summary says so.
const SUMMARY_MAX_BATCHES: usize = 4;

/// Cut the messages into runs that each fit one summarizer call. A message
/// counts at most `SUMMARY_MESSAGE_BYTES` because that is all of it the call
/// sees. Never empty: an empty input is one empty batch.
fn summary_batches(messages: &[ChatMessage]) -> Vec<&[ChatMessage]> {
    let mut batches: Vec<&[ChatMessage]> = Vec::new();
    let mut start = 0;
    let mut size = 0usize;
    for (i, msg) in messages.iter().enumerate() {
        let len = msg.content.len().min(SUMMARY_MESSAGE_BYTES) + 32;
        if i > start && size + len > SUMMARY_INPUT_BYTES {
            batches.push(&messages[start..i]);
            start = i;
            size = 0;
        }
        size += len;
    }
    batches.push(&messages[start..]);
    batches
}

async fn generate_summary(
    pool: &PgPool,
    messages: &[ChatMessage],
    existing_summary: Option<&str>,
) -> Result<String> {
    let batches = summary_batches(messages);

    if batches.len() > 1 {
        tracing::info!(
            batches = batches.len(),
            messages = messages.len(),
            "compaction: summarizing in batches"
        );
    }

    let mut summary: Option<String> = existing_summary.map(str::to_string);

    // Bounded: the oldest batches past the cap are not summarized, and the
    // summary carries a line saying how much it does not cover.
    let skip = batches.len().saturating_sub(SUMMARY_MAX_BATCHES);
    if skip > 0 {
        let dropped: usize = batches[..skip].iter().map(|b| b.len()).sum();
        tracing::warn!(dropped_messages = dropped, batches = batches.len(), "compaction: history past the batch cap is not summarized");
        let note = format!("[{dropped} earlier messages are not represented in this summary.]");
        summary = Some(match summary {
            Some(s) => format!("{s}\n{note}"),
            None => note,
        });
    }

    for batch in &batches[skip..] {
        let next = timeout(
            SUMMARY_CALL_TIMEOUT,
            summarize_batch(pool, batch, summary.as_deref()),
        )
        .await
        .map_err(|_| crate::Error::Other("Summary generation timed out after 60s".to_string()))??;
        // An empty reply is a refusal or a model that spent its output
        // elsewhere. Stored, it would replace every earlier batch with
        // nothing and advance the index past them; the caller keeps the old
        // summary and the turn goes out uncompacted instead.
        if next.trim().is_empty() {
            return Err(crate::Error::Other("Summary generation returned nothing".to_string()));
        }
        summary = Some(next);
    }
    Ok(summary.unwrap_or_default())
}

/// One summarizer call: the previous summary, then these messages.
async fn summarize_batch(
    pool: &PgPool,
    messages: &[ChatMessage],
    existing_summary: Option<&str>,
) -> Result<String> {
    let mut content = String::new();

    if let Some(summary) = existing_summary {
        content.push_str("=== PREVIOUS SUMMARY ===\n");
        content.push_str(summary);
        content.push_str("\n\n=== NEW MESSAGES TO INCORPORATE ===\n");
    }

    for msg in messages {
        let role_label = match msg.role.as_str() {
            "user" => "User",
            "assistant" => "Assistant",
            "system" => "System",
            _ => &msg.role,
        };

        let body = if msg.content.len() > SUMMARY_MESSAGE_BYTES {
            // On a char boundary, so a cut inside a multibyte character
            // does not panic.
            let mut end = SUMMARY_MESSAGE_BYTES;
            while !msg.content.is_char_boundary(end) {
                end -= 1;
            }
            format!("{}\n[… {} more bytes not shown]", &msg.content[..end], msg.content.len() - end)
        } else {
            msg.content.clone()
        };
        content.push_str(&format!("{}: {}\n\n", role_label, body));

        if let Some(tool_calls) = &msg.tool_calls {
            for tc in tool_calls {
                content.push_str(&format!("  [Tool: {}]\n", tc.tool_name));
            }
        }
    }

    crate::virtues_api::completion::system_completion(
        pool,
        virtues_registry::models::ModelSlot::Lite,
        "compaction",
        SUMMARY_SYSTEM_PROMPT,
        &content,
        crate::virtues_api::request::Thinking::Off,
        SUMMARY_TEMPERATURE,
    )
    .await
    .map_err(|e| crate::Error::Other(format!("Summary generation failed: {e}")))
}

// ============================================================================
// Compaction Logic
// ============================================================================

/// Compact a chat by summarizing older messages
///
/// This function:
/// 1. Loads the chat and its messages
/// 2. Determines which messages to summarize (all except recent N exchanges)
/// 3. Generates a summary incorporating the existing summary (if any)
/// 4. Updates the chat with the new summary and metadata
pub async fn compact_chat(
    pool: &PgPool,
    chat_id: String,
    options: CompactionOptions,
) -> Result<CompactionResult> {
    let chat_id_str = chat_id.clone();

    // Load chat metadata
    let chat_row = sqlx::query(
        r#"
        SELECT
            message_count,
            conversation_summary, summary_up_to_index, summary_version
        FROM app_chats
        WHERE id = $1
        "#,
    )
    .bind(&chat_id_str)
    .fetch_optional(pool)
    .await?;

    let chat_row =
        chat_row.ok_or_else(|| crate::Error::NotFound("Chat not found".into()))?;

    use sqlx::Row;
    let _message_count: i64 = chat_row.get("message_count");
    let conversation_summary: Option<String> = chat_row.get("conversation_summary");
    let summary_up_to_index: i32 = chat_row.get::<i64, _>("summary_up_to_index") as i32;
    let summary_version: i32 = chat_row.get::<i64, _>("summary_version") as i32;

    // Load messages from normalized table
    let message_rows = sqlx::query(
        r#"
        SELECT
            id, role, content, created_at as timestamp,
            model, provider, agent_id, reasoning, tool_calls, intent, subject, reasoning_details, parts
        FROM app_chat_messages
        WHERE chat_id = $1
        ORDER BY sequence_num ASC
        "#,
    )
    .bind(&chat_id_str)
    .fetch_all(pool)
    .await?;
    
    let messages: Vec<ChatMessage> = message_rows
        .into_iter()
        .map(|row| {
            use sqlx::Row;
            let id: String = row.get("id");
            let role: String = row.get("role");
            let content: String = row.get("content");
            let timestamp: Timestamp = row.get("timestamp");
            let model: Option<String> = row.get("model");
            let provider: Option<String> = row.get("provider");
            let agent_id: Option<String> = row.get("agent_id");
            let reasoning: Option<String> = row.get("reasoning");
            let tool_calls_raw: Option<serde_json::Value> = row.get("tool_calls");
            // `parts` carries the attachments. Leaving it out here is why the
            // gauge never saw a PDF: the estimate is what decides compaction.
            let parts_raw: Option<serde_json::Value> = row.get("parts");
            let intent_raw: Option<serde_json::Value> = row.get("intent");
            let subject: Option<String> = row.get("subject");
            let reasoning_details: Option<serde_json::Value> = row.get("reasoning_details");

            let tool_calls = tool_calls_raw.and_then(|tc| {
                serde_json::from_value(tc)
                    .map_err(|e| tracing::warn!(msg_id = %id, error = %e, "tool_calls did not parse"))
                    .ok()
            });
            let intent = intent_raw
                .and_then(|i| serde_json::from_value(i).ok());

            ChatMessage {
                id: Some(id.clone()),
                role,
                content,
                timestamp,
                model,
                provider,
                agent_id,
                reasoning,
                tool_calls,
                intent,
                subject,
                reasoning_details,
                parts: parts_raw.and_then(|p| crate::api::chat::parts_from_jsonb(p, &id)),
            }
        })
        .collect();
    
    let message_count = messages.len();

    // Calculate how many messages to keep verbatim
    // Each "exchange" is typically 2 messages (user + assistant)
    let messages_to_keep = options.keep_recent_exchanges * 2;

    // If we don't have enough messages to summarize, skip
    if message_count <= messages_to_keep && !options.force {
        return Ok(CompactionResult {
            success: true,
            messages_summarized: 0,
            messages_kept_verbatim: message_count as i32,
            summary_tokens: 0,
            new_usage_percentage: 0.0,
            previous_usage_percentage: 0.0,
            summary_version,
        });
    }

    // Determine the split point
    let split_index = if message_count > messages_to_keep {
        message_count - messages_to_keep
    } else {
        0
    };

    // Get the current summary index (messages already summarized)
    let current_summary_index = summary_up_to_index as usize;

    // Messages to add to summary (from current_summary_index to split_index)
    let messages_to_summarize = if split_index > current_summary_index {
        &messages[current_summary_index..split_index]
    } else {
        // Nothing new to summarize
        return Ok(CompactionResult {
            success: true,
            messages_summarized: current_summary_index as i32,
            messages_kept_verbatim: (message_count - current_summary_index) as i32,
            summary_tokens: 0,
            new_usage_percentage: 0.0,
            previous_usage_percentage: 0.0,
            summary_version,
        });
    };

    // The model is the Lite slot through the owner's background pin,
    // resolved by the helper. The timeout is per summarizer call, inside.
    let new_summary = generate_summary(
        pool,
        messages_to_summarize,
        conversation_summary.as_deref(),
    )
    .await?;

    // Calculate previous and new usage percentages, against the window the
    // turn will actually be sent to (BYO included — see `turn_context_window`).
    let context_window = if let Some(model_id) = &options.model_id {
        crate::api::chat_usage::turn_context_window(pool, model_id).await
    } else {
        crate::api::token_estimation::ASSUMED_CONTEXT_WINDOW
    };
    let previous_estimate = estimate_session_context(&messages, None, None, context_window);
    let verbatim_messages = &messages[split_index..];
    let new_estimate =
        estimate_session_context(verbatim_messages, Some(&new_summary), None, context_window);

    // Update the chat metadata
    let now = Timestamp::now();
    let new_version = summary_version + 1;
    let new_summary_index = split_index as i32;

    sqlx::query(
        r#"
        UPDATE app_chats
        SET
            conversation_summary = $1,
            summary_up_to_index = $2,
            summary_version = $3,
            last_compacted_at = $4,
            updated_at = $5
        WHERE id = $6
        "#,
    )
    .bind(&new_summary)
    .bind(new_summary_index)
    .bind(new_version)
    .bind(&now)
    .bind(&now)
    .bind(&chat_id_str)
    .execute(pool)
    .await?;

    // Insert checkpoint message into chat_messages
    // This makes the checkpoint visible in the chat UI and queryable
    tracing::info!(chat_id = %chat_id, version = new_version, "Creating checkpoint message");
    let checkpoint_part = UIPart::Checkpoint {
        version: new_version,
        messages_summarized: new_summary_index,
        // The last message this summary covers, by id — the boundary has to
        // survive rows being deleted below it.
        last_message_id: split_index
            .checked_sub(1)
            .and_then(|i| messages.get(i))
            .and_then(|m| m.id.clone()),
        summary: new_summary.clone(),
        timestamp: now.to_rfc3339(),
    };

    let checkpoint_message = ChatMessage {
        id: None, // Will be generated
        role: "checkpoint".to_string(),
        content: format!("Checkpoint v{}: {} messages summarized", new_version, new_summary_index),
        timestamp: now,
        model: None,
        provider: None,
        agent_id: None,
        reasoning: None,
        tool_calls: None,
        intent: None,
        subject: None,
        reasoning_details: None,
        parts: Some(vec![checkpoint_part]),
    };

    // Append the checkpoint message
    tracing::info!(chat_id = %chat_id, "Inserting checkpoint message");
    let checkpoint_msg_id = crate::api::chats::append_message(pool, chat_id.clone(), checkpoint_message).await?;
    tracing::info!(chat_id = %chat_id, msg_id = %checkpoint_msg_id, "Checkpoint message inserted");

    Ok(CompactionResult {
        success: true,
        messages_summarized: new_summary_index,
        messages_kept_verbatim: (message_count - split_index) as i32,
        summary_tokens: new_estimate.total_tokens,
        new_usage_percentage: new_estimate.usage_percentage,
        previous_usage_percentage: previous_estimate.usage_percentage,
        summary_version: new_version as i32,
    })
}

/// Build the context to send to the LLM, using checkpoint-based or legacy summary approach
///
/// This function now supports checkpoint messages:
/// 1. Finds the latest checkpoint message in the messages array
/// 2. Extracts the summary from that checkpoint
/// 3. Includes only messages AFTER the checkpoint
/// 4. Skips checkpoint messages in output (they're metadata, not conversation)
///
/// Falls back to legacy summary/summary_up_to_index if no checkpoint found.
///
/// Returns a vector of messages in OpenAI format ready for the API.
/// Note: Summary is combined into the system prompt to avoid multiple system messages,
/// which most LLM providers don't handle well.
/// Whether to ask the provider to cache the system prefix. ON unless
/// `VIRTUES_PROMPT_CACHE=0` — a kill switch that needs no rebuild, not an
/// opt-in. See the call site for what was measured before it was turned on.
fn prompt_cache_enabled() -> bool {
    !matches!(
        std::env::var("VIRTUES_PROMPT_CACHE").as_deref(),
        Ok("0") | Ok("false") | Ok("FALSE")
    )
}

pub fn build_context_for_llm(
    messages: &[ChatMessage],
    summary: Option<&str>,
    summary_up_to_index: usize,
    system_prompt: Option<&str>,
    system_tail: Option<&str>,
) -> Vec<serde_json::Value> {
    let mut context = Vec::new();

    // Find the latest checkpoint message and its index
    let (checkpoint_summary, checkpoint_split_index) = find_latest_checkpoint(messages);

    // Determine which summary to use (checkpoint takes precedence over legacy)
    let effective_summary = checkpoint_summary.as_deref().or(summary);
    let effective_start_index = if checkpoint_summary.is_some() {
        checkpoint_split_index
    } else {
        summary_up_to_index
    };

    // 1. Build combined system content (prompt + summary in one message)
    // Most LLM providers only properly handle one system message
    let mut system_content = String::new();
    if let Some(prompt) = system_prompt {
        system_content.push_str(prompt);
    }

    // Append summary as part of system prompt, not separate message
    if let Some(summary_text) = effective_summary {
        if !system_content.is_empty() {
            system_content.push_str("\n\n");
        }
        system_content.push_str("<compacted_conversation>\n");
        system_content.push_str(summary_text);
        system_content.push_str("\n</compacted_conversation>");
    }

    // The per-turn tail goes AFTER the breakpoint, so it can change freely
    // without invalidating the prefix. Keeping the textual order the model
    // sees exactly as it was — stable blocks, then tail, then summary — since
    // block order is a deliberate product decision (rules last, for adherence)
    // and a caching change has no business reordering the prompt. The cost is
    // that a compaction summary sits outside the cached block and is re-sent
    // whole each turn; moving it would change what the model reads, so that is
    // a decision for whoever owns prompt order, not a side effect of this.
    let tail = system_tail.unwrap_or("");

    // Only add system message if there's content
    if !system_content.is_empty() || !tail.is_empty() {
        // The system prompt is the largest stable prefix of every request in a
        // conversation, so it is where a cache breakpoint is worth the most.
        // Marking it needs block-shaped content rather than a bare string.
        //
        // Measured end to end on 2026-09-17 before this was turned on, because
        // a body a gateway refuses is a 400 on every chat turn for every box:
        //
        //   - `upstream_body` (services/virtues-api/src/providers.rs) rewrites
        //     model/stream/temperature/providerOptions and passes `messages`
        //     through untouched, so the marker survives the hop verbatim.
        //   - Two identical turns on anthropic/claude-haiku-4.5: the second
        //     reported 13,522 of ~13,865 prompt tokens served from cache.
        //   - xai/grok-4.5, zai/glm-4.7-flash and alibaba/qwen3-coder-plus all
        //     completed normally with the same body — block-shaped system
        //     content is not an Anthropic-only dialect here.
        //
        // Before this, `cache_control` appeared nowhere in the repo and every
        // usage row read 0 cache tokens across 29.2M input tokens.
        if prompt_cache_enabled() && !system_content.is_empty() {
            context.push(serde_json::json!({
                "role": "system",
                "content": [{
                    "type": "text",
                    "text": system_content,
                    "cache_control": { "type": "ephemeral" }
                }]
            }));
            if !tail.is_empty() {
                context.push(serde_json::json!({ "role": "system", "content": tail }));
            }
        } else {
            context.push(serde_json::json!({
                "role": "system",
                "content": format!("{system_content}{tail}")
            }));
        }
    }

    // 2. Recent messages (after checkpoint or summary_up_to_index)
    //
    // An index past the end means the summary covers everything, so what
    // follows it is nothing. This used to fall back to the WHOLE history, which
    // sent the summary AND every message it replaced — the context grew at the
    // one moment it had to shrink, since compaction only runs at 85% of the
    // window.
    let recent_messages = &messages[effective_start_index.min(messages.len())..];

    for msg in recent_messages {
        // Skip checkpoint messages - they're metadata, not conversation
        if msg.role == "checkpoint" {
            continue;
        }

        let mut parts = Vec::new();
        // A turn's tool calls do NOT live in its content. In the chat-completions
        // shape every provider here speaks, they are a sibling `tool_calls` field
        // on the assistant message, and each result is its own `role: "tool"`
        // message that must follow it immediately. Putting them in `content` —
        // which this did until a turn first arrived with `parts` populated —
        // is a flat 400 (`param: "messages.N.content"`), and it poisons the chat
        // rather than the turn: the bad message is persisted, so every later turn
        // replays it and fails too.
        let mut tool_calls = Vec::new();
        let mut tool_results = Vec::new();

        // Handle parts if present
        if let Some(msg_parts) = &msg.parts {
            for part in msg_parts {
                match part {
                    UIPart::Text { text } => {
                        // An empty text block is a hard 400 from Anthropic
                        // ("text content blocks must be non-empty") and a
                        // no-op everywhere else. The composer used to send one
                        // beside every attachment-only message (the AI SDK
                        // appends a text part for text `""`), and since user
                        // parts persist verbatim, that one turn poisoned every
                        // later Claude turn in the chat while Grok kept
                        // working. Dropping it here heals chats already
                        // carrying one.
                        if text.trim().is_empty() {
                            continue;
                        }
                        parts.push(serde_json::json!({
                            "type": "text",
                            "text": text
                        }));
                    }
                    UIPart::Reasoning { .. } => {
                        // `reasoning` is not a content block type either, and a
                        // replayed one is the same 400 the tool blocks below
                        // used to be. What a provider can actually take back is
                        // carried by the `reasoning_details` column, not here.
                    }
                    UIPart::ToolInvocation { tool_call_id, tool_name, input, output, error_text, .. } => {
                        // `function.arguments` is a JSON *string*, not an object.
                        // Anything that is not an argument map — a turn saved
                        // before the completed args were written back left
                        // `null` here — goes out as `{}` so the call still
                        // pairs with its result; that heals chats already
                        // carrying one.
                        let arguments = match input {
                            serde_json::Value::Object(_) => input.to_string(),
                            _ => "{}".to_string(),
                        };
                        tool_calls.push(serde_json::json!({
                            "id": tool_call_id,
                            "type": "function",
                            "function": { "name": tool_name, "arguments": arguments }
                        }));
                        // Every tool call must be answered or the request is
                        // rejected for the gap, so a call this turn never got
                        // an output for is answered with what is true about it.
                        // A call that FAILED is answered as a failure: replaying
                        // an error object in the result position tells the model
                        // the tool succeeded and returned something odd.
                        let content = match (error_text, output) {
                            (Some(err), _) => format!("Tool failed ({tool_name}): {err}"),
                            (None, Some(serde_json::Value::String(s))) => s.clone(),
                            (None, Some(res)) => res.to_string(),
                            (None, None) => "the tool did not finish".to_string(),
                        };
                        let content = clip_replayed_output(content);
                        tool_results.push(serde_json::json!({
                            "role": "tool",
                            "tool_call_id": tool_call_id,
                            "content": content
                        }));
                    }
                    UIPart::File { media_type, url, filename } => {
                        // Convert attachments to OpenAI-compatible content blocks
                        // (the gateway translates these per provider).
                        if media_type.starts_with("image/") {
                            parts.push(serde_json::json!({
                                "type": "image_url",
                                "image_url": { "url": url }
                            }));
                        } else if media_type == "application/pdf" {
                            // Vercel AI Gateway expects RAW base64 in `data` (no
                            // data: prefix) + `media_type` + `filename`.
                            let data = url.split_once(',').map(|(_, b)| b).unwrap_or(url.as_str());
                            parts.push(serde_json::json!({
                                "type": "file",
                                "file": {
                                    "data": data,
                                    "media_type": media_type,
                                    "filename": filename.clone().unwrap_or_else(|| "document.pdf".to_string())
                                }
                            }));
                        } else if media_type.starts_with("audio/") {
                            // input_audio wants raw base64 (no data: prefix) + a format token.
                            let data = url.split_once(',').map(|(_, b)| b).unwrap_or(url.as_str());
                            let format = match media_type.as_str() {
                                "audio/wav" | "audio/x-wav" => "wav",
                                "audio/ogg" => "ogg",
                                "audio/flac" => "flac",
                                "audio/aac" | "audio/x-m4a" | "audio/mp4" => "m4a",
                                "audio/webm" => "webm",
                                _ => "mp3",
                            };
                            parts.push(serde_json::json!({
                                "type": "input_audio",
                                "input_audio": { "data": data, "format": format }
                            }));
                        } else if media_type.starts_with("text/") || media_type == "application/json" {
                            // Text/code/doc files (and long pasted text) ride inline as
                            // a text block so any model can read them. The data URL holds
                            // base64 UTF-8.
                            let b64 = url.split_once(',').map(|(_, b)| b).unwrap_or(url.as_str());
                            let name = filename.clone().unwrap_or_else(|| "file.txt".to_string());
                            // Two `.ok()`s and a default used to make a payload
                            // that would not decode into an empty string, and
                            // the model was handed `[File: notes.txt]` with
                            // nothing under it — a header asserting a file that
                            // has no contents, which it then answered about
                            // confidently. Say what is true instead.
                            let decoded = base64::Engine::decode(
                                &base64::engine::general_purpose::STANDARD,
                                b64,
                            )
                            .ok()
                            .and_then(|bytes| String::from_utf8(bytes).ok());
                            let text = match decoded {
                                Some(content) => format!("[File: {}]\n{}", name, content),
                                None => format!(
                                    "[File: {}] — attached, but its contents could not be read. \
                                     Say so rather than guessing what it said.",
                                    name
                                ),
                            };
                            parts.push(serde_json::json!({ "type": "text", "text": text }));
                        } else {
                            // Unknown type — at least make the model aware of it.
                            parts.push(serde_json::json!({
                                "type": "text",
                                "text": format!("[Attached file: {}]", filename.clone().unwrap_or_else(|| media_type.clone()))
                            }));
                        }
                    }
                    UIPart::Checkpoint { .. } => {
                        // Skip checkpoint parts - handled above
                    }
                    UIPart::Unknown => {}
                }
            }
        }

        // If no parts (legacy), use content
        let content = if parts.is_empty() {
            // Nothing survived and there is no legacy content either: this
            // message would go out as `"content": ""`, which is the same
            // rejection in string form. Leaving it out is the only honest
            // rendering of a message with nothing in it — unless it is
            // carrying tool calls, which is a turn that said nothing and
            // called something, and those take `content: null`.
            if msg.content.trim().is_empty() {
                if tool_calls.is_empty() {
                    continue;
                }
                serde_json::Value::Null
            } else {
                serde_json::Value::String(msg.content.clone())
            }
        } else {
            serde_json::Value::Array(parts)
        };

        let mut message = serde_json::json!({
            "role": msg.role,
            "content": content
        });
        if !tool_calls.is_empty() {
            message["tool_calls"] = serde_json::Value::Array(tool_calls);
        }
        context.push(message);
        // Results answer the message that asked for them, so they follow it
        // directly — a provider that finds anything else in between rejects
        // the whole request.
        context.extend(tool_results);
    }

    context
}

/// How much of one tool's output is replayed on later turns.
///
/// The turn that called the tool sees all of it — that is what it asked for.
/// Every turn after replays it, though, and measured on a real box the median
/// tool result is 15 KB and the ninetieth percentile 94 KB, so three of them
/// compound past any window. The turn that needed the detail had it; a later
/// turn needs to know what was found, and can call again.
const MAX_REPLAYED_TOOL_BYTES: usize = 32 * 1024;

fn clip_replayed_output(content: String) -> String {
    if content.len() <= MAX_REPLAYED_TOOL_BYTES {
        return content;
    }
    // On a char boundary, or `String::truncate` panics on multi-byte text.
    let mut cut = MAX_REPLAYED_TOOL_BYTES;
    while cut > 0 && !content.is_char_boundary(cut) {
        cut -= 1;
    }
    let dropped = content.len() - cut;
    let mut clipped = content;
    clipped.truncate(cut);
    clipped.push_str(&format!(
        "\n… [{dropped} more bytes were returned to an earlier turn and are not repeated here; \
         call the tool again if you need them]"
    ));
    clipped
}

/// Find the latest checkpoint message and extract its summary.
///
/// Returns `(Option<summary_text>, first_message_the_summary_does_not_cover)`.
/// If no checkpoint is found, returns `(None, 0)`.
///
/// That index is the checkpoint's own `messages_summarized`, NOT the position
/// the checkpoint row sits at. `compact_chat` APPENDS its checkpoint after the
/// messages it deliberately left out of the summary, so "start after the
/// checkpoint" started after the verbatim window too — the most recent
/// exchanges were in neither the summary nor the context, and the model simply
/// lost them. `messages_summarized` is the split point, and it is the same
/// number the legacy `summary_up_to_index` path uses, so the two agree.
fn find_latest_checkpoint(messages: &[ChatMessage]) -> (Option<String>, usize) {
    // Search from the end to find the most recent checkpoint
    for msg in messages.iter().rev() {
        if msg.role == "checkpoint" {
            // Extract summary from checkpoint part
            if let Some(parts) = &msg.parts {
                for part in parts {
                    if let UIPart::Checkpoint {
                        summary,
                        messages_summarized,
                        last_message_id,
                        ..
                    } = part
                    {
                        // By id where there is one: the count is a position
                        // taken when the checkpoint was written, and rows get
                        // deleted below it.
                        let by_id = last_message_id.as_deref().and_then(|id| {
                            messages
                                .iter()
                                .position(|m| m.id.as_deref() == Some(id))
                                .map(|i| i + 1)
                        });
                        let start =
                            by_id.unwrap_or_else(|| (*messages_summarized).max(0) as usize);
                        return (Some(summary.clone()), start);
                    }
                }
            }
        }
    }
    (None, 0)
}

/// Check if a chat needs compaction based on context usage
pub async fn needs_compaction(
    pool: &PgPool,
    chat_id: String,
    context_window: i64,
) -> Result<ContextStatus> {
    let chat_id_str = chat_id.clone();

    // Load chat metadata
    let chat_row = sqlx::query(
        r#"
        SELECT conversation_summary, summary_up_to_index
        FROM app_chats
        WHERE id = $1
        "#,
    )
    .bind(&chat_id_str)
    .fetch_optional(pool)
    .await?;

    let chat_row =
        chat_row.ok_or_else(|| crate::Error::NotFound("Chat not found".into()))?;

    use sqlx::Row;
    let conversation_summary: Option<String> = chat_row.get("conversation_summary");
    let summary_up_to_index: i32 = chat_row.get::<i64, _>("summary_up_to_index") as i32;

    // Load messages from normalized table
    let message_rows = sqlx::query(
        r#"
        SELECT
            id, role, content, created_at as timestamp,
            model, provider, agent_id, reasoning, tool_calls, intent, subject, reasoning_details, parts
        FROM app_chat_messages
        WHERE chat_id = $1
        ORDER BY sequence_num ASC
        "#,
    )
    .bind(&chat_id_str)
    .fetch_all(pool)
    .await?;

    let messages: Vec<ChatMessage> = message_rows
        .into_iter()
        .map(|row| {
            use sqlx::Row;
            let id: String = row.get("id");
            let role: String = row.get("role");
            let content: String = row.get("content");
            let timestamp: Timestamp = row.get("timestamp");
            let model: Option<String> = row.get("model");
            let provider: Option<String> = row.get("provider");
            let agent_id: Option<String> = row.get("agent_id");
            let reasoning: Option<String> = row.get("reasoning");
            let tool_calls_raw: Option<serde_json::Value> = row.get("tool_calls");
            // `parts` carries the attachments. Leaving it out here is why the
            // gauge never saw a PDF: the estimate is what decides compaction.
            let parts_raw: Option<serde_json::Value> = row.get("parts");
            let intent_raw: Option<serde_json::Value> = row.get("intent");
            let subject: Option<String> = row.get("subject");
            let reasoning_details: Option<serde_json::Value> = row.get("reasoning_details");

            let tool_calls = tool_calls_raw.and_then(|tc| {
                serde_json::from_value(tc)
                    .map_err(|e| tracing::warn!(msg_id = %id, error = %e, "tool_calls did not parse"))
                    .ok()
            });
            let intent = intent_raw
                .and_then(|i| serde_json::from_value(i).ok());

            ChatMessage {
                id: Some(id.clone()),
                role,
                content,
                timestamp,
                model,
                provider,
                agent_id,
                reasoning,
                tool_calls,
                intent,
                subject,
                reasoning_details,
                parts: parts_raw.and_then(|p| crate::api::chat::parts_from_jsonb(p, &id)),
            }
        })
        .collect();

    // Get verbatim messages (after summary)
    let verbatim_messages = if (summary_up_to_index as usize) < messages.len() {
        &messages[(summary_up_to_index as usize)..]
    } else {
        // The summary covers everything, so what follows it is nothing. This
        // used to estimate the ENTIRE transcript plus the summary — the largest
        // number possible, at the moment the answer matters most.
        &[]
    };

    // Estimate context with summary + verbatim messages
    let estimate = estimate_session_context(
        verbatim_messages,
        conversation_summary.as_deref(),
        None,
        context_window,
    );

    Ok(estimate.status)
}

#[cfg(test)]
mod tests {
    /// The system message is a string when caching is off and an array of text
    /// blocks when it is on (the breakpoint needs block-shaped content). Tests
    /// that care about the TEXT should not care which — they broke silently
    /// when the cache default flipped, because they read `.as_str()` and got
    /// `None` from a shape that was perfectly correct.
    fn system_text(message: &serde_json::Value) -> String {
        let content = &message["content"];
        if let Some(s) = content.as_str() {
            return s.to_string();
        }
        content
            .as_array()
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default()
    }

    /// The cache breakpoint marks the STABLE prefix, and the per-turn tail
    /// travels as its OWN system message.
    ///
    /// Two earlier shapes were measured against the live gateway and both
    /// cached nothing once a page was bound:
    ///   - the marker on one block holding the whole prompt: the open page's
    ///     live text is then inside the cached block, so every keystroke
    ///     changes the prefix;
    ///   - the marker on the first of TWO content blocks in one system
    ///     message: the gateway accepts the body, answers normally, and
    ///     silently ignores the marker. Nothing is logged. 0 cached tokens.
    ///
    /// This shape measured 53,346 of 54,039 prompt tokens served from cache
    /// with the page text changing on every turn. Do not "simplify" it back
    /// into one message without re-measuring that number.
    #[test]
    fn the_per_turn_tail_travels_as_its_own_system_message() {
        let context =
            build_context_for_llm(&[], None, 0, Some("STABLE-PREFIX"), Some("VOLATILE-TAIL"));
        assert_eq!(context.len(), 2, "the tail is a separate message, not a second block");

        let parts = context[0]["content"]
            .as_array()
            .expect("the cached prefix is block-shaped so it can carry the marker");
        assert_eq!(parts.len(), 1, "the marked message holds exactly one block");
        assert_eq!(parts[0]["text"], "STABLE-PREFIX");
        assert!(parts[0].get("cache_control").is_some(), "the prefix is marked");

        assert_eq!(context[1]["role"], "system");
        assert_eq!(context[1]["content"], "VOLATILE-TAIL");
        assert!(
            !context[1].to_string().contains("cache_control"),
            "the per-turn tail is never inside the cached block"
        );
    }


    use super::*;

    fn text(role: &str, content: String) -> ChatMessage {
        ChatMessage {
            id: None,
            role: role.to_string(),
            content,
            timestamp: Timestamp::parse("2024-01-01T00:00:00Z").unwrap(),
            model: None,
            provider: None,
            agent_id: None,
            tool_calls: None,
            reasoning: None,
            intent: None,
            subject: None,
            reasoning_details: None,
            parts: None,
        }
    }

    /// The summarizer used to be handed the whole history in one call: at
    /// 85% of a 500k window that is ~480k tokens, which no Lite model takes,
    /// so compaction failed on exactly the turn that needed it.
    #[test]
    fn a_long_history_is_summarized_in_bounded_batches() {
        // 30 messages of 20k bytes = 600k, three times the per-call cap.
        let messages: Vec<ChatMessage> = (0..30)
            .map(|i| text(if i % 2 == 0 { "user" } else { "assistant" }, "x".repeat(20_000)))
            .collect();
        let batches = summary_batches(&messages);
        assert!(batches.len() >= 3, "expected several batches, got {}", batches.len());
        assert_eq!(batches.iter().map(|b| b.len()).sum::<usize>(), 30, "every message lands once");
        for b in &batches {
            let bytes: usize = b.iter().map(|m| m.content.len() + 32).sum();
            assert!(bytes <= SUMMARY_INPUT_BYTES, "a batch of {bytes} bytes exceeds the cap");
        }

        // One message past the cap on its own is still one batch: it is cut
        // inside the call, not dropped.
        let huge = vec![text("user", "y".repeat(SUMMARY_INPUT_BYTES * 2))];
        assert_eq!(summary_batches(&huge).len(), 1);

        // A short history is one call, as before.
        let short = vec![text("user", "hi".into()), text("assistant", "hello".into())];
        assert_eq!(summary_batches(&short).len(), 1);
    }

    #[test]
    fn test_build_context_without_summary() {
        let messages = vec![
            ChatMessage {
                id: None,
                role: "user".to_string(),
                content: "Hello".to_string(),
                timestamp: Timestamp::parse("2024-01-01T00:00:00Z").unwrap(),
                model: None,
                provider: None,
                agent_id: None,
                tool_calls: None,
                reasoning: None,
                intent: None,
                subject: None,
                reasoning_details: None,
                parts: None,
            },
            ChatMessage {
                id: None,
                role: "assistant".to_string(),
                content: "Hi there!".to_string(),
                timestamp: Timestamp::parse("2024-01-01T00:00:01Z").unwrap(),
                model: None,
                provider: None,
                agent_id: None,
                tool_calls: None,
                reasoning: None,
                intent: None,
                subject: None,
                reasoning_details: None,
                parts: None,
            },
        ];

        let context = build_context_for_llm(&messages, None, 0, Some("You are helpful."), None);

        assert_eq!(context.len(), 3); // system + 2 messages
        assert_eq!(context[0]["role"], "system");
        assert_eq!(context[1]["role"], "user");
        assert_eq!(context[2]["role"], "assistant");
    }

    /// An attachment-only send arrives as `[file, text ""]`. The file block
    /// must survive and the empty text block must not; a message that is
    /// empty through and through must not be sent at all.
    #[test]
    fn test_build_context_drops_empty_text_blocks() {
        let base = ChatMessage {
            id: None,
            role: "user".to_string(),
            content: String::new(),
            timestamp: Timestamp::parse("2024-01-01T00:00:00Z").unwrap(),
            model: None,
            provider: None,
            agent_id: None,
            tool_calls: None,
            reasoning: None,
            intent: None,
            subject: None,
            reasoning_details: None,
            parts: None,
        };
        let messages = vec![
            ChatMessage {
                parts: Some(vec![
                    UIPart::File {
                        media_type: "image/png".to_string(),
                        url: "data:image/png;base64,AAAA".to_string(),
                        filename: Some("shot.png".to_string()),
                    },
                    UIPart::Text { text: String::new() },
                ]),
                ..base.clone()
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: "I see it.".to_string(),
                ..base.clone()
            },
            // Empty in every representation: parts that all drop, no content.
            ChatMessage {
                parts: Some(vec![UIPart::Text { text: "   ".to_string() }]),
                ..base.clone()
            },
            ChatMessage {
                content: "and then?".to_string(),
                ..base
            },
        ];

        let context = build_context_for_llm(&messages, None, 0, None, None);

        assert_eq!(context.len(), 3, "the all-empty message is omitted");
        let first = context[0]["content"].as_array().expect("parts array");
        assert_eq!(first.len(), 1, "only the file block survives");
        assert_eq!(first[0]["type"], "image_url");
        assert_eq!(context[1]["role"], "assistant");
        assert_eq!(context[2]["content"], "and then?");
    }

    /// A tool-calling turn replayed as history. Every field here was a 400
    /// at `messages.N.content` when the turn's `parts` first arrived
    /// populated and the calls went out as content blocks.
    #[test]
    fn test_build_context_renders_tool_calls_beside_the_message() {
        let base = ChatMessage {
            id: None,
            role: "assistant".to_string(),
            content: String::new(),
            timestamp: Timestamp::parse("2024-01-01T00:00:00Z").unwrap(),
            model: None,
            provider: None,
            agent_id: None,
            tool_calls: None,
            reasoning: None,
            intent: None,
            subject: None,
            reasoning_details: None,
            parts: None,
        };
        let messages = vec![ChatMessage {
            content: "Pulling a bit of context.".to_string(),
            parts: Some(vec![
                UIPart::Text { text: "Pulling a bit of context.".to_string() },
                UIPart::ToolInvocation {
                    tool_call_id: "call-0".to_string(),
                    tool_name: "semantic_search".to_string(),
                    input: serde_json::json!({ "queries": ["shared projects"] }),
                    state: "output-available".to_string(),
                    output: Some(serde_json::json!({ "count": 15 })),
                    error_text: None,
                },
                // A turn saved before the completed args were written back.
                UIPart::ToolInvocation {
                    tool_call_id: "call-1".to_string(),
                    tool_name: "get_page_content".to_string(),
                    input: serde_json::Value::Null,
                    state: "input-available".to_string(),
                    output: None,
                    error_text: None,
                },
                UIPart::Text { text: "Here they are.".to_string() },
            ]),
            ..base
        }];

        let context = build_context_for_llm(&messages, None, 0, None, None);

        assert_eq!(context.len(), 3, "the turn, then one result per call");
        let turn = &context[0];
        assert_eq!(turn["role"], "assistant");

        let content = turn["content"].as_array().expect("parts array");
        assert_eq!(content.len(), 2, "only the text survives in content");
        assert!(content.iter().all(|p| p["type"] == "text"));

        let calls = turn["tool_calls"].as_array().expect("tool_calls beside it");
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0]["type"], "function");
        assert_eq!(calls[0]["function"]["name"], "semantic_search");
        // A JSON string, not an object — and never `null`.
        assert_eq!(
            calls[0]["function"]["arguments"],
            r#"{"queries":["shared projects"]}"#
        );
        assert_eq!(calls[1]["function"]["arguments"], "{}");

        assert_eq!(context[1]["role"], "tool");
        assert_eq!(context[1]["tool_call_id"], "call-0");
        assert!(context[1]["content"].is_string());
        // The unanswered call is still answered, or the request is rejected
        // for the gap.
        assert_eq!(context[2]["tool_call_id"], "call-1");
    }

    /// A turn that said nothing and only called something keeps its calls.
    #[test]
    fn test_build_context_keeps_a_silent_tool_turn() {
        let messages = vec![ChatMessage {
            id: None,
            role: "assistant".to_string(),
            content: String::new(),
            timestamp: Timestamp::parse("2024-01-01T00:00:00Z").unwrap(),
            model: None,
            provider: None,
            agent_id: None,
            tool_calls: None,
            reasoning: None,
            intent: None,
            subject: None,
            reasoning_details: None,
            parts: Some(vec![UIPart::ToolInvocation {
                tool_call_id: "call-0".to_string(),
                tool_name: "semantic_search".to_string(),
                input: serde_json::json!({}),
                state: "output-available".to_string(),
                output: Some(serde_json::Value::String("nothing found".to_string())),
                error_text: None,
            }]),
        }];

        let context = build_context_for_llm(&messages, None, 0, None, None);

        assert_eq!(context.len(), 2);
        assert!(context[0]["content"].is_null(), "no text, but not dropped");
        assert_eq!(context[0]["tool_calls"].as_array().unwrap().len(), 1);
        assert_eq!(context[1]["content"], "nothing found");
    }

    /// Compaction summarizes everything up to a split point and keeps the
    /// exchanges after it verbatim — but it APPENDS its checkpoint at the end,
    /// after those. Starting "after the checkpoint" therefore started after the
    /// verbatim window, and an index past the end fell back to the whole
    /// history.
    #[test]
    fn test_checkpoint_keeps_the_window_it_promised_to_keep() {
        fn m(role: &str, body: &str) -> ChatMessage {
            ChatMessage {
                id: None,
                role: role.to_string(),
                content: body.to_string(),
                timestamp: Timestamp::parse("2024-01-01T00:00:00Z").unwrap(),
                model: None, provider: None, agent_id: None, tool_calls: None,
                reasoning: None, intent: None, subject: None,
                reasoning_details: None, parts: None,
            }
        }
        // 20 messages; compaction summarized m0..m11 and kept m12..m19.
        let mut msgs: Vec<ChatMessage> = (0..20)
            .map(|i| m(if i % 2 == 0 { "user" } else { "assistant" }, &format!("m{i}")))
            .collect();
        let mut checkpoint = m("checkpoint", "Checkpoint v1");
        checkpoint.parts = Some(vec![UIPart::Checkpoint {
            version: 1,
            messages_summarized: 12,
            last_message_id: None,
            summary: "SUMMARY-OF-m0-TO-m11".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        }]);
        msgs.push(checkpoint);

        let context = build_context_for_llm(&msgs, None, 12, Some("SYS"), None);

        assert_eq!(context[0]["role"], "system");
        assert!(system_text(&context[0]).contains("SUMMARY-OF-m0-TO-m11"));
        let bodies: Vec<&str> = context[1..]
            .iter()
            .map(|m| m["content"].as_str().unwrap())
            .collect();
        assert_eq!(
            bodies,
            ["m12", "m13", "m14", "m15", "m16", "m17", "m18", "m19"],
            "the verbatim window, and nothing the summary already covers"
        );
    }

    /// The checkpoint is the last message and nothing has been said since.
    #[test]
    fn test_a_summary_that_covers_everything_leaves_nothing_behind() {
        let base = ChatMessage {
            id: None,
            role: "user".to_string(),
            content: String::new(),
            timestamp: Timestamp::parse("2024-01-01T00:00:00Z").unwrap(),
            model: None, provider: None, agent_id: None, tool_calls: None,
            reasoning: None, intent: None, subject: None,
            reasoning_details: None, parts: None,
        };
        let msgs = vec![
            ChatMessage { content: "m0".to_string(), ..base.clone() },
            ChatMessage {
                role: "checkpoint".to_string(),
                content: "Checkpoint v1".to_string(),
                parts: Some(vec![UIPart::Checkpoint {
                    version: 1,
                    messages_summarized: 1,
                    last_message_id: None,
                    summary: "ALL-OF-IT".to_string(),
                    timestamp: String::new(),
                }]),
                ..base
            },
        ];

        let context = build_context_for_llm(&msgs, None, 1, Some("SYS"), None);

        assert_eq!(context.len(), 1, "the system message and nothing else");
        assert!(system_text(&context[0]).contains("ALL-OF-IT"));
    }

    #[test]
    fn test_build_context_with_summary() {
        let messages = vec![
            ChatMessage {
                id: None,
                role: "user".to_string(),
                content: "Old message".to_string(),
                timestamp: Timestamp::parse("2024-01-01T00:00:00Z").unwrap(),
                model: None,
                provider: None,
                agent_id: None,
                tool_calls: None,
                reasoning: None,
                intent: None,
                subject: None,
                reasoning_details: None,
                parts: None,
            },
            ChatMessage {
                id: None,
                role: "assistant".to_string(),
                content: "Old response".to_string(),
                timestamp: Timestamp::parse("2024-01-01T00:00:01Z").unwrap(),
                model: None,
                provider: None,
                agent_id: None,
                tool_calls: None,
                reasoning: None,
                intent: None,
                subject: None,
                reasoning_details: None,
                parts: None,
            },
            ChatMessage {
                id: None,
                role: "user".to_string(),
                content: "Recent message".to_string(),
                timestamp: Timestamp::parse("2024-01-01T00:00:02Z").unwrap(),
                model: None,
                provider: None,
                agent_id: None,
                tool_calls: None,
                reasoning: None,
                intent: None,
                subject: None,
                reasoning_details: None,
                parts: None,
            },
        ];

        // summary_up_to_index = 2 means first 2 messages are summarized
        let context = build_context_for_llm(
            &messages,
            Some("User asked about something."),
            2,
            Some("You are helpful."),
            None,
        );

        // Should have: combined system prompt (with summary), 1 recent message
        assert_eq!(context.len(), 2);
        // System message should contain both prompt and summary
        let system_content = system_text(&context[0]);
        assert!(system_content.contains("You are helpful."));
        assert!(system_content.contains("<compacted_conversation>"));
        assert!(system_content.contains("User asked about something."));
        assert_eq!(context[1]["content"], "Recent message");
    }
}

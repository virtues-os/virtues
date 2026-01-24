//! Conversation Compaction Module
//!
//! Implements hierarchical summarization for context management.
//! Compresses older messages into a rolling summary while keeping
//! recent exchanges verbatim.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::time::Duration;
use tokio::time::timeout;

use crate::api::sessions::ChatMessage;
use crate::api::token_estimation::{estimate_session_context, ContextStatus};
use crate::error::Result;
use crate::llm::client::{LLMClient, LLMRequest, TollboothClient};

// ============================================================================
// Constants
// ============================================================================

// Note: Summarization model is now read from app_assistant_profile.background_model_id

/// Number of recent exchanges to keep verbatim (user + assistant pairs)
const DEFAULT_KEEP_RECENT_EXCHANGES: usize = 8;

/// Maximum tokens for summary generation
const SUMMARY_MAX_TOKENS: u32 = 1000;

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
    /// Number of recent exchanges to keep verbatim (default: 8)
    #[serde(default = "default_keep_recent")]
    pub keep_recent_exchanges: usize,
    /// Force compaction even if under threshold
    #[serde(default)]
    pub force: bool,
}

fn default_keep_recent() -> usize {
    DEFAULT_KEEP_RECENT_EXCHANGES
}

impl Default for CompactionOptions {
    fn default() -> Self {
        Self {
            keep_recent_exchanges: DEFAULT_KEEP_RECENT_EXCHANGES,
            force: false,
        }
    }
}

// ============================================================================
// Summary Generation
// ============================================================================

/// System prompt for summary generation
const SUMMARY_SYSTEM_PROMPT: &str = r#"You are a conversation summarizer. Create a concise context summary that will be injected into future messages to maintain conversation continuity.

PRESERVE:
- The user's goal and what they're trying to accomplish
- All technical decisions made (libraries, patterns, approaches)
- File paths, function names, and code locations mentioned
- User-stated preferences and constraints
- Any unresolved questions or pending tasks
- Key errors encountered and how they were resolved

FORMAT:
- Use bullet points for clarity
- Keep under 800 tokens
- Be factual and specific, not general
- Include concrete details (names, paths, values)

DO NOT include:
- Pleasantries or meta-commentary
- Redundant information
- Step-by-step recreation of the conversation"#;

/// Generate a summary of messages using the LLM
async fn generate_summary(
    client: &TollboothClient,
    messages: &[ChatMessage],
    existing_summary: Option<&str>,
    model: &str,
) -> Result<String> {
    // Build the content to summarize
    let mut content = String::new();

    // Include existing summary if present
    if let Some(summary) = existing_summary {
        content.push_str("=== PREVIOUS SUMMARY ===\n");
        content.push_str(summary);
        content.push_str("\n\n=== NEW MESSAGES TO INCORPORATE ===\n");
    }

    // Format messages for summarization
    for msg in messages {
        let role_label = match msg.role.as_str() {
            "user" => "User",
            "assistant" => "Assistant",
            "system" => "System",
            _ => &msg.role,
        };

        content.push_str(&format!("{}: {}\n\n", role_label, msg.content));

        // Include tool calls if present
        if let Some(tool_calls) = &msg.tool_calls {
            for tc in tool_calls {
                content.push_str(&format!("  [Tool: {}]\n", tc.tool_name));
            }
        }
    }

    // Generate summary
    let request = LLMRequest {
        model: model.to_string(),
        prompt: content,
        max_tokens: SUMMARY_MAX_TOKENS,
        temperature: SUMMARY_TEMPERATURE,
        system: Some(SUMMARY_SYSTEM_PROMPT.to_string()),
    };

    let response = client
        .generate(request)
        .await
        .map_err(|e| crate::Error::Other(format!("Summary generation failed: {}", e)))?;

    Ok(response.content)
}

// ============================================================================
// Compaction Logic
// ============================================================================

/// Compact a session by summarizing older messages
///
/// This function:
/// 1. Loads the session and its messages
/// 2. Determines which messages to summarize (all except recent N exchanges)
/// 3. Generates a summary incorporating the existing summary (if any)
/// 4. Updates the session with the new summary and metadata
pub async fn compact_session(
    pool: &SqlitePool,
    session_id: String,
    options: CompactionOptions,
) -> Result<CompactionResult> {
    let session_id_str = session_id.clone();

    // Load session metadata
    let session_row = sqlx::query!(
        r#"
        SELECT
            message_count,
            conversation_summary, summary_up_to_index, summary_version
        FROM app_chat_sessions
        WHERE id = $1
        "#,
        session_id_str
    )
    .fetch_optional(pool)
    .await?;

    let session_row =
        session_row.ok_or_else(|| crate::Error::NotFound("Session not found".into()))?;

    // Load messages from normalized table
    let message_rows = sqlx::query!(
        r#"
        SELECT
            id, role, content, created_at as timestamp,
            model, provider, agent_id, reasoning, tool_calls, intent, subject
        FROM app_chat_messages
        WHERE session_id = $1
        ORDER BY sequence_num ASC
        "#,
        session_id_str
    )
    .fetch_all(pool)
    .await?;
    
    let messages: Vec<ChatMessage> = message_rows
        .into_iter()
        .map(|row| {
            let tool_calls = row.tool_calls
                .as_ref()
                .and_then(|tc| serde_json::from_str(tc).ok());
            let intent = row.intent
                .as_ref()
                .and_then(|i| serde_json::from_str(i).ok());
            
            ChatMessage {
                id: row.id,
                role: row.role,
                content: row.content,
                timestamp: row.timestamp,
                model: row.model,
                provider: row.provider,
                agent_id: row.agent_id,
                reasoning: row.reasoning,
                tool_calls,
                intent,
                subject: row.subject,
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
            summary_version: session_row.summary_version.unwrap_or(0) as i32,
        });
    }

    // Determine the split point
    let split_index = if message_count > messages_to_keep {
        message_count - messages_to_keep
    } else {
        0
    };

    // Get the current summary index (messages already summarized)
    let current_summary_index = session_row.summary_up_to_index.unwrap_or(0) as usize;

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
            summary_version: session_row.summary_version.unwrap_or(0) as i32,
        });
    };

    // Create LLM client for summarization
    let client = TollboothClient::from_env()
        .map_err(|e| crate::Error::Other(format!("Failed to create LLM client: {}", e)))?;

    // Get background model from assistant profile
    let background_model = crate::api::assistant_profile::get_background_model(pool).await?;

    // Generate new summary with timeout to prevent hanging
    let new_summary = timeout(
        Duration::from_secs(60),
        generate_summary(
            &client,
            messages_to_summarize,
            session_row.conversation_summary.as_deref(),
            &background_model,
        ),
    )
    .await
    .map_err(|_| crate::Error::Other("Summary generation timed out after 60s".to_string()))??;

    // Calculate previous and new usage percentages
    let context_window = 1_000_000i64; // Default Gemini context window
    let previous_estimate = estimate_session_context(&messages, None, None, context_window);
    let verbatim_messages = &messages[split_index..];
    let new_estimate =
        estimate_session_context(verbatim_messages, Some(&new_summary), None, context_window);

    // Update the session
    let now = Utc::now().to_rfc3339();
    let new_version = session_row.summary_version.unwrap_or(0) + 1;
    let new_summary_index = split_index as i32;

    sqlx::query!(
        r#"
        UPDATE app_chat_sessions
        SET
            conversation_summary = $1,
            summary_up_to_index = $2,
            summary_version = $3,
            last_compacted_at = $4,
            updated_at = $4
        WHERE id = $5
        "#,
        new_summary,
        new_summary_index,
        new_version,
        now,
        session_id_str
    )
    .execute(pool)
    .await?;

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

/// Build the context to send to the LLM, using summary + recent messages
///
/// Returns a vector of messages in OpenAI format ready for the API.
pub fn build_context_for_llm(
    messages: &[ChatMessage],
    summary: Option<&str>,
    summary_up_to_index: usize,
    system_prompt: Option<&str>,
) -> Vec<serde_json::Value> {
    let mut context = Vec::new();

    // 1. System prompt
    if let Some(prompt) = system_prompt {
        context.push(serde_json::json!({
            "role": "system",
            "content": prompt
        }));
    }

    // 2. Conversation summary (if exists)
    if let Some(summary_text) = summary {
        context.push(serde_json::json!({
            "role": "system",
            "content": format!("Previous conversation context:\n{}", summary_text)
        }));
    }

    // 3. Recent messages (after summary_up_to_index)
    let recent_messages = if summary_up_to_index < messages.len() {
        &messages[summary_up_to_index..]
    } else {
        messages
    };

    for msg in recent_messages {
        context.push(serde_json::json!({
            "role": msg.role,
            "content": msg.content
        }));
    }

    context
}

/// Check if a session needs compaction based on context usage
pub async fn needs_compaction(
    pool: &SqlitePool,
    session_id: String,
    context_window: i64,
) -> Result<ContextStatus> {
    let session_id_str = session_id.clone();

    // Load session metadata
    let session_row = sqlx::query!(
        r#"
        SELECT conversation_summary, summary_up_to_index
        FROM app_chat_sessions
        WHERE id = $1
        "#,
        session_id_str
    )
    .fetch_optional(pool)
    .await?;

    let session_row =
        session_row.ok_or_else(|| crate::Error::NotFound("Session not found".into()))?;

    // Load messages from normalized table
    let message_rows = sqlx::query!(
        r#"
        SELECT
            id, role, content, created_at as timestamp,
            model, provider, agent_id, reasoning, tool_calls, intent, subject
        FROM app_chat_messages
        WHERE session_id = $1
        ORDER BY sequence_num ASC
        "#,
        session_id_str
    )
    .fetch_all(pool)
    .await?;
    
    let messages: Vec<ChatMessage> = message_rows
        .into_iter()
        .map(|row| {
            let tool_calls = row.tool_calls
                .as_ref()
                .and_then(|tc| serde_json::from_str(tc).ok());
            let intent = row.intent
                .as_ref()
                .and_then(|i| serde_json::from_str(i).ok());
            
            ChatMessage {
                id: row.id,
                role: row.role,
                content: row.content,
                timestamp: row.timestamp,
                model: row.model,
                provider: row.provider,
                agent_id: row.agent_id,
                reasoning: row.reasoning,
                tool_calls,
                intent,
                subject: row.subject,
            }
        })
        .collect();

    // Get verbatim messages (after summary)
    let summary_up_to_index = session_row.summary_up_to_index.unwrap_or(0) as usize;
    let verbatim_messages = if summary_up_to_index < messages.len() {
        &messages[summary_up_to_index..]
    } else {
        &messages[..]
    };

    // Estimate context with summary + verbatim messages
    let estimate = estimate_session_context(
        verbatim_messages,
        session_row.conversation_summary.as_deref(),
        None,
        context_window,
    );

    Ok(estimate.status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_context_without_summary() {
        let messages = vec![
            ChatMessage {
                id: None,
                role: "user".to_string(),
                content: "Hello".to_string(),
                timestamp: "2024-01-01T00:00:00Z".to_string(),
                model: None,
                provider: None,
                agent_id: None,
                tool_calls: None,
                reasoning: None,
                intent: None,
                subject: None,
            },
            ChatMessage {
                id: None,
                role: "assistant".to_string(),
                content: "Hi there!".to_string(),
                timestamp: "2024-01-01T00:00:01Z".to_string(),
                model: None,
                provider: None,
                agent_id: None,
                tool_calls: None,
                reasoning: None,
                intent: None,
                subject: None,
            },
        ];

        let context = build_context_for_llm(&messages, None, 0, Some("You are helpful."));

        assert_eq!(context.len(), 3); // system + 2 messages
        assert_eq!(context[0]["role"], "system");
        assert_eq!(context[1]["role"], "user");
        assert_eq!(context[2]["role"], "assistant");
    }

    #[test]
    fn test_build_context_with_summary() {
        let messages = vec![
            ChatMessage {
                id: None,
                role: "user".to_string(),
                content: "Old message".to_string(),
                timestamp: "2024-01-01T00:00:00Z".to_string(),
                model: None,
                provider: None,
                agent_id: None,
                tool_calls: None,
                reasoning: None,
                intent: None,
                subject: None,
            },
            ChatMessage {
                id: None,
                role: "assistant".to_string(),
                content: "Old response".to_string(),
                timestamp: "2024-01-01T00:00:01Z".to_string(),
                model: None,
                provider: None,
                agent_id: None,
                tool_calls: None,
                reasoning: None,
                intent: None,
                subject: None,
            },
            ChatMessage {
                id: None,
                role: "user".to_string(),
                content: "Recent message".to_string(),
                timestamp: "2024-01-01T00:00:02Z".to_string(),
                model: None,
                provider: None,
                agent_id: None,
                tool_calls: None,
                reasoning: None,
                intent: None,
                subject: None,
            },
        ];

        // summary_up_to_index = 2 means first 2 messages are summarized
        let context = build_context_for_llm(
            &messages,
            Some("User asked about something."),
            2,
            Some("You are helpful."),
        );

        // Should have: system prompt, summary, 1 recent message
        assert_eq!(context.len(), 3);
        assert!(context[1]["content"]
            .as_str()
            .unwrap()
            .contains("Previous conversation context"));
        assert_eq!(context[2]["content"], "Recent message");
    }
}

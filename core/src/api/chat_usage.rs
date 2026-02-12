//! Chat Usage Tracking Module
//!
//! Tracks token usage per chat for context management.
//! Provides cumulative token counts, cost estimation, and compaction status.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::api::models::{get_default_model, get_model};
use crate::api::chats::ChatMessage;
use crate::api::token_estimation::{estimate_session_context, ContextStatus};
use crate::error::Result;
use crate::types::Timestamp;

// ============================================================================
// Types
// ============================================================================

/// Token usage record for a chat-model pair
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatUsageRecord {
    pub id: String,
    pub chat_id: String,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub estimated_cost_usd: f64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Aggregated usage for a chat
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatUsageInfo {
    pub chat_id: String,
    pub model: String,
    pub context_window: i64,
    pub total_tokens: i64,
    pub usage_percentage: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub total_cost_usd: f64,
    pub user_message_count: i32,
    pub assistant_message_count: i32,
    pub first_message_at: Option<Timestamp>,
    pub last_message_at: Option<Timestamp>,
    pub compaction_status: CompactionStatus,
    pub context_status: String,
}

/// Compaction status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionStatus {
    pub summary_exists: bool,
    pub messages_summarized: i32,
    pub messages_verbatim: i32,
    pub summary_version: i32,
    pub last_compacted_at: Option<Timestamp>,
}

/// Usage data to record after an LLM response
#[derive(Debug, Clone)]
pub struct UsageData {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
}

// ============================================================================
// Cost Calculation
// ============================================================================

/// Default pricing per 1M tokens (fallback if model pricing not available)
const DEFAULT_INPUT_COST_PER_M: f64 = 0.15; // $0.15 per 1M input tokens
const DEFAULT_OUTPUT_COST_PER_M: f64 = 0.60; // $0.60 per 1M output tokens

/// Calculate estimated cost for token usage
///
/// Uses model-specific pricing if available, falls back to defaults.
/// Pricing is per 1M tokens.
pub fn calculate_cost(
    model: &str,
    input_tokens: i64,
    output_tokens: i64,
    _reasoning_tokens: i64,
) -> f64 {
    // Model-specific pricing (per 1M tokens)
    let (input_rate, output_rate) = match model {
        // Google models
        m if m.contains("gemini-2.5") => (1.25, 10.0), // Gemini 2.5 Pro
        m if m.contains("gemini-3-flash") || m.contains("gemini-3.5") => (0.075, 0.30), // Flash models
        m if m.contains("gemini") => (0.15, 0.60), // Other Gemini models

        // Anthropic models
        m if m.contains("claude-3-5-sonnet") || m.contains("claude-sonnet-4") => (3.0, 15.0),
        m if m.contains("claude-3-5-haiku") || m.contains("claude-haiku") => (0.80, 4.0),
        m if m.contains("claude-opus") => (15.0, 75.0),
        m if m.contains("claude") => (3.0, 15.0), // Default Claude pricing

        // OpenAI models
        m if m.contains("gpt-4o-mini") => (0.15, 0.60),
        m if m.contains("gpt-4o") => (2.50, 10.0),
        m if m.contains("gpt-4-turbo") => (10.0, 30.0),
        m if m.contains("gpt-4") => (30.0, 60.0),
        m if m.contains("o1") => (15.0, 60.0), // o1 reasoning models
        m if m.contains("gpt-3.5") => (0.50, 1.50),

        // Default fallback
        _ => (DEFAULT_INPUT_COST_PER_M, DEFAULT_OUTPUT_COST_PER_M),
    };

    let input_cost = (input_tokens as f64 / 1_000_000.0) * input_rate;
    let output_cost = (output_tokens as f64 / 1_000_000.0) * output_rate;

    input_cost + output_cost
}

// ============================================================================
// Database Operations
// ============================================================================

/// Record token usage after an LLM response
///
/// This uses upsert to accumulate usage per chat-model pair.
pub async fn record_chat_usage(
    pool: &SqlitePool,
    chat_id: String,
    model: &str,
    usage: UsageData,
) -> Result<()> {
    let chat_id_str = chat_id.clone();
    let id = format!("{}_{}", chat_id_str, model.replace('/', "_"));
    let now = Timestamp::now();

    let cost = calculate_cost(
        model,
        usage.input_tokens,
        usage.output_tokens,
        usage.reasoning_tokens,
    );

    // Upsert: increment existing or insert new
    sqlx::query(
        r#"
        INSERT INTO app_chat_usage (
            id, chat_id, model,
            input_tokens, output_tokens, reasoning_tokens,
            cache_read_tokens, cache_write_tokens,
            estimated_cost_usd, created_at, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT (chat_id, model) DO UPDATE SET
            input_tokens = app_chat_usage.input_tokens + excluded.input_tokens,
            output_tokens = app_chat_usage.output_tokens + excluded.output_tokens,
            reasoning_tokens = app_chat_usage.reasoning_tokens + excluded.reasoning_tokens,
            cache_read_tokens = app_chat_usage.cache_read_tokens + excluded.cache_read_tokens,
            cache_write_tokens = app_chat_usage.cache_write_tokens + excluded.cache_write_tokens,
            estimated_cost_usd = app_chat_usage.estimated_cost_usd + excluded.estimated_cost_usd,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(id)
    .bind(chat_id_str)
    .bind(model)
    .bind(usage.input_tokens)
    .bind(usage.output_tokens)
    .bind(usage.reasoning_tokens)
    .bind(usage.cache_read_tokens)
    .bind(usage.cache_write_tokens)
    .bind(cost)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(())
}

/// Get cumulative usage for a chat
pub async fn get_chat_usage(pool: &SqlitePool, chat_id: String) -> Result<ChatUsageInfo> {
    let chat_id_str = chat_id.clone();

    // Get chat metadata
    let chat_row = sqlx::query(
        r#"
        SELECT
            id, title, message_count,
            conversation_summary, summary_up_to_index, summary_version, last_compacted_at,
            created_at, updated_at
        FROM app_chats
        WHERE id = ?
        "#,
    )
    .bind(&chat_id_str)
    .fetch_optional(pool)
    .await?;

    let chat_row =
        chat_row.ok_or_else(|| crate::Error::NotFound("Chat not found".into()))?;

    use sqlx::Row;
    let message_count: i32 = chat_row.get("message_count");
    let conversation_summary: Option<String> = chat_row.get("conversation_summary");
    let summary_up_to_index: i32 = chat_row.get("summary_up_to_index");
    let summary_version: i32 = chat_row.get("summary_version");
    let last_compacted_at: Option<String> = chat_row.get("last_compacted_at");

    // Load messages from normalized table
    let message_rows = sqlx::query(
        r#"
        SELECT
            id, role, content, created_at as timestamp,
            model, provider, agent_id, reasoning, tool_calls, intent, subject, thought_signature
        FROM app_chat_messages
        WHERE chat_id = ?
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
            let tool_calls_raw: Option<String> = row.get("tool_calls");
            let intent_raw: Option<String> = row.get("intent");
            let subject: Option<String> = row.get("subject");
            let thought_signature: Option<String> = row.get("thought_signature");

            let tool_calls = tool_calls_raw
                .and_then(|tc| serde_json::from_str(&tc).ok());
            let intent = intent_raw
                .and_then(|i| serde_json::from_str(&i).ok());

            ChatMessage {
                id: Some(id),
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
                thought_signature,
                parts: None,
            }
        })
        .collect();

    // Get aggregated usage from chat_usage
    let usage_row = sqlx::query(
        r#"
        SELECT
            COALESCE(SUM(input_tokens), 0) as "input_tokens",
            COALESCE(SUM(output_tokens), 0) as "output_tokens",
            COALESCE(SUM(reasoning_tokens), 0) as "reasoning_tokens",
            COALESCE(SUM(cache_read_tokens), 0) as "cache_read_tokens",
            COALESCE(SUM(cache_write_tokens), 0) as "cache_write_tokens",
            COALESCE(SUM(estimated_cost_usd), 0.0) as "total_cost",
            model
        FROM app_chat_usage
        WHERE chat_id = ?
        GROUP BY chat_id
        "#,
    )
    .bind(&chat_id_str)
    .fetch_optional(pool)
    .await?;

    // Get the most recently used model, falling back to registry default
    let last_model = match messages.iter().rev().find_map(|m| m.model.clone()) {
        Some(model) => model,
        None => get_default_model()
            .await
            .map(|m| m.model_id)
            .unwrap_or_else(|_| "anthropic/claude-sonnet-4-20250514".to_string()),
    };

    // Get model context window from registry
    let context_window = match get_model(&last_model).await {
        Ok(model_info) => model_info.context_window.unwrap_or(1_000_000) as i64,
        Err(_) => 1_000_000, // Default 1M for Gemini
    };

    // Calculate message counts
    let user_message_count = messages.iter().filter(|m| m.role == "user").count() as i32;
    let assistant_message_count = messages.iter().filter(|m| m.role == "assistant").count() as i32;

    // Get timestamps
    let first_message_at = messages.first().map(|m| m.timestamp);
    let last_message_at = messages.last().map(|m| m.timestamp);

    // Parse compaction info
    let summary_exists = conversation_summary.is_some();

    let messages_summarized = if summary_exists { summary_up_to_index } else { 0 };
    let messages_verbatim = message_count - messages_summarized;

    // Calculate context estimate
    let estimate = estimate_session_context(
        &messages,
        conversation_summary.as_deref(),
        None, // System prompt not stored in chat
        context_window,
    );

    // Use recorded usage if available, otherwise estimate from messages
    let (input_tokens, output_tokens, reasoning_tokens, cache_read, cache_write, total_cost) =
        if let Some(usage) = usage_row {
            use sqlx::Row;
            (
                usage.get("input_tokens"),
                usage.get("output_tokens"),
                usage.get("reasoning_tokens"),
                usage.get("cache_read_tokens"),
                usage.get("cache_write_tokens"),
                usage.get("total_cost"),
            )
        } else {
            // Estimate from messages if no recorded usage
            (estimate.total_tokens / 2, estimate.total_tokens / 2, 0, 0, 0, 0.0)
        };

    let total_tokens = input_tokens + output_tokens;
    let usage_percentage = (total_tokens as f64 / context_window as f64) * 100.0;

    let context_status = if usage_percentage >= 85.0 {
        ContextStatus::Critical
    } else if usage_percentage >= 70.0 {
        ContextStatus::Warning
    } else {
        ContextStatus::Healthy
    };

    Ok(ChatUsageInfo {
        chat_id: chat_id_str,
        model: last_model,
        context_window,
        total_tokens,
        usage_percentage,
        input_tokens,
        output_tokens,
        reasoning_tokens,
        cache_read_tokens: cache_read,
        cache_write_tokens: cache_write,
        total_cost_usd: total_cost,
        user_message_count,
        assistant_message_count,
        first_message_at,
        last_message_at,
        compaction_status: CompactionStatus {
            summary_exists,
            messages_summarized,
            messages_verbatim,
            summary_version,
            last_compacted_at: last_compacted_at.and_then(|s| s.parse::<Timestamp>().ok()),
        },
        context_status: context_status.as_str().to_string(),
    })
}

/// Check if compaction is needed for a chat
pub async fn check_compaction_needed(
    pool: &SqlitePool,
    chat_id: String,
    model: &str,
) -> Result<ContextStatus> {
    let chat_id_str = chat_id.clone();

    // Get chat metadata
    let chat_row = sqlx::query(
        r#"
        SELECT conversation_summary, summary_up_to_index
        FROM app_chats
        WHERE id = ?
        "#,
    )
    .bind(&chat_id_str)
    .fetch_optional(pool)
    .await?;

    let chat_row =
        chat_row.ok_or_else(|| crate::Error::NotFound("Chat not found".into()))?;

    use sqlx::Row;
    let conversation_summary: Option<String> = chat_row.get("conversation_summary");
    let summary_up_to_index: i32 = chat_row.get("summary_up_to_index");

    // Load messages from normalized table
    let message_rows = sqlx::query(
        r#"
        SELECT
            id, role, content, created_at as timestamp,
            model, provider, agent_id, reasoning, tool_calls, intent, subject, thought_signature
        FROM app_chat_messages
        WHERE chat_id = ?
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
            let tool_calls_raw: Option<String> = row.get("tool_calls");
            let intent_raw: Option<String> = row.get("intent");
            let subject: Option<String> = row.get("subject");
            let thought_signature: Option<String> = row.get("thought_signature");

            let tool_calls = tool_calls_raw
                .and_then(|tc| serde_json::from_str(&tc).ok());
            let intent = intent_raw
                .and_then(|i| serde_json::from_str(&i).ok());

            ChatMessage {
                id: Some(id),
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
                thought_signature,
                parts: None,
            }
        })
        .collect();

    // Get model context window from registry
    let context_window = match get_model(model).await {
        Ok(model_info) => model_info.context_window.unwrap_or(1_000_000) as i64,
        Err(_) => 1_000_000,
    };

    // Get verbatim messages (after summary)
    let verbatim_messages = if (summary_up_to_index as usize) < messages.len() {
        &messages[(summary_up_to_index as usize)..]
    } else {
        &messages[..]
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
    use super::*;

    #[test]
    fn test_calculate_cost_gemini() {
        // Gemini 2.5 Flash: $1.25/1M input, $10/1M output
        let cost = calculate_cost("gemini-2.5-flash-preview-05-06", 1_000_000, 1_000_000, 0);
        assert!((cost - 11.25).abs() < 0.001); // 1.25 + 10 = 11.25
    }

    #[test]
    fn test_calculate_cost_claude() {
        // Claude Sonnet 4: $3/1M input, $15/1M output
        let cost = calculate_cost("claude-sonnet-4-20250514", 1_000_000, 1_000_000, 0);
        assert!((cost - 18.0).abs() < 0.001); // 3 + 15 = 18
    }

    #[test]
    fn test_calculate_cost_small_usage() {
        // 1000 tokens each with gpt-4o-mini
        let cost = calculate_cost("gpt-4o-mini", 1000, 1000, 0);
        let expected = (1000.0 / 1_000_000.0) * 0.15 + (1000.0 / 1_000_000.0) * 0.60;
        assert!((cost - expected).abs() < 0.0001);
    }
}

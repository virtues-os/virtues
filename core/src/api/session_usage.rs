//! Session Usage Tracking Module
//!
//! Tracks token usage per chat session for context management.
//! Provides cumulative token counts, cost estimation, and compaction status.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::api::models::{get_default_model, get_model};
use crate::api::sessions::ChatMessage;
use crate::api::token_estimation::{estimate_session_context, ContextStatus};
use crate::error::Result;
use crate::types::Timestamp;

// ============================================================================
// Types
// ============================================================================

/// Token usage record for a session-model pair
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionUsageRecord {
    pub id: String,
    pub session_id: String,
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

/// Aggregated usage for a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionUsage {
    pub session_id: String,
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
/// This uses upsert to accumulate usage per session-model pair.
pub async fn record_session_usage(
    pool: &SqlitePool,
    session_id: String,
    model: &str,
    usage: UsageData,
) -> Result<()> {
    let session_id_str = session_id.clone();
    let id = format!("{}_{}", session_id_str, model.replace('/', "_"));
    let now = Utc::now().to_rfc3339();

    let cost = calculate_cost(
        model,
        usage.input_tokens,
        usage.output_tokens,
        usage.reasoning_tokens,
    );

    // Upsert: increment existing or insert new
    sqlx::query!(
        r#"
        INSERT INTO app_session_usage (
            id, session_id, model,
            input_tokens, output_tokens, reasoning_tokens,
            cache_read_tokens, cache_write_tokens,
            estimated_cost_usd, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)
        ON CONFLICT (session_id, model) DO UPDATE SET
            input_tokens = app_session_usage.input_tokens + excluded.input_tokens,
            output_tokens = app_session_usage.output_tokens + excluded.output_tokens,
            reasoning_tokens = app_session_usage.reasoning_tokens + excluded.reasoning_tokens,
            cache_read_tokens = app_session_usage.cache_read_tokens + excluded.cache_read_tokens,
            cache_write_tokens = app_session_usage.cache_write_tokens + excluded.cache_write_tokens,
            estimated_cost_usd = app_session_usage.estimated_cost_usd + excluded.estimated_cost_usd,
            updated_at = excluded.updated_at
        "#,
        id,
        session_id_str,
        model,
        usage.input_tokens,
        usage.output_tokens,
        usage.reasoning_tokens,
        usage.cache_read_tokens,
        usage.cache_write_tokens,
        cost,
        now
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Get cumulative usage for a session
pub async fn get_session_usage(pool: &SqlitePool, session_id: String) -> Result<SessionUsage> {
    let session_id_str = session_id.clone();

    // Get session metadata
    let session_row = sqlx::query!(
        r#"
        SELECT
            id, title, message_count,
            conversation_summary, summary_up_to_index, summary_version, last_compacted_at,
            created_at, updated_at
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

    // Get aggregated usage from app_session_usage
    let usage_row = sqlx::query!(
        r#"
        SELECT
            COALESCE(SUM(input_tokens), 0) as "input_tokens!: i64",
            COALESCE(SUM(output_tokens), 0) as "output_tokens!: i64",
            COALESCE(SUM(reasoning_tokens), 0) as "reasoning_tokens!: i64",
            COALESCE(SUM(cache_read_tokens), 0) as "cache_read_tokens!: i64",
            COALESCE(SUM(cache_write_tokens), 0) as "cache_write_tokens!: i64",
            COALESCE(SUM(estimated_cost_usd), 0.0) as "total_cost!: f64",
            model
        FROM app_session_usage
        WHERE session_id = $1
        GROUP BY session_id
        "#,
        session_id_str
    )
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
    let first_message_at = messages.first().and_then(|m| m.timestamp.parse::<Timestamp>().ok());
    let last_message_at = messages.last().and_then(|m| m.timestamp.parse::<Timestamp>().ok());

    // Parse compaction info
    let summary_up_to_index = session_row.summary_up_to_index.unwrap_or(0) as i32;
    let summary_version = session_row.summary_version.unwrap_or(0) as i32;
    let summary_exists = session_row.conversation_summary.is_some();

    let messages_summarized = if summary_exists { summary_up_to_index } else { 0 };
    let messages_verbatim = (session_row.message_count as i32) - messages_summarized;

    // Calculate context estimate
    let estimate = estimate_session_context(
        &messages,
        session_row.conversation_summary.as_deref(),
        None, // System prompt not stored in session
        context_window,
    );

    // Use recorded usage if available, otherwise estimate from messages
    let (input_tokens, output_tokens, reasoning_tokens, cache_read, cache_write, total_cost) =
        if let Some(usage) = usage_row {
            (
                usage.input_tokens,
                usage.output_tokens,
                usage.reasoning_tokens,
                usage.cache_read_tokens,
                usage.cache_write_tokens,
                usage.total_cost,
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

    Ok(SessionUsage {
        session_id: session_id_str,
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
            last_compacted_at: session_row.last_compacted_at.and_then(|s| s.parse::<Timestamp>().ok()),
        },
        context_status: context_status.as_str().to_string(),
    })
}

/// Check if compaction is needed for a session
pub async fn check_compaction_needed(
    pool: &SqlitePool,
    session_id: String,
    model: &str,
) -> Result<ContextStatus> {
    let session_id_str = session_id.clone();

    // Get session metadata
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

    // Get model context window from registry
    let context_window = match get_model(model).await {
        Ok(model_info) => model_info.context_window.unwrap_or(1_000_000) as i64,
        Err(_) => 1_000_000,
    };

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

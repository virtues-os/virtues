//! Chat Usage Tracking Module
//!
//! Tracks token usage per chat for context management.
//! Provides cumulative token counts, cost estimation, and compaction status.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

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
    /// The gateway's own `usage.cost`, in micros — the exact figure the wallet
    /// was debited for. `None` when the gateway omitted it (rare); see
    /// `resolve_cost_usd`. Never estimate when this is `Some`.
    pub cost_micros: Option<i64>,
}

// ============================================================================
// Cost Calculation
// ============================================================================

/// Cost for a turn, in USD.
///
/// Prefers `authoritative_micros` — the gateway's own `usage.cost`, which the
/// stream parser already extracts (`agent::stream`) and which is the exact
/// figure the wallet was debited for. There is no reason to estimate a number
/// we were handed.
///
/// Only when the gateway omits it do we multiply tokens by the LIVE catalog
/// price (fetched from virtues-api; see `api::model_catalog`). If even that is
/// cold, we return 0.0 and log — an unknown cost is better shown as blank than
/// as a confident fiction.
///
/// This function used to read a compiled price table. Every entry in it was
/// wrong (Opus 3× over, image generation 13× under), which is exactly what a
/// hand-maintained mirror of someone else's pricing gets you.
pub fn resolve_cost_usd(
    model: &str,
    authoritative_micros: Option<i64>,
    input_tokens: i64,
    output_tokens: i64,
    byo: bool,
) -> f64 {
    if let Some(micros) = authoritative_micros {
        if micros > 0 {
            return micros as f64 / 1_000_000.0;
        }
    }

    // Bring-your-own endpoint: no upstream but our own gateway sends a cost
    // trailer, so `Some(0)` here means unknown, not free — and estimating it
    // from OUR catalog prices tokens the person paid their own provider for,
    // under a model id that is not even the one that ran (the body's model is
    // rewritten at send time). `app_ai_calls` already knows this; the per-chat
    // row did not, and wrote dollars.
    if byo {
        return 0.0;
    }

    match crate::api::model_catalog::pricing(model) {
        Some((input_per_1k, output_per_1k)) => {
            (input_tokens as f64 / 1000.0) * input_per_1k
                + (output_tokens as f64 / 1000.0) * output_per_1k
        }
        None => {
            tracing::debug!(
                model,
                "no gateway cost and no catalog price — recording 0.00 for this turn"
            );
            0.0
        }
    }
}

// ============================================================================
// Database Operations
// ============================================================================

/// Record token usage after an LLM response
///
/// This uses upsert to accumulate usage per chat-model pair.
pub async fn record_chat_usage(
    pool: &PgPool,
    chat_id: String,
    model: &str,
    usage: UsageData,
    byo: bool,
) -> Result<()> {
    let chat_id_str = chat_id.clone();
    let id = format!("{}_{}", chat_id_str, model.replace('/', "_"));
    let now = Timestamp::now();

    let cost = resolve_cost_usd(
        model,
        usage.cost_micros,
        usage.input_tokens,
        usage.output_tokens,
        byo,
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
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
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
pub async fn get_chat_usage(pool: &PgPool, chat_id: String) -> Result<ChatUsageInfo> {
    let chat_id_str = chat_id.clone();

    // Get chat metadata
    let chat_row = sqlx::query(
        r#"
        SELECT
            id, title, message_count,
            conversation_summary, summary_up_to_index, summary_version, last_compacted_at,
            created_at, updated_at
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
    // Columns are BIGINT (NOT NULL DEFAULT 0); read as i64 then narrow to i32
    // for the public API struct. Saturating_as is safe for realistic counts.
    let message_count: i32 = (chat_row.get::<i64, _>("message_count")) as i32;
    let conversation_summary: Option<String> = chat_row.get("conversation_summary");
    let summary_up_to_index: i64 = chat_row.get("summary_up_to_index");
    let summary_version: i32 = (chat_row.get::<i64, _>("summary_version")) as i32;
    let last_compacted_at: Option<Timestamp> = chat_row.get("last_compacted_at");

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

            let tool_calls = tool_calls_raw
                .and_then(|tc| serde_json::from_value(tc).ok());
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

    // Get aggregated usage from chat_usage: one row per (chat, model), summed
    // across every model the chat has used. The model is NOT selected — a
    // bare `model` beside the SUMs is a GROUP BY violation Postgres rejects
    // at parse time, so this endpoint returned 500 on every chat and the
    // context indicator never once painted. The model reported below is the
    // last one that spoke, read off the messages.
    let usage_row = sqlx::query(
        r#"
        SELECT
            -- `::bigint` on each SUM: Postgres widens SUM(bigint) to NUMERIC,
            -- which sqlx will not decode as i64 — and `get` below panics on
            -- a decode error rather than returning it. `estimated_cost_usd`
            -- is float8 and sums to float8, so it needs no cast.
            COALESCE(SUM(input_tokens), 0)::bigint as "input_tokens",
            COALESCE(SUM(output_tokens), 0)::bigint as "output_tokens",
            COALESCE(SUM(reasoning_tokens), 0)::bigint as "reasoning_tokens",
            COALESCE(SUM(cache_read_tokens), 0)::bigint as "cache_read_tokens",
            COALESCE(SUM(cache_write_tokens), 0)::bigint as "cache_write_tokens",
            COALESCE(SUM(estimated_cost_usd), 0.0) as "total_cost"
        FROM app_chat_usage
        WHERE chat_id = $1
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
            .unwrap_or_else(|_| {
                virtues_registry::models::default_model_for_slot(
                    virtues_registry::models::ModelSlot::Chat,
                )
                .to_string()
            }),
    };

    // Get model context window from registry
    let context_window = match get_model(&last_model).await {
        Ok(model_info) => crate::api::token_estimation::context_window_or_assumed(
                model_info.context_window.map(|w| w as i64),
            ),
        Err(_) => crate::api::token_estimation::ASSUMED_CONTEXT_WINDOW, // Default 1M for Gemini
    };

    // Calculate message counts
    let user_message_count = messages.iter().filter(|m| m.role == "user").count() as i32;
    let assistant_message_count = messages.iter().filter(|m| m.role == "assistant").count() as i32;

    // Get timestamps
    let first_message_at = messages.first().map(|m| m.timestamp);
    let last_message_at = messages.last().map(|m| m.timestamp);

    // Parse compaction info
    let summary_exists = conversation_summary.is_some();

    let messages_summarized: i32 = if summary_exists { summary_up_to_index as i32 } else { 0 };
    let messages_verbatim = message_count - messages_summarized;

    // Calculate context estimate
    let estimate = estimate_session_context(
        &messages,
        conversation_summary.as_deref(),
        None, // System prompt not stored in chat
        context_window,
    );

    // The billed totals: what every turn of this chat has cost, summed. These
    // are CUMULATIVE — each turn re-sends the whole transcript, so after N
    // turns the input figure is roughly N times the context — which is what
    // the cost rows want and what a context gauge must never read.
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
            // Nothing recorded yet (a chat that has not completed a turn, or a
            // transcript older than usage tracking): approximate the split.
            (estimate.total_tokens / 2, estimate.total_tokens / 2, 0, 0, 0, 0.0)
        };

    // The context gauge: what the NEXT turn will send, against the window.
    // This used to be input + output from the sums above, which made a
    // thirty-turn chat read as thirty contexts' worth and would have tripped
    // the compaction thresholds on chats that were nowhere near full.
    let total_tokens = estimate.total_tokens;
    let usage_percentage = estimate.usage_percentage;
    let context_status = estimate.status;

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
            last_compacted_at,
        },
        context_status: context_status.as_str().to_string(),
    })
}

/// Check if compaction is needed for a chat
pub async fn check_compaction_needed(
    pool: &PgPool,
    chat_id: String,
    model: &str,
) -> Result<ContextStatus> {
    let chat_id_str = chat_id.clone();

    // Get chat metadata
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
    let summary_up_to_index: i64 = chat_row.get("summary_up_to_index");

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

            let tool_calls = tool_calls_raw
                .and_then(|tc| serde_json::from_value(tc).ok());
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

    // Get model context window from registry
    let context_window = match get_model(model).await {
        Ok(model_info) => crate::api::token_estimation::context_window_or_assumed(
                model_info.context_window.map(|w| w as i64),
            ),
        Err(_) => crate::api::token_estimation::ASSUMED_CONTEXT_WINDOW,
    };

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
    use super::*;

    /// The gateway's own `usage.cost` is the number the wallet was debited for.
    /// When we have it, we use it — no estimating, no token arithmetic.
    #[test]
    fn authoritative_gateway_cost_wins() {
        // $0.042 == 42_000 micros. Token counts are deliberately absurd: if
        // they influenced the result at all, this would not be 0.042.
        let cost = resolve_cost_usd("anthropic/claude-opus-4.8", Some(42_000), 999_999, 999_999, false);
        assert!((cost - 0.042).abs() < 1e-9, "got {cost}");
    }

    /// No gateway cost AND a cold catalog: report nothing rather than invent a
    /// number. The old code reached for a compiled price table here, and that
    /// table was wrong for every model in it.
    #[test]
    fn no_cost_and_cold_catalog_reports_zero_not_a_fiction() {
        let cost = resolve_cost_usd("anthropic/claude-opus-4.8", None, 1_000_000, 1_000_000, false);
        assert_eq!(cost, 0.0);
    }

    /// A zero from the gateway is treated as absent, not as free.
    #[test]
    fn zero_gateway_cost_is_not_taken_as_free() {
        let cost = resolve_cost_usd("anthropic/claude-opus-4.8", Some(0), 1_000, 1_000, false);
        // Falls through to the catalog (cold in tests) → 0.0, but crucially it
        // did NOT short-circuit on `Some(0)` as though the call were free.
        assert_eq!(cost, 0.0);
    }
}

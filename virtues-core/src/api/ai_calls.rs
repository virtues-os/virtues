//! Box-local per-call AI cost log (`app_ai_calls`).
//!
//! Virtues collects no central telemetry. The cloud wallet (virtues-api ledger)
//! is the authoritative money truth, but it has no per-call breakdown the user
//! can see on their own box. This module records one row per paid AI call with
//! the AUTHORITATIVE `usage.cost` the gateway returns — never re-estimated — so
//! the Usage tab can show "where did my money go" and the Telemetry tab can show
//! the AI-call log.
//!
//! METADATA ONLY: feature bucket, model, token counts, cost. Never prompt or
//! response content. No egress — this table lives only on the user's box.

use serde::Serialize;
use sqlx::PgPool;

/// One paid AI call to record. Cost is micros-USD from the gateway `usage.cost`.
#[derive(Debug, Clone, Default)]
pub struct AiCall {
    /// Coarse bucket: chat | transcription | search | embedding | agent.
    pub feature: &'static str,
    pub model: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub reasoning_tokens: i64,
    pub cost_micros: i64,
    pub chat_id: Option<String>,
    pub action_run_id: Option<String>,
}

/// A summary row for the Usage tab — spend grouped by feature or model.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AiSpendBucket {
    pub label: String,
    pub calls: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub cost_micros: i64,
}

/// Insert one call row. Best-effort: a logging failure must never break the
/// user-facing request, so callers log-and-continue on error.
pub async fn record_ai_call(pool: &PgPool, call: &AiCall) -> Result<(), sqlx::Error> {
    let id = format!("aic_{}", uuid::Uuid::new_v4());
    sqlx::query(
        r#"
        INSERT INTO app_ai_calls
            (id, feature, model, prompt_tokens, completion_tokens,
             reasoning_tokens, cost_micros, chat_id, action_run_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
    )
    .bind(id)
    .bind(call.feature)
    .bind(&call.model)
    .bind(call.prompt_tokens)
    .bind(call.completion_tokens)
    .bind(call.reasoning_tokens)
    .bind(call.cost_micros)
    .bind(&call.chat_id)
    .bind(&call.action_run_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Spend grouped by feature since `month_start` (a SQL timestamp boundary),
/// highest cost first. Drives the Usage tab breakdown.
pub async fn spend_by_feature(
    pool: &PgPool,
    since: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<AiSpendBucket>, sqlx::Error> {
    sqlx::query_as::<_, AiSpendBucket>(
        r#"
        SELECT COALESCE(feature, 'other') AS label,
               COUNT(*)                    AS calls,
               COALESCE(SUM(prompt_tokens), 0)     AS prompt_tokens,
               COALESCE(SUM(completion_tokens), 0) AS completion_tokens,
               COALESCE(SUM(cost_micros), 0)       AS cost_micros
        FROM app_ai_calls
        WHERE created_at >= $1
        GROUP BY COALESCE(feature, 'other')
        ORDER BY cost_micros DESC
        "#,
    )
    .bind(since)
    .fetch_all(pool)
    .await
}

/// Spend grouped by model since `since`, highest cost first.
pub async fn spend_by_model(
    pool: &PgPool,
    since: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<AiSpendBucket>, sqlx::Error> {
    sqlx::query_as::<_, AiSpendBucket>(
        r#"
        SELECT COALESCE(model, 'unknown') AS label,
               COUNT(*)                    AS calls,
               COALESCE(SUM(prompt_tokens), 0)     AS prompt_tokens,
               COALESCE(SUM(completion_tokens), 0) AS completion_tokens,
               COALESCE(SUM(cost_micros), 0)       AS cost_micros
        FROM app_ai_calls
        WHERE created_at >= $1
        GROUP BY COALESCE(model, 'unknown')
        ORDER BY cost_micros DESC
        "#,
    )
    .bind(since)
    .fetch_all(pool)
    .await
}

/// Recent individual calls (for the Telemetry tab AI-call log).
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AiCallRow {
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub feature: Option<String>,
    pub model: Option<String>,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub reasoning_tokens: i64,
    pub cost_micros: i64,
    pub status: String,
}

/// The most recent `limit` calls, newest first.
pub async fn recent_calls(pool: &PgPool, limit: i64) -> Result<Vec<AiCallRow>, sqlx::Error> {
    sqlx::query_as::<_, AiCallRow>(
        r#"
        SELECT created_at, feature, model, prompt_tokens, completion_tokens,
               reasoning_tokens, cost_micros, status
        FROM app_ai_calls
        ORDER BY created_at DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
}

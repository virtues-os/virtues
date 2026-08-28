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
    /// Coarse bucket: chat | council | deep_research | transcription |
    /// compaction | day_summary | … (the calling feature, or the client's
    /// purpose tag for callers that don't set one).
    pub feature: String,
    pub model: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub reasoning_tokens: i64,
    /// Micros-USD from the gateway's `usage.cost`. **Meaningful only when
    /// `route == "wallet"`.** No upstream but our own gateway reports a price,
    /// so a BYO row lands at 0, and 0 there means unknown — never free.
    pub cost_micros: i64,
    /// Which purse paid: `wallet` or `byo`. See [`Route`].
    pub route: Route,
    pub applet_run_id: Option<String>,
}

/// Which purse paid for a call.
///
/// The distinction exists because cost is only knowable on one side of it.
/// Summing `cost_micros` across both would report BYO traffic as free spending
/// on the wallet, which is the specific lie this column prevents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Route {
    /// Through virtues-api. `usage.cost` is authoritative.
    #[default]
    Wallet,
    /// Direct to the user's own endpoint. Tokens are known; cost is not.
    Byo,
}

impl Route {
    pub fn as_str(self) -> &'static str {
        match self {
            Route::Wallet => "wallet",
            Route::Byo => "byo",
        }
    }
}

/// A summary row for the Usage tab — spend grouped by feature or model.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AiSpendBucket {
    pub label: String,
    pub calls: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    /// Wallet spend only. BYO calls contribute tokens to this bucket but never
    /// money, because their price is unknown to us — see [`Route`].
    pub cost_micros: i64,
    /// How many of `calls` went out on the user's own key. When this equals
    /// `calls`, `cost_micros` is 0 because we cannot know it, and the UI must
    /// say so rather than render "$0.00".
    pub byo_calls: i64,
}

/// Insert one call row. Best-effort: a logging failure must never break the
/// user-facing request, so callers log-and-continue on error.
pub async fn record_ai_call(pool: &PgPool, call: &AiCall) -> Result<(), sqlx::Error> {
    let id = format!("aic_{}", uuid::Uuid::new_v4());
    sqlx::query(
        r#"
        INSERT INTO app_ai_calls
            (id, feature, model, prompt_tokens, completion_tokens,
             reasoning_tokens, cost_micros, route, applet_run_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
    )
    .bind(id)
    .bind(&call.feature)
    .bind(&call.model)
    .bind(call.prompt_tokens)
    .bind(call.completion_tokens)
    .bind(call.reasoning_tokens)
    .bind(call.cost_micros)
    .bind(call.route.as_str())
    .bind(&call.applet_run_id)
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
               -- Wallet rows only. Summing BYO cost would report someone
               -- else's bill as Virtues spend, at a value we invented.
               COALESCE(SUM(cost_micros) FILTER (WHERE route = 'wallet'), 0) AS cost_micros,
               COUNT(*) FILTER (WHERE route = 'byo')                         AS byo_calls
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
               -- Wallet rows only. Summing BYO cost would report someone
               -- else's bill as Virtues spend, at a value we invented.
               COALESCE(SUM(cost_micros) FILTER (WHERE route = 'wallet'), 0) AS cost_micros,
               COUNT(*) FILTER (WHERE route = 'byo')                         AS byo_calls
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

/// Recent individual calls (for the Usage page's AI-call log).
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AiCallRow {
    /// Stable row id, so the grid can key on it. Two calls in the same
    /// millisecond to the same model are distinct rows, and keying a paged
    /// grid on a timestamp makes them collide.
    pub id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub feature: Option<String>,
    pub model: Option<String>,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub reasoning_tokens: i64,
    pub cost_micros: i64,
    /// `wallet` | `byo`. On a byo row `cost_micros` is 0-as-unknown, and the
    /// grid shows the token counts instead of a price.
    pub route: String,
}

/// Query for one page of the call log.
#[derive(Debug, serde::Deserialize)]
pub struct AiCallsQuery {
    #[serde(default)]
    pub offset: i64,
    #[serde(default = "default_calls_limit")]
    pub limit: i64,
    /// Matched against feature and model.
    #[serde(default)]
    pub search: Option<String>,
    /// `asc` sorts oldest-first; anything else is newest-first.
    #[serde(default)]
    pub dir: Option<String>,
}

fn default_calls_limit() -> i64 {
    50
}

/// One page of the call log, plus the total the query matches.
#[derive(Debug, Serialize)]
pub struct AiCallPage {
    pub items: Vec<AiCallRow>,
    pub total: i64,
}

/// One page of calls, newest first by default.
///
/// Paged rather than a fixed `LIMIT 100`: the log is the only window onto a
/// runaway (an applet burning the wallet in a loop writes thousands of rows in
/// an hour), and a page that silently stops at 100 hides exactly the case it
/// exists to catch — while shipping every row to the browser would be worse.
pub async fn list_calls(pool: &PgPool, q: AiCallsQuery) -> Result<AiCallPage, sqlx::Error> {
    let limit = q.limit.clamp(1, 200);
    let offset = q.offset.max(0);
    // `%` and `_` are LIKE wildcards, not text — escape them so a search for
    // "gpt_4" doesn't match everything.
    let pattern = q
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            format!(
                "%{}%",
                s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
            )
        });
    let ascending = q.dir.as_deref() == Some("asc");

    let where_sql = "WHERE ($1::text IS NULL
                            OR feature ILIKE $1 ESCAPE '\\'
                            OR model ILIKE $1 ESCAPE '\\')";

    let total: (i64,) =
        sqlx::query_as(&format!("SELECT COUNT(*) FROM app_ai_calls {where_sql}"))
            .bind(&pattern)
            .fetch_one(pool)
            .await?;

    let items = sqlx::query_as::<_, AiCallRow>(&format!(
        "SELECT id, created_at, feature, model, prompt_tokens, completion_tokens,
                reasoning_tokens, cost_micros, route
           FROM app_ai_calls
           {where_sql}
          ORDER BY created_at {}, id
          LIMIT $2 OFFSET $3",
        if ascending { "ASC" } else { "DESC" }
    ))
    .bind(&pattern)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok(AiCallPage {
        items,
        total: total.0,
    })
}

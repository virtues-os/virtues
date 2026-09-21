//! The visits log and its frecency read.
//!
//! `POST /api/visits` records that the owner opened a chat, page or project
//! by a gesture of their own (migration 0031). The client decides what counts
//! — a tab activated by a click, a row chosen from a panel or ⌘K, held for a
//! few seconds — and leaves out restores on reload and opens the assistant
//! made for them. This side only refuses a repeat within a minute, so a
//! twitchy client or two devices cannot turn one sitting into ten.
//!
//! `GET /api/visits/frecency` is the only reader. It scores each record over
//! the last [`VISIT_RETENTION_DAYS`] the way Firefox's address bar does: a
//! visit this week counts in full and one from two months ago a third, so a
//! page hammered once in March cannot outrank the one opened every morning.
//! ⌘K uses it as a prior inside each result group. No screen shows the number.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

/// Visits older than this are pruned by the sweeper and ignored by frecency.
pub const VISIT_RETENTION_DAYS: i64 = 90;

/// Two opens of one record inside this window are one visit.
const DEDUPE_WINDOW: &str = "1 minute";

const KINDS: [&str; 3] = ["chat", "page", "project"];

#[derive(Debug, Clone, Deserialize)]
pub struct RecordVisitRequest {
    pub kind: String,
    pub record_id: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Frecency {
    pub kind: String,
    pub record_id: String,
    pub score: i64,
}

/// Append a visit unless the same record was visited within the last minute.
pub async fn record_visit(pool: &PgPool, req: RecordVisitRequest) -> Result<()> {
    if !KINDS.contains(&req.kind.as_str()) {
        return Err(Error::InvalidInput(format!(
            "unknown visit kind '{}' (chat | page | project)",
            req.kind
        )));
    }
    if req.record_id.trim().is_empty() {
        return Err(Error::InvalidInput("record_id is required".into()));
    }
    sqlx::query(&format!(
        "INSERT INTO app_visits (kind, record_id) \
         SELECT $1, $2 \
         WHERE NOT EXISTS ( \
             SELECT 1 FROM app_visits \
             WHERE kind = $1 AND record_id = $2 \
               AND occurred_at > now() - interval '{DEDUPE_WINDOW}')"
    ))
    .bind(&req.kind)
    .bind(&req.record_id)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to record visit: {e}")))?;
    Ok(())
}

/// Every visited record's frecency, highest first. Bucketed by age, summed:
/// under 4 days 100, under 14 days 70, under 31 days 50, else 30.
pub async fn frecency(pool: &PgPool) -> Result<Vec<Frecency>> {
    sqlx::query_as::<_, Frecency>(&format!(
        "SELECT kind, record_id, \
                SUM(CASE WHEN occurred_at > now() - interval '4 days'  THEN 100 \
                         WHEN occurred_at > now() - interval '14 days' THEN 70 \
                         WHEN occurred_at > now() - interval '31 days' THEN 50 \
                         ELSE 30 END)::bigint AS score \
         FROM app_visits \
         WHERE occurred_at > now() - make_interval(days => {VISIT_RETENTION_DAYS}) \
         GROUP BY kind, record_id \
         ORDER BY score DESC, MAX(occurred_at) DESC \
         LIMIT 2000"
    ))
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to read frecency: {e}")))
}

/// Prune visits past the retention window. The sweeper's tick.
pub async fn prune(pool: &PgPool) -> Result<u64> {
    let n = sqlx::query(&format!(
        "DELETE FROM app_visits \
         WHERE occurred_at < now() - make_interval(days => {VISIT_RETENTION_DAYS})"
    ))
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to prune visits: {e}")))?
    .rows_affected();
    Ok(n)
}

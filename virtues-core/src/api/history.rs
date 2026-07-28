//! App navigation history — the data behind the sidebar's "Recents".
//!
//! Sibling to [`super::pins`]: pins are the routes the user chose to keep,
//! history is the routes they've been. Same `url` convention, so the two
//! interoperate without translation.
//!
//! Why a log rather than sorting the existing tables by `updated_at`: opening
//! something doesn't modify it. Reading a PDF, re-reading a chat, or looking at
//! a record leaves no trace in any table, so "recent" built from `updated_at`
//! answers "what did I last *change*" — a different and much less useful
//! question than "what was I last *looking at*".
//!
//! Append-only and never pruned — see migration 0068. A visit row is ~100
//! bytes, so even heavy use costs single-digit megabytes a year, which is
//! nothing next to what the box already holds. Read speed is protected by
//! scanning a bounded recent window rather than by throwing history away.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::error::Result;
use crate::types::Timestamp;

/// One row of collapsed history — a url, with the most recent visit to it.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct HistoryEntry {
    pub url: String,
    pub label: Option<String>,
    pub icon: Option<String>,
    pub kind: Option<String>,
    pub visited_at: Timestamp,
    /// How many times this url has been visited inside the retained window.
    pub visit_count: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecordVisitRequest {
    pub url: String,
    pub label: Option<String>,
    pub icon: Option<String>,
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct HistoryQuery {
    /// Restrict to these kinds. Empty/absent means all.
    pub kinds: Option<Vec<String>>,
    /// Only visits at or after this instant.
    pub since: Option<String>,
    pub limit: Option<i64>,
}

const DEFAULT_LIMIT: i64 = 20;
const MAX_LIMIT: i64 = 200;

/// How many recent visits the collapse considers.
///
/// The table grows forever, but "what was I just looking at" only ever needs
/// the recent end of it. Taking the newest N rows off the index first, then
/// collapsing within that, keeps the query flat whether the log holds a
/// thousand rows or ten million. Generous enough that a genuinely recent
/// destination can't fall outside it.
const SCAN_WINDOW: i64 = 5000;

/// Record a visit. Fire-and-forget from the client's perspective — a failure
/// here must never interrupt navigation, so the caller ignores the result.
pub async fn record_visit(db: &PgPool, req: RecordVisitRequest) -> Result<()> {
    let url = req.url.trim();
    if url.is_empty() {
        return Ok(());
    }

    sqlx::query(
        r#"INSERT INTO app_history (url, label, icon, kind)
           VALUES ($1, $2, $3, $4)"#,
    )
    .bind(url)
    .bind(req.label.as_deref())
    .bind(req.icon.as_deref())
    .bind(req.kind.as_deref())
    .execute(db)
    .await?;

    Ok(())
}

/// Collapsed history, most recent first.
///
/// `DISTINCT ON (url)` keeps one row per destination — the sidebar wants "the
/// last twenty things you looked at", not "the last twenty times you looked at
/// something", which for anyone with a habit is the same three pages over and
/// over.
pub async fn list_history(db: &PgPool, query: HistoryQuery) -> Result<Vec<HistoryEntry>> {
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let kinds = query.kinds.filter(|k| !k.is_empty());

    let rows = sqlx::query_as::<_, HistoryEntry>(
        r#"
        WITH recent AS (
            -- Bounded first, collapsed second. This is what lets the log grow
            -- without bound: the planner takes these off idx_app_history_recent
            -- and stops, so the work never scales with the archive's size.
            --
            -- Every occurrence carries its cast, not just the first. Postgres
            -- infers a bare `$n` from its surrounding context, so `kind = ANY($1)`
            -- and `visited_at >= $2` were being typed from the *bind* (text)
            -- rather than from the column, which failed with
            -- "operator does not exist: timestamp with time zone >= text".
            SELECT url, label, icon, kind, visited_at
            FROM app_history
            WHERE ($1::text[] IS NULL OR kind = ANY($1::text[]))
              AND ($2::timestamptz IS NULL OR visited_at >= $2::timestamptz)
            ORDER BY visited_at DESC
            LIMIT $4
        )
        SELECT url, label, icon, kind, visited_at, visit_count
        FROM (
            SELECT DISTINCT ON (url)
                   url,
                   label,
                   icon,
                   kind,
                   visited_at,
                   -- Visits within the scanned window, not for all time. For a
                   -- recents list that's the more useful number anyway.
                   COUNT(*) OVER (PARTITION BY url) AS visit_count
            FROM recent
            ORDER BY url, visited_at DESC
        ) collapsed
        ORDER BY visited_at DESC
        LIMIT $3
        "#,
    )
    .bind(kinds.as_deref())
    .bind(query.since.as_deref())
    .bind(limit)
    .bind(SCAN_WINDOW)
    .fetch_all(db)
    .await?;

    Ok(rows)
}

/// Forget one destination entirely — every visit to it, not just the latest.
pub async fn forget_url(db: &PgPool, url: &str) -> Result<()> {
    sqlx::query("DELETE FROM app_history WHERE url = $1")
        .bind(url)
        .execute(db)
        .await?;
    Ok(())
}

/// Clear all history.
///
/// Not optional on a box that holds someone's whole life: a complete record of
/// everything its owner has looked at needs a way to be unmade, and it has to
/// be reachable from the UI rather than from a SQL prompt.
pub async fn clear_history(db: &PgPool) -> Result<()> {
    sqlx::query("TRUNCATE app_history").execute(db).await?;
    Ok(())
}

//! Stream health — per-stream ingest freshness, so a silent stall surfaces on
//! its own instead of being found by hand.
//!
//! Nothing in the app told anyone when a stream stopped. Messages went dark for
//! three days, the calendar sync died for two weeks, and finance dropped every
//! batch — all invisible until someone queried the tables directly. This turns
//! that into an at-a-glance signal.
//!
//! Driven off the ontology registry (`registered_ontologies()`), so a new
//! stream shows up here automatically — there is no second list to keep in sync.
//!
//! # Status — self-calibrating, no hardcoded per-stream cadence
//!
//!   never   — 0 rows ever. The source was never connected.
//!   live    — something arrived in the last 24h.
//!   stalled — nothing in 24h, but it WAS flowing this week. The alarm.
//!   idle    — has data, but nothing in 7 days. Genuinely quiet, or a long stall.
//!
//! The 24h/7d split is deliberately coarse: it fires on the failures we
//! actually hit (a stream that was flowing and stopped) and tolerates a
//! legitimately quiet stream by calling it `idle`, not `stalled`. A stream's
//! own rhythm — not a global threshold — is what a later version would compare
//! against; this is the honest floor that already catches the real cases.

use crate::database::Database;
use crate::Result;
use serde::{Deserialize, Serialize};
use sqlx::Row;

#[derive(Debug, Serialize, Deserialize)]
pub struct StreamHealth {
    /// Registry name, e.g. `communication_message`.
    pub name: String,
    pub display_name: String,
    /// `never` | `live` | `stalled` | `idle`.
    pub status: String,
    pub total: i64,
    pub count_24h: i64,
    pub count_7d: i64,
    /// Newest event time (the stream's own timestamp column).
    pub last_event: Option<chrono::DateTime<chrono::Utc>>,
    /// Newest ingest time (`created_at`) — how fresh the *pipe* is, which can
    /// lag the event time when a derivation falls behind.
    pub last_ingest: Option<chrono::DateTime<chrono::Utc>>,
}

/// Freshness for every ingest stream, worst-first (stalled → idle → never →
/// live) so the caller leads with what needs attention.
pub async fn stream_health(db: &Database) -> Result<Vec<StreamHealth>> {
    // Ingest streams only. Chats/pages are user content in the same registry,
    // not sources, so a quiet notebook must not read as a broken pipe.
    let streams: Vec<_> = virtues_registry::ontologies::registered_ontologies()
        .into_iter()
        .filter(|o| o.table_name.starts_with("data_"))
        .collect();
    if streams.is_empty() {
        return Ok(vec![]);
    }

    // One UNION ALL over the registry — a 0-row table still returns its row
    // (count 0 → `never`), which a group-over-rows form would silently drop.
    // `table_name` / `timestamp_column` are compile-time-static registry values
    // (never user input), so interpolating them is injection-safe.
    let sql = streams
        .iter()
        .map(|o| {
            format!(
                "SELECT '{name}' AS name, '{disp}' AS display_name, \
                   count(*)::int8 AS total, \
                   count(*) FILTER (WHERE created_at > now() - interval '24 hours')::int8 AS c24, \
                   count(*) FILTER (WHERE created_at > now() - interval '7 days')::int8  AS c7, \
                   max({ts})::timestamptz AS last_event, \
                   max(created_at)::timestamptz AS last_ingest \
                 FROM {table}",
                name = o.name,
                disp = o.display_name.replace('\'', "''"),
                ts = o.timestamp_column,
                table = o.table_name,
            )
        })
        .collect::<Vec<_>>()
        .join(" UNION ALL ");

    let rows = sqlx::query(&sql).fetch_all(db.pool()).await?;

    let mut out: Vec<StreamHealth> = rows
        .into_iter()
        .map(|r| {
            let total: i64 = r.get("total");
            let c24: i64 = r.get("c24");
            let c7: i64 = r.get("c7");
            let status = if total == 0 {
                "never"
            } else if c24 > 0 {
                "live"
            } else if c7 > 0 {
                "stalled"
            } else {
                "idle"
            };
            StreamHealth {
                name: r.get("name"),
                display_name: r.get("display_name"),
                status: status.to_string(),
                total,
                count_24h: c24,
                count_7d: c7,
                last_event: r.get("last_event"),
                last_ingest: r.get("last_ingest"),
            }
        })
        .collect();

    fn rank(s: &str) -> u8 {
        match s {
            "stalled" => 0,
            "idle" => 1,
            "never" => 2,
            _ => 3, // live
        }
    }
    out.sort_by(|a, b| rank(&a.status).cmp(&rank(&b.status)).then(a.name.cmp(&b.name)));
    Ok(out)
}

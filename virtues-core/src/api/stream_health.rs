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
    /// Sources that declare they write this stream, by display name. Answers
    /// "what would fill this" — the question a page showing an empty stream
    /// could not previously answer at all.
    pub provided_by: Vec<String>,
    /// True when at least one of those sources is actually connected. This is
    /// the axis that was missing: `total == 0` alone cannot tell "nothing
    /// provides this" from "provided, but switched off or not yet delivered".
    pub connected: bool,
    /// No source writes it, yet rows exist — the box computed it from other
    /// streams (a sessionizer). "Connect something" is the wrong advice here,
    /// so the UI must not offer it.
    pub derived: bool,
}

/// One stream's arrivals, one cell per day, oldest first.
#[derive(Debug, Serialize, Deserialize)]
pub struct StreamDays {
    pub name: String,
    pub display_name: String,
    /// The provider that actually wrote these rows, from `source_provider` on
    /// the data itself — ground truth, not the manifest's claim. One entry per
    /// (stream, provider) pair, so a stream two sources both write (bookmarks
    /// from a Mac and from GitHub) is two rows, each with its own shape.
    pub provider: String,
    /// `days[i]` is the row count for `from + i` days. Zero-filled, so the
    /// caller can render a fixed grid without reconciling sparse dates.
    pub days: Vec<i64>,
}

/// Daily arrival counts per stream over a window.
///
/// A scalar "last seen" per stream cannot show the only thing that matters at a
/// glance: whether streams stopped *together*. Nineteen rows all reading `Jul 7`
/// is one event — a device stopped — but as a list it reads as nineteen
/// problems. Given a day axis the same data draws a vertical cliff across every
/// row that device feeds, and the shape says it without a word.
///
/// It also exposes rhythm, which a scalar cannot: a calendar that only fills on
/// weekdays is healthy, a heart rate that only fills on weekdays is a phone left
/// at home.
pub async fn stream_days(db: &Database, days: i64) -> Result<Vec<StreamDays>> {
    let days = days.clamp(7, 365);
    let streams: Vec<_> = virtues_registry::ontologies::registered_ontologies()
        .into_iter()
        .filter(|o| o.table_name.starts_with("data_"))
        .collect();
    if streams.is_empty() {
        return Ok(vec![]);
    }

    // One UNION ALL, grouped per provider per day. Grouping on the data's own
    // `source_provider` rather than on the manifest's declared `writes` means
    // the grid shows what actually happened: if a stream is fed by two sources,
    // or by one the manifest never claimed, the rows are still right. The
    // declared map stays useful for the opposite question — what would fill a
    // stream that has no rows at all — which no observation can answer.
    //
    // `table_name` is a compile-time registry constant, never user input, so
    // interpolation is injection-safe; the freshness query above relies on the
    // same argument.
    let sql = streams
        .iter()
        .map(|o| {
            format!(
                "SELECT '{name}' AS name, \
                        source_provider AS provider, \
                        (created_at AT TIME ZONE 'UTC')::date AS day, \
                        count(*)::int8 AS n \
                   FROM {table} \
                  WHERE created_at >= now() - ($1::int * interval '1 day') \
                  GROUP BY 1, 2, 3",
                name = o.name,
                table = o.table_name,
            )
        })
        .collect::<Vec<_>>()
        .join(" UNION ALL ");

    let rows = sqlx::query(&sql).bind(days as i32).fetch_all(db.pool()).await?;

    let today = chrono::Utc::now().date_naive();
    let start = today - chrono::Duration::days(days - 1);
    let width = days as usize;

    // (stream, provider) -> daily counts.
    let mut acc: std::collections::HashMap<(String, String), Vec<i64>> =
        std::collections::HashMap::new();
    for r in rows {
        let name: String = r.get("name");
        let provider: String = r.get("provider");
        let day: chrono::NaiveDate = r.get("day");
        let n: i64 = r.get("n");
        let i = (day - start).num_days();
        if i < 0 || i as usize >= width {
            continue;
        }
        acc.entry((name, provider))
            .or_insert_with(|| vec![0i64; width])[i as usize] = n;
    }

    let display: std::collections::HashMap<&str, &str> = streams
        .iter()
        .map(|o| (o.name, o.display_name))
        .collect();

    let mut out: Vec<StreamDays> = acc
        .into_iter()
        .map(|((name, provider), days)| StreamDays {
            display_name: display.get(name.as_str()).copied().unwrap_or("").to_string(),
            provider,
            days,
            name,
        })
        .collect();
    // Stable order: provider, then stream, so the grid does not reshuffle
    // between polls.
    out.sort_by(|a, b| a.provider.cmp(&b.provider).then(a.name.cmp(&b.name)));
    Ok(out)
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
            let name: String = r.get("name");
            let writers = crate::applet_templates::sources_writing(&name);
            StreamHealth {
                display_name: r.get("display_name"),
                status: status.to_string(),
                total,
                count_24h: c24,
                count_7d: c7,
                last_event: r.get("last_event"),
                last_ingest: r.get("last_ingest"),
                provided_by: writers
                    .iter()
                    .map(|id| {
                        crate::applet_templates::lookup_source(id)
                            .map(|s| s.display_name)
                            .unwrap_or_else(|| id.clone())
                    })
                    .collect(),
                // Filled in below — needs one query, not one per stream.
                connected: false,
                derived: writers.is_empty() && total > 0,
                name,
            }
        })
        .collect();

    // Which sources are actually connected. One pass over both tables rather
    // than a lookup per stream: OAuth and API-key sources mint a credential,
    // device sources (iOS, Mac) pair into app_device and never do — a stream
    // fed by an iPhone would look unconnected if only credentials were checked.
    let live_sources: std::collections::HashSet<String> = sqlx::query_scalar::<_, String>(
        "SELECT source_id FROM credentials WHERE status = 'active' \
         UNION \
         SELECT source_id FROM app_device WHERE source_id IS NOT NULL AND revoked_at IS NULL",
    )
    .fetch_all(db.pool())
    .await
    .unwrap_or_default()
    .into_iter()
    .collect();

    for s in &mut out {
        let ids = crate::applet_templates::sources_writing(&s.name);
        s.connected = ids.iter().any(|id| live_sources.contains(id));
    }

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

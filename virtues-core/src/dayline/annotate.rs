//! Annotate segmented events from their own time windows.
//!
//! An event is a `[start_time, end_time)` interval. Everything that happened
//! inside it is already in the lake, keyed by timestamp — so membership needs
//! no join table, just the window. This module walks each event and stamps it
//! with what the window contains.
//!
//! # Why this exists — the bug it fixes
//!
//! `autonomic_scoring` builds its baseline from
//!
//! ```sql
//! WHERE is_sleep = FALSE AND avg_hr IS NOT NULL
//! ```
//!
//! and the ONLY writer of `avg_hr` in the entire codebase was `dayline::sleep`,
//! which writes it exclusively on `is_sleep = TRUE` events. The two predicates
//! are **mutually exclusive by construction**: the baseline was always empty,
//! so `compute_autonomic_for_day` returned `Ok(0)` for every user, every day,
//! forever, and `hr_z` / `autonomic_z` were permanently NULL. The demo seeds
//! hand-populate `avg_hr`, which is why the day page looked alive.
//!
//! `docs/the-day.md:464` describes this exact step ("annotate wiki_events.avg_hr
//! from HR data in event window"). It was never implemented.
//!
//! Same class of gap for `entities` (only the chat-agent tool ever wrote it, so
//! `topic_entity_novelty` scored empty arrays) and `source_ontologies` (passed
//! as `None` at `day_summary.rs`, a dead column since 0006).
//!
//! # Ordering
//!
//! This MUST run after `generate_day_summary` — which deletes and re-creates
//! every auto event — and after `resolve_sleep_events`, so the sleep event gets
//! annotated too. See the comment block in `actions/day_summary_eod/main.rs`.
//!
//! It is idempotent and independently re-runnable over history: every value it
//! writes is a pure function of immutable `data_*` records and the event's own
//! window. That is the whole point — narrative is a re-derivable projection of
//! evidence, so re-cutting the events is cheap rather than lossy.

use chrono::NaiveDate;
use serde_json::json;
use sqlx::{PgPool, Row};

use crate::error::Result;
use virtues_registry::ontologies::registered_ontologies;

/// Annotate every event on `date`. Returns the number of events updated.
pub async fn annotate_events_for_day(pool: &PgPool, date: NaiveDate) -> Result<u32> {
    let events = sqlx::query(
        r#"
        SELECT e.id, e.start_time, e.end_time, e.kind
        FROM wiki_events e
        JOIN wiki_days d ON d.id = e.day_id
        WHERE d.date = $1
        ORDER BY e.start_time
        "#,
    )
    .bind(date)
    .fetch_all(pool)
    .await?;

    let mut annotated = 0u32;

    for row in &events {
        let id: String = row.get("id");
        let start: chrono::DateTime<chrono::Utc> = row.get("start_time");
        let end: chrono::DateTime<chrono::Utc> = row.get("end_time");
        let kind: String = row.get("kind");

        let avg_hr = window_avg_hr(pool, start, end).await;
        let entities = window_entities(pool, start, end).await?;
        let ontologies = window_ontologies(pool, start, end).await;

        // Confidence — how sure we are of the block. Deterministic, per the model in
        // docs/event-timeline.md: `unknown` is low by definition (no signal); `sleep`
        // is high (authoritative sleep data); `transit` is medium (a deterministic
        // place-change, but thin); a `stay` scores by WITNESS AGREEMENT — how many
        // independent source types corroborate its window (3+ high, 2 medium, else low).
        let confidence = match kind.as_str() {
            "unknown" => "low",
            "sleep" => "high",
            "transit" => "medium",
            _ => match ontologies.len() {
                n if n >= 3 => "high",
                2 => "medium",
                _ => "low",
            },
        };

        // COALESCE on avg_hr: `dayline::sleep` may already have written it for
        // the sleep event, and its window is the authoritative one. Don't
        // clobber a value with a NULL.
        sqlx::query(
            r#"
            UPDATE wiki_events
            SET avg_hr            = COALESCE($2, avg_hr),
                entities          = $3,
                source_ontologies = $4,
                confidence        = $5
            WHERE id = $1
            "#,
        )
        .bind(&id)
        .bind(avg_hr)
        .bind(json!(entities))
        .bind(json!(ontologies))
        .bind(confidence)
        .execute(pool)
        .await?;

        annotated += 1;
    }

    if annotated > 0 {
        tracing::info!(
            date = %date,
            events = annotated,
            "annotated events (avg_hr, entities, source_ontologies)"
        );
    }

    Ok(annotated)
}

/// Mean heart rate over the event's window.
///
/// `None` when the user wore nothing, or wasn't wearing it then. That is
/// silence, not zero — and `autonomic_scoring` correctly skips it rather than
/// scoring a fictional resting heart rate of 0 bpm.
async fn window_avg_hr(
    pool: &PgPool,
    start: chrono::DateTime<chrono::Utc>,
    end: chrono::DateTime<chrono::Utc>,
) -> Option<f64> {
    sqlx::query_scalar(
        r#"SELECT AVG(CAST(bpm AS REAL))
           FROM data_health_heart_rate
           WHERE timestamp >= $1 AND timestamp < $2"#,
    )
    .bind(start)
    .bind(end)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

/// The entities already resolved to records inside this window.
///
/// No LLM. `wiki_entity_refs` is populated by the deterministic resolvers
/// (`entity_resolution::people` / `::places`) and by the mention resolver, and
/// it is the authoritative edge — `wiki_events.entities` is a derived, rebuilt
/// cache over it, never hand-edited. Events get re-cut; the refs don't move.
async fn window_entities(
    pool: &PgPool,
    start: chrono::DateTime<chrono::Utc>,
    end: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<String>> {
    let rows = sqlx::query(
        r#"
        SELECT DISTINCT entity_id
        FROM wiki_entity_refs
        WHERE timestamp >= $1 AND timestamp < $2
        "#,
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| r.get("entity_id")).collect())
}

/// Which ontologies actually had records in this window.
///
/// Registry-driven — no per-source branch, so a new ontology is covered the day
/// it lands. Mirrors `day_summary::detect_ontology_presence`, but scoped to an
/// event rather than a whole day.
async fn window_ontologies(
    pool: &PgPool,
    start: chrono::DateTime<chrono::Utc>,
    end: chrono::DateTime<chrono::Utc>,
) -> Vec<String> {
    let mut present = Vec::new();

    for ont in registered_ontologies() {
        // Interpolated identifiers are compile-time constants from the
        // registry, never user input. The window is bound.
        let sql = format!(
            "SELECT 1 FROM {} WHERE {} >= $1 AND {} < $2 LIMIT 1",
            ont.table_name, ont.timestamp_column, ont.timestamp_column
        );

        let has_data = sqlx::query(&sql)
            .bind(start)
            .bind(end)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .is_some();

        if has_data {
            present.push(ont.name.to_string());
        }
    }

    present
}

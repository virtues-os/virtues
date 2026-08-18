//! Sleep event resolution for the dayline.
//!
//! Creates or updates a single `is_sleep=1` wiki_event per day from
//! `data_health_sleep` records. Sleep belongs to the day you wake up on.
//! Called as a deterministic pre-step in the EOD maintenance flow.

use chrono::{DateTime, NaiveDate, Utc};
use sqlx::PgPool;

/// Resolve sleep events for a date and the day before it.
/// Finds completed sleep records where wake-up time falls on each date,
/// creates/updates the corresponding `is_sleep=1` wiki_event.
pub async fn resolve_sleep_events(pool: &PgPool, date: NaiveDate) {
    resolve_sleep_for_date(pool, date).await;
    let prev = date - chrono::Duration::days(1);
    resolve_sleep_for_date(pool, prev).await;
}

/// Resolve sleep for a single date. Finds sleep records where end_time
/// falls on this calendar date. If a sleep wiki_event already exists, updates it.
/// If not, creates one.
async fn resolve_sleep_for_date(pool: &PgPool, date: NaiveDate) {
    use sqlx::Row;

    let date_str = date.format("%Y-%m-%d").to_string();
    let next_date = (date + chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();

    // Find sleep record where wake-up (end_time) falls on this date
    let sleep_row: Option<sqlx::postgres::PgRow> = sqlx::query(
        r#"SELECT id, started_at, ended_at, duration_minutes
           FROM data_health_sleep
           WHERE ended_at >= ($1 || 'T00:00:00Z')::timestamptz
             AND ended_at < ($2 || 'T00:00:00Z')::timestamptz
           ORDER BY ended_at DESC LIMIT 1"#,
    )
    .bind(&date_str)
    .bind(&next_date)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let sleep_row = match sleep_row {
        Some(r) => r,
        None => return, // No sleep data for this date
    };

    // `start_time`/`end_time` are TIMESTAMPTZ — decode them as DateTime<Utc>,
    // not String (a String decode silently fails → empty string → garbage).
    let sleep_start: DateTime<Utc> = match sleep_row.try_get("started_at") {
        Ok(v) => v,
        Err(_) => return,
    };
    let sleep_end: DateTime<Utc> = match sleep_row.try_get("ended_at") {
        Ok(v) => v,
        Err(_) => return,
    };
    let duration_mins: Option<i64> = sleep_row.try_get("duration_minutes").ok();

    // Clamp start_time to UTC midnight of this date (the day page's boundary).
    let day_midnight: DateTime<Utc> = date.and_hms_opt(0, 0, 0).unwrap().and_utc();
    let event_start = if sleep_start < day_midnight {
        day_midnight
    } else {
        sleep_start
    };

    // Get day_id
    let day_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM wiki_days WHERE date = $1::date",
    )
    .bind(&date_str)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let day_id = match day_id {
        Some(id) => id,
        None => return, // No wiki_day for this date
    };

    // Compute avg HR during sleep window from heart rate data
    let avg_hr: Option<f64> = sqlx::query_scalar(
        r#"SELECT AVG(CAST(bpm AS REAL))
           FROM data_health_heart_rate
           WHERE occurred_at >= $1 AND occurred_at < $2"#,
    )
    .bind(sleep_start)
    .bind(sleep_end)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    // Build summary
    let hours = duration_mins.unwrap_or(0) as f64 / 60.0;
    let summary = format!("Slept {:.1} hours.", hours);

    // Check if sleep event already exists for this day
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT id FROM wiki_events WHERE day_id = $1 AND is_sleep = TRUE LIMIT 1",
    )
    .bind(&day_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    if let Some(event_id) = existing {
        // Update existing sleep event
        let _ = sqlx::query(
            r#"UPDATE wiki_events
               SET started_at = $1, ended_at = $2, avg_hr = $3, event_summary = $4
               WHERE id = $5"#,
        )
        .bind(event_start)
        .bind(sleep_end)
        .bind(avg_hr)
        .bind(&summary)
        .bind(&event_id)
        .execute(pool)
        .await;
    } else {
        // Create new sleep event
        let event_id = format!("ev_sleep_{}",
            date_str.replace('-', ""));
        let _ = sqlx::query(
            r#"INSERT INTO wiki_events
               (id, day_id, started_at, ended_at, auto_label, auto_location,
                source_ontologies, kind, event_summary, topics, entities,
                agent_action, avg_hr, confidence)
               VALUES ($1, $2, $3, $4, 'Sleep', 'Home', '["sleep"]'::jsonb,
                       'sleep', $5, '["sleep"]'::jsonb, '[]'::jsonb, 'NEW', $6, 'high')
               ON CONFLICT (id) DO NOTHING"#,
        )
        .bind(&event_id)
        .bind(&day_id)
        .bind(event_start)
        .bind(sleep_end)
        .bind(&summary)
        .bind(avg_hr)
        .execute(pool)
        .await;
    }

    // Sleep is AUTHORITATIVE for its window — it stands on real sleep-tracking
    // data, not inference. The detective produces a gapless 00:00–24:00 timeline
    // that necessarily covers the overnight too (as "Unknown"), so without this the
    // authoritative sleep event OVERLAPS those backfilled blocks and the timeline
    // stops being gapless-and-non-overlapping. Reconcile: clip the non-sleep auto
    // events (never user events, never the sleep event itself) to the sleep window.
    reconcile_overlaps(pool, &day_id, event_start, sleep_end).await;
}

/// Clip non-sleep AUTO events so none overlaps the authoritative sleep window
/// `[start, end)`, keeping the timeline gapless. User events are sacred and never
/// touched.
///
///   * spans the whole window → SPLIT into head `[·, start)` + tail `[end, ·)`
///     (the overnight Unknown almost always wraps the sleep fragment this way —
///     truncating it instead of splitting would punch a gap)
///   * straddles the start → truncated to end at `start`
///   * straddles the end   → pushed to begin at `end`
///   * fully inside        → deleted (the sleep block replaces it)
async fn reconcile_overlaps(
    pool: &PgPool,
    day_id: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) {
    // Spanning events first: materialise the TAIL `[end, orig_end)` as a copy, then
    // (below) truncate the head. Done before the other rules so the freshly-created
    // tail (which begins exactly at `end`) is not itself re-clipped.
    let _ = sqlx::query(
        "INSERT INTO wiki_events \
           (id, day_id, started_at, ended_at, auto_label, auto_location, \
            source_ontologies, kind, is_user_added, is_user_edited, \
            user_hidden, user_created, topics, entities, event_summary, confidence) \
         SELECT 'ev_' || replace(gen_random_uuid()::text, '-', ''), day_id, $3, ended_at, \
                auto_label, auto_location, source_ontologies, kind, \
                FALSE, is_user_edited, user_hidden, user_created, topics, entities, \
                event_summary, confidence \
         FROM wiki_events \
         WHERE day_id = $1 AND is_sleep = FALSE AND is_user_added = FALSE \
           AND started_at < $2 AND ended_at > $3",
    )
    .bind(day_id)
    .bind(start)
    .bind(end)
    .execute(pool)
    .await;

    // Straddles the start, OR the (now tail-copied) spanning head → end at `start`.
    let _ = sqlx::query(
        "UPDATE wiki_events SET ended_at = $2 \
         WHERE day_id = $1 AND is_sleep = FALSE AND is_user_added = FALSE \
           AND started_at < $2 AND ended_at > $2",
    )
    .bind(day_id)
    .bind(start)
    .bind(end)
    .execute(pool)
    .await;

    // Straddles the end (starts within, ends after) → begin at `end`.
    let _ = sqlx::query(
        "UPDATE wiki_events SET started_at = $3 \
         WHERE day_id = $1 AND is_sleep = FALSE AND is_user_added = FALSE \
           AND started_at >= $2 AND started_at < $3 AND ended_at > $3",
    )
    .bind(day_id)
    .bind(start)
    .bind(end)
    .execute(pool)
    .await;

    // Fully inside → gone. Restricted to `is_unknown` backfill: that is the only
    // thing sleep is meant to replace. A real LABELED auto event fully inside a
    // tracked-sleep window is contradictory data (you were logged doing something
    // AND asleep) — we keep it (it may briefly overlap the sleep block) rather than
    // silently destroy a real, content-addressed event we cannot recover.
    let _ = sqlx::query(
        "DELETE FROM wiki_events \
         WHERE day_id = $1 AND is_sleep = FALSE AND is_user_added = FALSE AND is_unknown = TRUE \
           AND started_at >= $2 AND ended_at <= $3",
    )
    .bind(day_id)
    .bind(start)
    .bind(end)
    .execute(pool)
    .await;
}

//! Sleep for the dayline: which night belongs to a day, and its event.
//!
//! A night belongs to the day you wake up on, in that day's own zone
//! ([`crate::timezone::day_window`]). Creates or updates a single `is_sleep`
//! wiki_event per day from it. Called as a deterministic pre-step in the EOD
//! maintenance flow.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde_json::Value;
use sqlx::PgPool;

use crate::error::Result;

/// HealthKit can deliver one night as several records (a wake at 5:17, back
/// asleep at 5:18). Records closer than this are one night; keeping only one
/// record made a six-hour night read as 41 minutes.
const SAME_NIGHT_GAP_MINUTES: i64 = 30;

/// How far before a day's start a night's first record can begin. Generous on
/// purpose: it only bounds the fetch, and the join decides what a night is.
const NIGHT_LOOKBACK_HOURS: i64 = 24;

/// One night: its sleep records joined.
#[derive(Debug, Clone, PartialEq)]
pub struct Night {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    /// Time covered by the records themselves, so the short wakes between
    /// records are not counted as sleep and overlapping records are not counted
    /// twice.
    pub asleep_minutes: i64,
    /// Every record's `sleep_stages` entries, in record order.
    pub stages: Vec<Value>,
}

/// One `data_health_sleep` row, as the join reads it.
#[derive(Debug, Clone)]
struct SleepRecord {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    stages: Vec<Value>,
}

/// Join time-ordered sleep records into nights.
fn join_nights(mut records: Vec<SleepRecord>) -> Vec<Night> {
    records.sort_by_key(|r| r.start);
    let gap = Duration::minutes(SAME_NIGHT_GAP_MINUTES);
    let mut nights: Vec<Night> = Vec::new();
    for r in records.into_iter().filter(|r| r.end > r.start) {
        match nights.last_mut() {
            Some(n) if r.start - n.end < gap => {
                // Count only the part of this record the night does not already cover.
                let fresh_from = r.start.max(n.end);
                if r.end > fresh_from {
                    n.asleep_minutes += (r.end - fresh_from).num_minutes();
                }
                n.end = n.end.max(r.end);
                n.stages.extend(r.stages);
            }
            _ => nights.push(Night {
                start: r.start,
                end: r.end,
                asleep_minutes: (r.end - r.start).num_minutes(),
                stages: r.stages,
            }),
        }
    }
    nights
}

/// Nights that end in `[from, to)`, each joined from all of its records.
pub async fn nights_ending_between(
    pool: &PgPool,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<Night>> {
    use sqlx::Row;
    let rows = sqlx::query(
        r#"SELECT started_at, ended_at, sleep_stages
           FROM data_health_sleep
           WHERE ended_at >= $1 AND started_at < $2
           ORDER BY started_at"#,
    )
    .bind(from - Duration::hours(NIGHT_LOOKBACK_HOURS))
    .bind(to)
    .fetch_all(pool)
    .await?;

    let mut records = Vec::with_capacity(rows.len());
    for row in &rows {
        // absent-ok: a row without stage detail is still a night's span.
        let stages = match row.try_get::<Option<Value>, _>("sleep_stages")? {
            Some(Value::Array(arr)) => arr,
            _ => Vec::new(),
        };
        records.push(SleepRecord {
            start: row.try_get("started_at")?,
            end: row.try_get("ended_at")?,
            stages,
        });
    }
    Ok(join_nights(records)
        .into_iter()
        .filter(|n| n.end >= from && n.end < to)
        .collect())
}

/// The night `date` woke up from: of the nights ending inside the day, the
/// longest, so an afternoon nap never stands in for the night.
pub async fn night_for_day(pool: &PgPool, date: NaiveDate) -> Result<Option<Night>> {
    let (start, end) = crate::timezone::day_window(pool, date).await?;
    Ok(nights_ending_between(pool, start, end)
        .await?
        .into_iter()
        .max_by_key(|n| n.asleep_minutes))
}

/// Resolve sleep events for a date and the day before it.
pub async fn resolve_sleep_events(pool: &PgPool, date: NaiveDate) {
    resolve_sleep_for_date(pool, date).await;
    let prev = date - Duration::days(1);
    resolve_sleep_for_date(pool, prev).await;
}

/// Resolve sleep for a single date: write or update its sleep event from the
/// night it woke up from.
async fn resolve_sleep_for_date(pool: &PgPool, date: NaiveDate) {
    let date_str = date.format("%Y-%m-%d").to_string();

    let night = match night_for_day(pool, date).await {
        Ok(Some(n)) => n,
        Ok(None) => return, // No sleep data for this date
        Err(e) => {
            // "Nothing to say about this date" and "the dayline is missing sleep
            // it actually has" must not look the same. Say which.
            tracing::error!(date = %date_str, error = %e, "sleep lookup failed — dayline will omit sleep for this date");
            return;
        }
    };
    let (sleep_start, sleep_end) = (night.start, night.end);

    // The event starts no earlier than the day does: the part of the night
    // before midnight belongs to the evening before.
    let day_start = match crate::timezone::day_window(pool, date).await {
        Ok((start, _)) => start,
        Err(e) => {
            tracing::error!(date = %date_str, error = %e, "couldn't window the day — dayline will omit sleep for this date");
            return;
        }
    };
    let event_start = sleep_start.max(day_start);

    // Get day_id
    let day_id: Option<String> =
        sqlx::query_scalar("SELECT id FROM wiki_days WHERE date = $1::date")
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

    // The joined night's own length, not one record's: see `SAME_NIGHT_GAP_MINUTES`.
    let summary = format!("Slept {:.1} hours.", night.asleep_minutes as f64 / 60.0);

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
        let event_id = format!("ev_sleep_{}", date_str.replace('-', ""));
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
async fn reconcile_overlaps(pool: &PgPool, day_id: &str, start: DateTime<Utc>, end: DateTime<Utc>) {
    // Spanning events first: materialise the TAIL `[end, orig_end)` as a copy, then
    // (below) truncate the head. Done before the other rules so the freshly-created
    // tail (which begins exactly at `end`) is not itself re-clipped.
    let _ = sqlx::query(
        "INSERT INTO wiki_events \
           (id, day_id, started_at, ended_at, auto_label, auto_location, \
            source_ontologies, kind, is_user_added, is_user_edited, \
            user_hidden, topics, entities, event_summary, confidence) \
         SELECT 'ev_' || replace(gen_random_uuid()::text, '-', ''), day_id, $3, ended_at, \
                auto_label, auto_location, source_ontologies, kind, \
                FALSE, is_user_edited, user_hidden, topics, entities, \
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

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hh: u32, mm: u32, day: u32) -> DateTime<Utc> {
        NaiveDate::from_ymd_opt(2026, 1, day)
            .unwrap()
            .and_hms_opt(hh, mm, 0)
            .unwrap()
            .and_utc()
    }

    fn rec(start: DateTime<Utc>, end: DateTime<Utc>) -> SleepRecord {
        SleepRecord {
            start,
            end,
            stages: vec![],
        }
    }

    #[test]
    fn a_night_split_by_a_one_minute_wake_is_one_night() {
        // 23:14–05:17, then 05:18–05:59: keeping only the last record read 41 min.
        let nights = join_nights(vec![
            rec(at(5, 18, 2), at(5, 59, 2)),
            rec(at(23, 14, 1), at(5, 17, 2)),
        ]);
        assert_eq!(nights.len(), 1);
        assert_eq!(nights[0].start, at(23, 14, 1));
        assert_eq!(nights[0].end, at(5, 59, 2));
        assert_eq!(nights[0].asleep_minutes, 363 + 41);
    }

    #[test]
    fn overlapping_records_are_not_counted_twice() {
        let nights = join_nights(vec![
            rec(at(23, 0, 1), at(3, 0, 2)),
            rec(at(2, 0, 2), at(6, 0, 2)),
        ]);
        assert_eq!(nights.len(), 1);
        assert_eq!(nights[0].asleep_minutes, 7 * 60);
    }

    #[test]
    fn a_long_gap_starts_a_new_night() {
        let nights = join_nights(vec![
            rec(at(23, 0, 1), at(6, 0, 2)),
            rec(at(14, 0, 2), at(14, 40, 2)),
        ]);
        assert_eq!(nights.len(), 2);
        assert_eq!(nights[1].asleep_minutes, 40);
    }

    #[test]
    fn stages_follow_record_order() {
        let mut a = rec(at(23, 0, 1), at(3, 0, 2));
        a.stages = vec![serde_json::json!({"stage": "core"})];
        let mut b = rec(at(3, 10, 2), at(6, 0, 2));
        b.stages = vec![serde_json::json!({"stage": "rem"})];
        let nights = join_nights(vec![b, a]);
        assert_eq!(nights[0].stages.len(), 2);
        assert_eq!(nights[0].stages[0]["stage"], "core");
    }
}

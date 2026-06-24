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
        r#"SELECT id, start_time, end_time, duration_minutes
           FROM data_health_sleep
           WHERE end_time >= ($1 || 'T00:00:00Z')::timestamptz
             AND end_time < ($2 || 'T00:00:00Z')::timestamptz
           ORDER BY end_time DESC LIMIT 1"#,
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
    let sleep_start: DateTime<Utc> = match sleep_row.try_get("start_time") {
        Ok(v) => v,
        Err(_) => return,
    };
    let sleep_end: DateTime<Utc> = match sleep_row.try_get("end_time") {
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
           WHERE timestamp >= $1 AND timestamp < $2"#,
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
               SET start_time = $1, end_time = $2, avg_hr = $3, event_summary = $4
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
               (id, day_id, start_time, end_time, auto_label, auto_location,
                source_ontologies, is_sleep, event_summary, topics, entities,
                agent_action, avg_hr)
               VALUES ($1, $2, $3, $4, 'Sleep', 'Home', '["sleep"]'::jsonb,
                       TRUE, $5, '["sleep"]'::jsonb, '[]'::jsonb, 'NEW', $6)
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
}

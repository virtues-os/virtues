//! The RAW RECORDS beneath a day, before anything has been made of them.
//!
//! Everything else in the wiki is something the box concluded — an event, an
//! entity, a sentence of prose. This is the layer under all of it: the
//! ontology rows exactly as they were collected, plus the three streams the
//! home page shows as spans (where you were, what was on your calendar, what
//! was heard) and the chats and heart rate of a day.
//!
//! It is deliberately the least interpreted surface in the room. When the
//! record says something the person does not recognise, this is where they go
//! to see what it was actually looking at.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::error::{Error, Result};

// ============================================================================
// Day Sources - Ontology records for a day
// ============================================================================

/// A data source record from an ontology table for a specific day
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaySource {
    pub source_type: String,
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub label: String,
    pub preview: Option<String>,
    /// True for high-frequency measurement streams (heart rate, steps, HRV).
    /// The day page hides these behind a filter by default since a single day
    /// can hold thousands of them.
    pub continuous: bool,
}

/// Resolve the timezone a given day should be rendered/windowed in —
/// "the timezone you woke up in", fixed at the day's start:
///   1. the locked `wiki_days.start_timezone` if a summary already ran (past days
///      keep the zone they were lived in), else
///   2. `tzf-rs(first located point of the day)` — the same "where you woke up"
///      signal the EOD lock uses, so live-today and locked-history agree even on
///      a travel day (a move surfaces as *tomorrow*, not a mid-day re-anchor), else
///   3. the viewing device's zone, but ONLY for an in-progress today with no
///      located points yet (web-only / location off), else
///   4. `home_timezone`.
/// See agents/record/timezone-model.md.
async fn resolve_render_timezone(
    pool: &PgPool,
    date: NaiveDate,
    client_tz: Option<&str>,
) -> String {
    use sqlx::Row;
    // 1. Locked per-day zone from a prior summary.
    if let Ok(Some(row)) =
        sqlx::query("SELECT start_timezone FROM wiki_days WHERE date = $1")
            .bind(date)
            .fetch_optional(pool)
            .await
    {
        if let Ok(Some(tz)) = row.try_get::<Option<String>, _>("start_timezone") {
            if !tz.is_empty() {
                return tz;
            }
        }
    }

    let home_tz = super::profile::get_timezone(pool)
        .await
        .unwrap_or(None)
        .unwrap_or_else(|| "UTC".to_string());

    // 2. Where the owner woke up that day (first located point). Authoritative and
    //    consistent with the EOD lock — does NOT drift to where the viewer is now.
    if let Some(tz) = crate::timezone::first_point_timezone(pool, date, &home_tz).await {
        return tz;
    }

    // 3. No location for the day — for an in-progress *today* only, fall back to the
    //    viewing device's zone (best available "where are you" for a web-only/
    //    location-off owner). Never applied to a past day. "Today" is in home_tz.
    let today_in_home = home_tz
        .parse::<chrono_tz::Tz>()
        .ok()
        .map(|tz| Utc::now().with_timezone(&tz).date_naive());
    if today_in_home == Some(date) {
        if let Some(tz) = client_tz {
            if !tz.is_empty() {
                return tz.to_string();
            }
        }
    }

    // 4. Home.
    home_tz
}

/// Get all ontology data sources for a specific date (registry-driven).
///
/// Iterates over all registered ontologies that have a `DaySourceConfig` and builds
/// dynamic SQL queries from the config. No arbitrary LIMITs — all data included
/// with a sanity check for overflow.
pub async fn get_day_sources(
    pool: &PgPool,
    date: NaiveDate,
    client_tz: Option<&str>,
) -> Result<Vec<DaySource>> {
    use sqlx::Row;
    use virtues_registry::ontologies::registered_ontologies;

    // Day window in the per-day "where the owner was" timezone, fixed at the
    // day's start ("the timezone you woke up in"). Resolution order:
    //   1. the locked wiki_days.start_timezone for this day (past days), else
    //   2. the viewing device's zone for an in-progress today (client_tz), else
    //   3. tzf-rs(first located point of the day) → home_timezone fallback.
    // See agents/record/timezone-model.md.
    let timezone = resolve_render_timezone(pool, date, client_tz).await;
    let (start_str, end_str) =
        super::day_summary::day_boundaries_utc(date, Some(&timezone));
    let mut sources: Vec<DaySource> = Vec::new();

    for ont in registered_ontologies() {
        let cfg = match &ont.day_source {
            Some(c) => c,
            None => continue,
        };

        // Build SELECT columns
        let source_type_col = cfg
            .source_type_sql
            .map(|sql| format!("{} as source_type_dyn", sql))
            .unwrap_or_else(|| format!("'{}' as source_type_dyn", cfg.source_type));

        let query = if cfg.use_date_filter {
            format!(
                "SELECT {id} as src_id, {ts} as src_ts, {label} as src_label, {preview} as src_preview, {st} \
                 FROM {table} t \
                 WHERE date(t.{ts_col}) = $1 \
                 {extra} \
                 ORDER BY t.{ts_col} ASC",
                id = cfg.id_sql,
                ts = ont.timestamp_column,
                label = cfg.label_sql,
                preview = cfg.preview_sql,
                st = source_type_col,
                table = ont.table_name,
                ts_col = ont.timestamp_column,
                extra = cfg.extra_where.unwrap_or(""),
            )
        } else {
            format!(
                "SELECT {id} as src_id, {ts} as src_ts, {label} as src_label, {preview} as src_preview, {st} \
                 FROM {table} t \
                 WHERE t.{ts_col} >= $1::timestamptz AND t.{ts_col} <= $2::timestamptz \
                 {extra} \
                 ORDER BY t.{ts_col} ASC",
                id = cfg.id_sql,
                ts = ont.timestamp_column,
                label = cfg.label_sql,
                preview = cfg.preview_sql,
                st = source_type_col,
                table = ont.table_name,
                ts_col = ont.timestamp_column,
                extra = cfg.extra_where.unwrap_or(""),
            )
        };

        let rows = if cfg.use_date_filter {
            sqlx::query(&query)
                .bind(date)
                .fetch_all(pool)
                .await
        } else {
            sqlx::query(&query)
                .bind(&start_str)
                .bind(&end_str)
                .fetch_all(pool)
                .await
        };

        // A day-source query that fails is not a warning. It means the day is being
        // assembled with a HOLE in it — and then an LLM writes a confident account
        // of a day it was never shown.
        //
        // Two of these were broken on the box for as long as they have existed:
        //
        //   location_visit       `encode(t.id,'hex')` on a TEXT id
        //   activity_app_session `extra_where` missing its leading AND
        //
        // Both raised here, both were swallowed with `warn!` + `continue`, and the
        // cron reported SUCCESS every single night. The result: 103 days of a real
        // life produced 2 events and zero autobiographies, and nothing anywhere said
        // a word about it.
        //
        // A missing source is a broken query, and a broken query is a bug to fix —
        // never a day to fabricate around it.
        let rows = rows.map_err(|e| {
            Error::Database(format!(
                "day source query failed for ontology `{}` — the day cannot be \
                 assembled without it, and generating a narrative from the gap would \
                 invent a day you did not live: {e}",
                ont.name
            ))
        })?;

        // Sanity check
        if rows.len() > 5000 {
            tracing::warn!(
                ontology = ont.name,
                count = rows.len(),
                "Unusually large source count for single day"
            );
        }

        for row in &rows {
            let id: String = match row.try_get("src_id") {
                Ok(v) => v,
                Err(_) => continue,
            };
            // `src_ts` aliases a TIMESTAMPTZ column — decode it directly. Reading
            // it as String (then re-parsing) failed at the decode step and
            // `continue`d past every row, so these ontologies never appeared.
            let ts: DateTime<Utc> = match row.try_get("src_ts") {
                Ok(v) => v,
                Err(_) => continue,
            };
            let label: String = row.try_get("src_label").unwrap_or_else(|_| ont.display_name.to_string());
            let preview: Option<String> = row.try_get("src_preview").ok().flatten();
            let source_type: String = row.try_get("source_type_dyn").unwrap_or_else(|_| cfg.source_type.to_string());

            sources.push(DaySource {
                source_type,
                id,
                timestamp: ts,
                label,
                preview,
                continuous: ont.temporal_type
                    == virtues_registry::ontologies::TemporalType::Continuous,
            });
        }
    }

    sources.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    Ok(sources)
}

// ============================================================================
// Today Streams - the three raw record streams, as spans, before synthesis
// ============================================================================
//
// The homepage renders the day *before* the nightly synthesis has read it into
// a biography. At that point the box does not have "events" — it has three
// sensor streams, each with real start/end spans: where the phone was
// (data_location_visit), what the calendar promised (data_calendar_event), and
// when the microphone was open (data_audio_recording — the raw live chunks, NOT
// the nightly `data_audio_session` rollup, which doesn't exist mid-day). This
// endpoint returns exactly those three, tz-anchored, drawn as rectangles.

/// A location visit span (where you were, and for how long).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodayLocationSpan {
    pub id: String,
    pub start_time: String,
    pub end_time: String,
    pub place_name: Option<String>,
    pub place_category: Option<String>,
    pub duration_minutes: Option<i32>,
}

/// A calendar event span (the day as intended).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodayCalendarSpan {
    pub id: String,
    pub start_time: String,
    pub end_time: String,
    pub title: String,
    pub is_all_day: bool,
    pub is_sacred: bool,
    pub location_name: Option<String>,
    pub calendar_name: Option<String>,
}

/// A raw audio recording chunk (~5 min each) — the live mic capture, before any
/// sessionization. `is_silent` marks a chunk the box flagged as silence. The
/// client merges contiguous chunks into "mic was open" blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodayAudioSpan {
    pub id: String,
    pub start_time: String,
    pub end_time: String,
    pub is_silent: bool,
}

/// The three raw streams for a day, before the nightly synthesis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodayStreamsView {
    pub date: String,
    /// The zone the spans are anchored to (see agents/record/timezone-model.md).
    pub timezone: String,
    pub location: Vec<TodayLocationSpan>,
    pub calendar: Vec<TodayCalendarSpan>,
    pub audio: Vec<TodayAudioSpan>,
}

/// One heart-rate sample, for the day page's Autonomic chart.
#[derive(Debug, serde::Serialize)]
pub struct DayHeartRateSample {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub bpm: i32,
}

/// Raw heart-rate samples across a day's window, oldest first. The client
/// draws; sparse days (a dozen samples) are normal and the chart must read
/// honestly at that density — dots joined by a line, never a smoothed curve
/// that invents continuity the record does not hold.
pub async fn get_day_heart_rate(
    pool: &PgPool,
    date: NaiveDate,
    client_tz: Option<&str>,
) -> Result<Vec<DayHeartRateSample>> {
    let timezone = resolve_render_timezone(pool, date, client_tz).await;
    let (start_str, end_str) = super::day_summary::day_boundaries_utc(date, Some(&timezone));

    let rows: Vec<(chrono::DateTime<chrono::Utc>, i32)> = sqlx::query_as(
        r#"SELECT occurred_at, bpm FROM data_health_heart_rate
           WHERE occurred_at >= $1::timestamptz AND occurred_at < $2::timestamptz
           ORDER BY occurred_at"#,
    )
    .bind(&start_str)
    .bind(&end_str)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to load heart rate: {}", e)))?;

    Ok(rows
        .into_iter()
        .map(|(timestamp, bpm)| DayHeartRateSample { timestamp, bpm })
        .collect())
}

/// Get the three raw record streams (location, calendar, audio) for a day as
/// spans. Anchored to the day's effective timezone exactly like `get_day_sources`.
pub async fn get_today_streams(
    pool: &PgPool,
    date: NaiveDate,
    client_tz: Option<&str>,
) -> Result<TodayStreamsView> {
    use sqlx::Row;

    let timezone = resolve_render_timezone(pool, date, client_tz).await;
    let (start_str, end_str) = super::day_summary::day_boundaries_utc(date, Some(&timezone));

    // --- Location: where the phone was ---
    let loc_rows = sqlx::query(
        r#"
        SELECT
            v.id               AS id,
            v.started_at     AS started_at,
            v.ended_at   AS ended_at,
            v.duration_minutes AS duration_minutes,
            p.name             AS place_name,
            p.category         AS place_category
        FROM data_location_visit v
        LEFT JOIN wiki_refs er
            ON er.source_table = 'data_location_visit'
           AND er.source_id    = v.id
           AND er.entity_type  = 'place'
        LEFT JOIN wiki_places p ON p.id = er.entity_id
        WHERE v.started_at >= $1::timestamptz AND v.started_at < $2::timestamptz
        ORDER BY v.started_at ASC
        "#,
    )
    .bind(&start_str)
    .bind(&end_str)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("today streams: location query failed: {e}")))?;

    let location: Vec<TodayLocationSpan> = loc_rows
        .iter()
        .filter_map(|row| {
            let arrival: DateTime<Utc> = row.try_get("started_at").ok()?;
            let departure: Option<DateTime<Utc>> =
                row.try_get::<Option<DateTime<Utc>>, _>("ended_at").ok().flatten();
            Some(TodayLocationSpan {
                id: row.try_get("id").ok()?,
                start_time: arrival.to_rfc3339(),
                end_time: departure.unwrap_or(arrival).to_rfc3339(),
                place_name: row.try_get("place_name").ok().flatten(),
                place_category: row.try_get("place_category").ok().flatten(),
                duration_minutes: row.try_get("duration_minutes").ok(),
            })
        })
        .collect();

    // --- Calendar: the day as intended ---
    let cal_rows = sqlx::query(
        r#"
        SELECT id, title, started_at, ended_at, is_all_day,
               COALESCE(is_sacred, FALSE) AS is_sacred, location_name, calendar_name
        FROM data_calendar_event
        WHERE started_at >= $1::timestamptz AND started_at < $2::timestamptz
        ORDER BY started_at ASC
        "#,
    )
    .bind(&start_str)
    .bind(&end_str)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("today streams: calendar query failed: {e}")))?;

    let calendar: Vec<TodayCalendarSpan> = cal_rows
        .iter()
        .filter_map(|row| {
            let start: DateTime<Utc> = row.try_get("started_at").ok()?;
            let end: DateTime<Utc> = row.try_get("ended_at").ok()?;
            Some(TodayCalendarSpan {
                id: row.try_get("id").ok()?,
                start_time: start.to_rfc3339(),
                end_time: end.to_rfc3339(),
                title: row.try_get("title").unwrap_or_else(|_| "(no title)".to_string()),
                is_all_day: row.try_get("is_all_day").unwrap_or(false),
                is_sacred: row.try_get("is_sacred").unwrap_or(false),
                location_name: row.try_get("location_name").ok().flatten(),
                calendar_name: row.try_get("calendar_name").ok().flatten(),
            })
        })
        .collect();

    // --- Audio: raw recording chunks (the live mic capture) ---
    let aud_rows = sqlx::query(
        r#"
        SELECT id, started_at, ended_at, duration_seconds, COALESCE(is_silent, FALSE) AS is_silent
        FROM data_audio_recording
        WHERE started_at >= $1::timestamptz AND started_at < $2::timestamptz
        ORDER BY started_at ASC
        "#,
    )
    .bind(&start_str)
    .bind(&end_str)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("today streams: audio query failed: {e}")))?;

    let audio: Vec<TodayAudioSpan> = aud_rows
        .iter()
        .filter_map(|row| {
            let start: DateTime<Utc> = row.try_get("started_at").ok()?;
            let ended: Option<DateTime<Utc>> =
                row.try_get::<Option<DateTime<Utc>>, _>("ended_at").ok().flatten();
            let dur_s: Option<f64> = row.try_get("duration_seconds").ok().flatten();
            // Fall back to the chunk's duration (or a nominal 5 min) when it has no end.
            let end = ended.unwrap_or_else(|| {
                start + chrono::Duration::milliseconds((dur_s.unwrap_or(300.0) * 1000.0) as i64)
            });
            Some(TodayAudioSpan {
                id: row.try_get("id").ok()?,
                start_time: start.to_rfc3339(),
                end_time: end.to_rfc3339(),
                is_silent: row.try_get("is_silent").unwrap_or(false),
            })
        })
        .collect();

    Ok(TodayStreamsView {
        date: date.to_string(),
        timezone,
        location,
        calendar,
        audio,
    })
}

// ============================================================================
// Day Streams - Dynamic Ontology Queries
// ============================================================================

/// A single record from an ontology table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamRecord {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub end_timestamp: Option<DateTime<Utc>>,
    pub preview: serde_json::Value,
}

/// Data stream from a single ontology
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayStream {
    pub ontology_name: String,
    pub display_name: String,
    pub domain: String,
    pub count: usize,
    pub records: Vec<StreamRecord>,
}

/// Response for GET /api/wiki/day/{date}/streams
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayStreamsResponse {
    pub date: NaiveDate,
    pub queried_at: DateTime<Utc>,
    pub streams: Vec<DayStream>,
    pub total_count: usize,
}

/// Get all ontology data streams for a specific date
///
/// Dynamically queries all registered ontology tables using their
/// timestamp_column metadata to filter records for the given day.
pub async fn get_day_streams(pool: &PgPool, date: NaiveDate) -> Result<DayStreamsResponse> {
    use virtues_registry::ontologies::registered_ontologies;
    use sqlx::Row;

    // Calculate UTC bounds for the date
    // Expand window to cover any timezone: UTC-12 to UTC+14
    let start = date
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .checked_sub_signed(chrono::Duration::hours(12))
        .unwrap();
    let end = date
        .succ_opt()
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .checked_add_signed(chrono::Duration::hours(14))
        .unwrap();

    let ontologies = registered_ontologies();
    let mut streams = Vec::new();

    for ontology in &ontologies {
        // Skip non-time-series ontologies
        if should_skip_ontology_for_streams(ontology.name) {
            continue;
        }

        let table = ontology.table_name;
        let ts_col = ontology.timestamp_column;

        // Build SELECT clause for end timestamp if present
        let end_select = ontology
            .end_timestamp_column
            .map(|c| format!(", {} as end_ts", c))
            .unwrap_or_default();

        // All ontology tables (incl. location_visit) use TEXT UUID ids — see the
        // `data_location_visit` join comment above. An earlier version assumed
        // location_visit had blob ids and wrapped them in `encode(id, 'hex')`,
        // but `encode()` only accepts `bytea`, so that query failed at runtime
        // with "function encode(text, unknown) does not exist" — silently
        // breaking the day page's location rendering. Select the id directly.
        let id_select = "id";

        // Build dynamic query - select id, timestamps, and all other columns as JSON
        let sql = format!(
            "SELECT {id_select}, {ts_col} as ts{end_select}, * FROM {table}
             WHERE {ts_col} >= $1 AND {ts_col} < $2
             ORDER BY {ts_col} ASC
             LIMIT 100",
            id_select = id_select,
            ts_col = ts_col,
            end_select = end_select,
            table = table,
        );

        // Execute query with dynamic SQL
        let rows = match sqlx::query(&sql)
            .bind(start)
            .bind(end)
            .fetch_all(pool)
            .await
        {
            Ok(rows) => rows,
            Err(e) => {
                tracing::warn!(
                    "Failed to query {} for day streams: {}",
                    ontology.name,
                    e
                );
                continue;
            }
        };

        if rows.is_empty() {
            continue;
        }

        let mut records = Vec::new();
        for row in &rows {
            // Get id
            let id: String = row.try_get("id").unwrap_or_default();
            if id.is_empty() {
                continue;
            }

            // Get timestamp — `ts` aliases a TIMESTAMPTZ column, decode directly.
            let timestamp: DateTime<Utc> = match row.try_get("ts") {
                Ok(ts) => ts,
                Err(_) => continue,
            };

            // Get end timestamp if present
            let end_timestamp = if ontology.end_timestamp_column.is_some() {
                row.try_get::<Option<DateTime<Utc>>, _>("end_ts").ok().flatten()
            } else {
                None
            };

            // Build preview from key columns based on ontology type
            let preview = build_preview_for_ontology(ontology.name, &row);

            records.push(StreamRecord {
                id,
                timestamp,
                end_timestamp,
                preview,
            });
        }

        if !records.is_empty() {
            streams.push(DayStream {
                ontology_name: ontology.name.to_string(),
                display_name: ontology.display_name.to_string(),
                domain: ontology.domain.to_string(),
                count: records.len(),
                records,
            });
        }
    }

    // Sort streams by domain for consistent ordering
    streams.sort_by(|a, b| a.domain.cmp(&b.domain));

    let total_count = streams.iter().map(|s| s.count).sum();

    Ok(DayStreamsResponse {
        date,
        queried_at: Utc::now(),
        streams,
        total_count,
    })
}

/// Check if an ontology should be skipped for day streams
fn should_skip_ontology_for_streams(name: &str) -> bool {
    // Skip entity tables (not time-series events)
    name.starts_with("entities_")
        // Skip financial accounts (reference data, not events)
        || name == "financial_account"
        // Skip location points (use visits instead)
        || name == "location_point"
}

/// Build a preview JSON object for a specific ontology type
fn build_preview_for_ontology(ontology_name: &str, row: &sqlx::postgres::PgRow) -> serde_json::Value {
    use sqlx::Row;

    match ontology_name {
        "calendar_event" => {
            serde_json::json!({
                "title": row.try_get::<String, _>("title").ok(),
                "location": row.try_get::<String, _>("location_name").ok(),
            })
        }
        "communication_email" => {
            serde_json::json!({
                "subject": row.try_get::<String, _>("subject").ok(),
                "from": row.try_get::<String, _>("from_email").ok(),
                "direction": row.try_get::<String, _>("direction").ok(),
            })
        }
        "communication_message" => {
            let body: Option<String> = row.try_get("body").ok();
            let preview = body.map(|c| {
                let truncated: String = c.chars().take(100).collect();
                if truncated.len() < c.len() {
                    format!("{truncated}...")
                } else {
                    truncated
                }
            });
            serde_json::json!({
                "from": row.try_get::<String, _>("from_name").ok(),
                "channel": row.try_get::<String, _>("channel").ok(),
                "preview": preview,
            })
        }
        "location_visit" => {
            serde_json::json!({
                "place_name": row.try_get::<String, _>("place_name").ok(),
                "duration_minutes": row.try_get::<i32, _>("duration_minutes").ok(),
            })
        }
        "health_workout" => {
            serde_json::json!({
                "workout_type": row.try_get::<String, _>("workout_type").ok(),
                "duration_minutes": row.try_get::<i32, _>("duration_minutes").ok(),
                "calories": row.try_get::<i32, _>("calories_burned").ok(),
            })
        }
        "health_sleep" => {
            serde_json::json!({
                "duration_minutes": row.try_get::<i32, _>("duration_minutes").ok(),
                "quality_score": row.try_get::<f64, _>("sleep_quality_score").ok(),
            })
        }
        "health_heart_rate" => {
            serde_json::json!({
                "bpm": row.try_get::<i32, _>("bpm").ok(),
            })
        }
        "health_steps" => {
            serde_json::json!({
                "step_count": row.try_get::<i32, _>("step_count").ok(),
            })
        }
        "financial_transaction" => {
            let amount_cents: Option<i64> = row.try_get("amount").ok();
            serde_json::json!({
                "merchant": row.try_get::<String, _>("merchant_name").ok(),
                "amount": amount_cents.map(|c| c as f64 / 100.0),
                "category": row.try_get::<String, _>("merchant_category").ok(),
            })
        }
        "activity_app_usage" => {
            serde_json::json!({
                "app_name": row.try_get::<String, _>("app_name").ok(),
                "window_title": row.try_get::<String, _>("window_title").ok(),
            })
        }
        "activity_web_browsing" => {
            serde_json::json!({
                "domain": row.try_get::<String, _>("domain").ok(),
                "page_title": row.try_get::<String, _>("page_title").ok(),
            })
        }
        "content_conversation" => {
            let content: Option<String> = row.try_get("content").ok();
            let preview = content.map(|c| {
                let truncated: String = c.chars().take(100).collect();
                if truncated.len() < c.len() {
                    format!("{truncated}...")
                } else {
                    truncated
                }
            });
            serde_json::json!({
                "role": row.try_get::<String, _>("role").ok(),
                "provider": row.try_get::<String, _>("provider").ok(),
                "preview": preview,
            })
        }
        "content_document" => {
            serde_json::json!({
                "title": row.try_get::<String, _>("title").ok(),
                "document_type": row.try_get::<String, _>("document_type").ok(),
            })
        }
        "communication_transcription" => {
            let text: Option<String> = row.try_get("text").ok();
            let preview = text.map(|t| {
                let truncated: String = t.chars().take(100).collect();
                if truncated.len() < t.len() {
                    format!("{truncated}...")
                } else {
                    truncated
                }
            });
            serde_json::json!({
                "duration_seconds": row.try_get::<f64, _>("duration_seconds").ok(),
                "preview": preview,
            })
        }
        _ => {
            // Generic fallback - just return empty object
            serde_json::json!({})
        }
    }
}

// ============================================================================
// Day Chats - In-app Virtues chats + external AI conversations
// ============================================================================

/// A single chat conversation surfaced on a day's wiki page.
///
/// Unifies two sources:
/// - In-app Virtues chats (table: `chats`) — navigable, source = "virtues"
/// - External AI conversations from ontology imports (table:
///   `data_content_conversation`) — Claude.ai, Gemini, ChatGPT, etc.
///   Not navigable; only displayed with a provider badge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayChat {
    pub id: String,
    /// "virtues" for in-app chats, "external" for ontology-imported chats.
    pub source: String,
    /// External provider name (e.g. "claude", "gemini", "chatgpt"). None for in-app.
    pub provider: Option<String>,
    pub title: String,
    pub message_count: i64,
    pub started_at: DateTime<Utc>,
}

/// Get all AI chats (in-app + external) that started on the given day.
///
/// Day window matches `get_day_sources`: UTC midnight → noon next day,
/// which covers every timezone.
pub async fn get_day_chats(pool: &PgPool, date: NaiveDate) -> Result<Vec<DayChat>> {
    use sqlx::Row;

    let start_of_day = date.and_hms_opt(0, 0, 0).unwrap().and_utc();
    let end_of_day = date
        .succ_opt()
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap()
        .and_utc();
    let mut chats: Vec<DayChat> = Vec::new();

    // ── In-app Virtues chats ────────────────────────────────────────────────
    let in_app_rows = sqlx::query(
        r#"
        SELECT id, title, message_count, created_at
        FROM app_chats
        WHERE created_at >= $1 AND created_at <= $2
        ORDER BY created_at ASC
        "#,
    )
    .bind(start_of_day)
    .bind(end_of_day)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to query in-app chats: {}", e)))?;

    for row in &in_app_rows {
        let id: String = match row.try_get("id") {
            Ok(v) => v,
            Err(_) => continue,
        };
        let title: String = row.try_get("title").unwrap_or_else(|_| "Untitled chat".to_string());
        let message_count: i64 = row.try_get("message_count").unwrap_or(0);
        let started_at: DateTime<Utc> = match row.try_get("created_at") {
            Ok(v) => v,
            Err(_) => continue,
        };
        chats.push(DayChat {
            id,
            source: "virtues".to_string(),
            provider: None,
            title,
            message_count,
            started_at,
        });
    }

    // ── External AI conversations (ontology-imported) ───────────────────────
    // Group messages by conversation_id in Rust to keep SQL simple. Excludes
    // any rows with source_provider='virtues' so we don't double-count an
    // in-app chat that was also synced into the ontology lake.
    let ext_rows = sqlx::query(
        r#"
        SELECT conversation_id, role, content, provider, occurred_at
        FROM data_content_conversation
        WHERE occurred_at >= $1 AND occurred_at <= $2
          AND source_provider != 'virtues'
        ORDER BY conversation_id, occurred_at ASC
        "#,
    )
    .bind(start_of_day)
    .bind(end_of_day)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to query external chats: {}", e)))?;

    use std::collections::BTreeMap;
    struct ExtAccum {
        provider: Option<String>,
        first_ts: Option<DateTime<Utc>>,
        first_user_content: Option<String>,
        count: i64,
    }
    let mut groups: BTreeMap<String, ExtAccum> = BTreeMap::new();

    for row in &ext_rows {
        let conv_id: String = match row.try_get("conversation_id") {
            Ok(v) => v,
            Err(_) => continue,
        };
        let role: String = row.try_get("role").unwrap_or_default();
        let content: String = row.try_get("content").unwrap_or_default();
        let provider: Option<String> = row.try_get("provider").ok();
        // `timestamp` is a TIMESTAMPTZ column — decode directly. Reading it as
        // String failed at decode and `continue`d past every message, so
        // external AI conversations never showed up on the day page.
        let ts: DateTime<Utc> = match row.try_get("occurred_at") {
            Ok(v) => v,
            Err(_) => continue,
        };

        let entry = groups.entry(conv_id).or_insert(ExtAccum {
            provider: None,
            first_ts: None,
            first_user_content: None,
            count: 0,
        });
        entry.count += 1;
        if entry.provider.is_none() {
            entry.provider = provider;
        }
        if entry.first_ts.map(|t| ts < t).unwrap_or(true) {
            entry.first_ts = Some(ts);
        }
        if role == "user" && entry.first_user_content.is_none() && !content.trim().is_empty() {
            entry.first_user_content = Some(content);
        }
    }

    for (conv_id, acc) in groups {
        let started_at = match acc.first_ts {
            Some(t) => t,
            None => continue,
        };
        let title = acc
            .first_user_content
            .as_deref()
            .map(truncate_title)
            .unwrap_or_else(|| match acc.provider.as_deref() {
                Some(p) => format!("{} conversation", p),
                None => "AI conversation".to_string(),
            });
        chats.push(DayChat {
            id: conv_id,
            source: "external".to_string(),
            provider: acc.provider,
            title,
            message_count: acc.count,
            started_at,
        });
    }

    chats.sort_by(|a, b| a.started_at.cmp(&b.started_at));
    Ok(chats)
}

/// Truncate the first user message to a short, single-line title.
fn truncate_title(s: &str) -> String {
    let first_line = s.lines().next().unwrap_or("").trim();
    let chars: Vec<char> = first_line.chars().collect();
    if chars.len() <= 80 {
        first_line.to_string()
    } else {
        let truncated: String = chars.iter().take(80).collect();
        format!("{}…", truncated.trim_end())
    }
}

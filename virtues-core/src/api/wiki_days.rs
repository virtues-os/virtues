//! A DAY, as the wiki holds it: the row, what arrived on it, and the shape of
//! where the person was.
//!
//! The day is the wiki's base resolution — every rung above it (a year, a
//! chapter, a life) is assembled from days, and the day itself is assembled
//! from the records beneath it by event resolution. What lives here is the
//! day as a SUBJECT: getting or creating its row, listing days, how much
//! arrived on each, and the location chunks the movement map draws.
//!
//! The day's prose is not here — an article is an article, whatever it is
//! about, and lives in [`crate::api::wiki_articles`]. Its events are in
//! [`crate::api::wiki_events`]; its raw records in
//! [`crate::api::wiki_streams`].

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::error::{Error, Result};
use crate::ids;

/// A day wiki page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiDay {
    pub id: String,
    pub date: NaiveDate,
    pub start_timezone: Option<String>,
    /// The day's prose, from `wiki_day_prose`. The article page is its only
    /// home — the legacy `autobiography` column was dropped in 0106.
    pub article: Option<String>,
    // Gone with the columns behind them (0025): `last_edited_by` was the
    // one-pen freeze flag, which ownership no longer has; `cover_image` and
    // `snapshot` never had a writer; `data_quality` and `epigraph` were parsed
    // out of a response the narrate prompt forbids the model to produce, so
    // both were NULL on every row of every box.
    //
    // The comment this replaces said it already: a field that serializes a
    // permanent None to a client is schema drift hiding in plain sight, and
    // `try_get(...).ok()` is what lets it hide. Prefer removal to tolerance.
    /// Count of entities first referenced on this day
    pub new_entity_count: i64,
    /// Count of topics first seen on this day
    pub new_topic_count: i64,
    /// Sleep cycles with autonomic scores, computed at query time from
    /// data_health_sleep stages + heart rate data. Not stored.
    pub sleep_cycles: Vec<ScoredSleepCycle>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A single sleep cycle with autonomic scoring, derived from sleep stage
/// boundaries and heart rate data during the cycle window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredSleepCycle {
    pub start_time: String,
    pub end_time: String,
    pub dominant_stage: String,
    pub avg_hr: Option<f64>,
    pub autonomic_z: Option<f64>,
}

// ============================================================================
// Telos CRUD Operations
// ============================================================================



// ============================================================================
// Act CRUD Operations
// ============================================================================



// ============================================================================
// Chapter CRUD Operations
// ============================================================================



// ============================================================================
// Day CRUD Operations
// ============================================================================

/// Get a day by date (creates if not exists)
pub async fn get_or_create_day(pool: &PgPool, date: NaiveDate) -> Result<WikiDay> {
    let date_str = date.format("%Y-%m-%d").to_string();

    // Try to get existing day
    let existing: Option<sqlx::postgres::PgRow> = sqlx::query(
        r#"
        SELECT
            id, date, start_timezone,
            (SELECT dp.prose FROM wiki_day_prose dp WHERE dp.day_id = wiki_days.id) AS article,
            created_at, updated_at
        FROM wiki_days
        WHERE date = $1
        "#,
    )
    .bind(date)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get day: {}", e)))?;

    if let Some(row) = existing {
        let (ne, nt) = get_day_novelty_counts(pool, &date_str).await?;
        let mut day = wiki_day_from_row_with_counts(&row, date, ne, nt)?;
        day.sleep_cycles = compute_sleep_cycles(pool, date).await?;
        return Ok(day);
    }

    // Create new day
    let day_id = ids::generate_id(ids::WIKI_DAY_PREFIX, &[&date_str]);
    let row: sqlx::postgres::PgRow = sqlx::query(
        r#"
        INSERT INTO wiki_days (id, date)
        VALUES ($1, $2)
        RETURNING
            id, date, start_timezone,
            created_at, updated_at
        "#,
    )
    .bind(&day_id)
    .bind(date)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create day: {}", e)))?;

    wiki_day_from_row(&row, date)
}

/// Parse a WikiDay from a raw `PgRow`
fn wiki_day_from_row(row: &sqlx::postgres::PgRow, date: NaiveDate) -> Result<WikiDay> {
    wiki_day_from_row_with_counts(row, date, 0, 0)
}

fn wiki_day_from_row_with_counts(row: &sqlx::postgres::PgRow, date: NaiveDate, new_entity_count: i64, new_topic_count: i64) -> Result<WikiDay> {
    use sqlx::Row;

    let id: String = row
        .try_get("id")
        .map_err(|e| Error::Database(format!("Missing day ID: {e}")))?;
    Ok(WikiDay {
        id,
        date,
        start_timezone: row.try_get("start_timezone").ok().flatten(),
        // Absent from the INSERT..RETURNING path (a just-created day has no
        // prose anyway) — `.ok()` makes that read as None rather than an error.
        article: row.try_get("article").ok().flatten(),
        new_entity_count,
        new_topic_count,
        sleep_cycles: vec![], // populated after construction
        created_at: row.try_get("created_at").unwrap_or_else(|_| Utc::now()),
        updated_at: row.try_get("updated_at").unwrap_or_else(|_| Utc::now()),
    })
}

/// Compute scored sleep cycles for a day from sleep stage data + heart rate readings.
/// Derives cycle boundaries by splitting sleep_stages at "awake" entries,
/// then computes avg HR per cycle and z-scores against a 14-day sleep HR baseline.
async fn compute_sleep_cycles(pool: &PgPool, date: NaiveDate) -> Result<Vec<ScoredSleepCycle>> {
    Ok({
        use sqlx::Row;

        let start = date.and_hms_opt(0, 0, 0).unwrap().and_utc();
        let end = (date + chrono::Duration::days(1))
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();

        // 1. Get sleep record for this night (overlaps with this calendar day)
        let sleep_row: Option<sqlx::postgres::PgRow> = sqlx::query(
            r#"SELECT sleep_stages FROM data_health_sleep
           WHERE started_at >= $1
             AND started_at < $2
           ORDER BY started_at ASC LIMIT 1"#,
        )
        .bind(start)
        .bind(end)
        .fetch_optional(pool)
        .await?;

        // sleep_stages is JSONB in pg — sqlx decodes directly into serde_json::Value.
        let stages: Vec<serde_json::Value> = match sleep_row {
            Some(row) => match row.try_get::<Option<serde_json::Value>, _>("sleep_stages") {
                Ok(Some(serde_json::Value::Array(arr))) => arr,
                _ => return Ok(vec![]),
            },
            None => return Ok(vec![]),
        };

        // Group consecutive non-awake stages into cycles
        let mut cycles: Vec<(String, String, String)> = vec![]; // (start, end, dominant_stage)
        let mut cycle_start: Option<String> = None;
        let mut cycle_end: Option<String> = None;
        let mut stage_durations: std::collections::HashMap<String, i64> =
            std::collections::HashMap::new();

        for stage in &stages {
            let stage_name = stage["stage"].as_str().unwrap_or("unknown");
            let start = stage["start"].as_str().unwrap_or("");
            let end = stage["end"].as_str().unwrap_or("");

            if stage_name == "awake" {
                // Close current cycle if we have one
                if let (Some(cs), Some(ce)) = (&cycle_start, &cycle_end) {
                    let dominant = stage_durations
                        .iter()
                        .max_by_key(|(_, v)| *v)
                        .map(|(k, _)| k.clone())
                        .unwrap_or_else(|| "core".to_string());
                    cycles.push((cs.clone(), ce.clone(), dominant));
                    cycle_start = None;
                    cycle_end = None;
                    stage_durations.clear();
                }
            } else {
                if cycle_start.is_none() {
                    cycle_start = Some(start.to_string());
                }
                cycle_end = Some(end.to_string());

                // Estimate duration in minutes for dominant stage calculation
                if let (Ok(s), Ok(e)) = (
                    DateTime::parse_from_rfc3339(start),
                    DateTime::parse_from_rfc3339(end),
                ) {
                    let mins = (e - s).num_minutes();
                    let key = stage_name.replace("asleep_", "");
                    *stage_durations.entry(key).or_insert(0) += mins;
                }
            }
        }
        // Close final cycle
        if let (Some(cs), Some(ce)) = (&cycle_start, &cycle_end) {
            let dominant = stage_durations
                .iter()
                .max_by_key(|(_, v)| *v)
                .map(|(k, _)| k.clone())
                .unwrap_or_else(|| "core".to_string());
            cycles.push((cs.clone(), ce.clone(), dominant));
        }

        if cycles.is_empty() {
            return Ok(vec![]);
        }

        // 3. Get 14-day sleep HR baseline (median of nightly avg HRs)
        let baseline_start = (date - chrono::Duration::days(14))
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let baseline_hrs: Vec<f64> = sqlx::query_scalar(
            r#"SELECT AVG(CAST(hr.bpm AS REAL))
           FROM data_health_heart_rate hr
           INNER JOIN data_health_sleep s
             ON hr.occurred_at >= s.started_at AND hr.occurred_at < s.ended_at
           WHERE s.started_at >= $1
             AND s.started_at < $2
           GROUP BY s.id"#,
        )
        .bind(baseline_start)
        .bind(end)
        .fetch_all(pool)
        .await?;

        let (baseline_mean, baseline_std) = if baseline_hrs.len() >= 2 {
            let mean = baseline_hrs.iter().sum::<f64>() / baseline_hrs.len() as f64;
            let variance = baseline_hrs.iter().map(|x| (x - mean).powi(2)).sum::<f64>()
                / baseline_hrs.len() as f64;
            let std = variance.sqrt().max(1.0); // floor at 1 bpm to avoid div-by-zero
            (mean, std)
        } else {
            (0.0, 0.0) // insufficient baseline
        };

        // 4. Score each cycle
        let mut scored: Vec<ScoredSleepCycle> = vec![];
        for (start, end, dominant) in &cycles {
            // The bounds arrive as RFC3339 TEXT from the stage JSON, and
            // `occurred_at` is a timestamptz. Binding the strings asks Postgres
            // for `timestamp with time zone >= text`, which it refuses — and
            // because `sqlx::query` is untyped, that shipped as a runtime error
            // instead of a compile one. Every day that HAS sleep cycles failed
            // here, so day_summary_eod could not narrate it and retried the
            // same day every hour, forever.
            let window = match (
                DateTime::parse_from_rfc3339(start),
                DateTime::parse_from_rfc3339(end),
            ) {
                (Ok(s), Ok(e)) => Some((s.with_timezone(&Utc), e.with_timezone(&Utc))),
                // Unparseable bounds cost this cycle its heart rate, not its
                // row: the cycle itself is real and still worth reporting.
                _ => None,
            };

            // Get avg HR during this cycle window
            let avg_hr: Option<f64> = match window {
                Some((from, to)) => sqlx::query_scalar(
                    r#"SELECT AVG(CAST(bpm AS REAL))
               FROM data_health_heart_rate
               WHERE occurred_at >= $1 AND occurred_at < $2"#,
                )
                .bind(from)
                .bind(to)
                .fetch_optional(pool)
                .await
                .map_err(|e| {
                    Error::Database(format!("Failed to read sleep-cycle heart rate: {e}"))
                })?
                .flatten(),
                None => None,
            };

            let autonomic_z = match (avg_hr, baseline_std > 0.0) {
                (Some(hr), true) => {
                    // For sleep: lower HR = better recovery = more negative z
                    let z = (hr - baseline_mean) / baseline_std;
                    Some(z.clamp(-3.0, 3.0))
                }
                _ => None,
            };

            scored.push(ScoredSleepCycle {
                start_time: start.clone(),
                end_time: end.clone(),
                dominant_stage: dominant.clone(),
                avg_hr,
                autonomic_z,
            });
        }

        scored
    })
}

/// Count new entities and new topics for a date.
/// "New entity" = an entity whose earliest wiki_refs.timestamp falls on this date.
/// "New topic" = a topic in search_topic_cache whose created_at falls on this date.
async fn get_day_novelty_counts(pool: &PgPool, date_str: &str) -> Result<(i64, i64)> {
    // New entities: count distinct entity_ids where their earliest ref timestamp is on this date
    let next_date = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .map(|d| (d + chrono::Duration::days(1)).format("%Y-%m-%d").to_string())
        .unwrap_or_default();
    let new_entities: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(DISTINCT r.entity_id)
           FROM wiki_refs r
           WHERE r.occurred_at >= ($1 || 'T00:00:00Z')::timestamptz
             AND r.occurred_at < ($2 || 'T00:00:00Z')::timestamptz
             AND NOT EXISTS (
               SELECT 1 FROM wiki_refs r2
               WHERE r2.entity_id = r.entity_id
                 AND r2.occurred_at < ($1 || 'T00:00:00Z')::timestamptz
             )"#,
    )
    .bind(date_str)
    .bind(&next_date)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to count new entities: {}", e)))?;

    // New topics: count topics from this day's events that don't appear in prior days
    let new_topics: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(DISTINCT jt)
           FROM wiki_events e, jsonb_array_elements_text(e.topics) jt
           WHERE e.day_id = 'day_' || $1
             AND jt != 'sleep'
             AND NOT EXISTS (
               SELECT 1 FROM wiki_events e2, jsonb_array_elements_text(e2.topics) jt2
               WHERE e2.day_id != e.day_id
                 AND e2.started_at < e.started_at
                 AND jt2 = jt
             )"#,
    )
    .bind(date_str)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to count new topics: {}", e)))?;

    Ok((new_entities, new_topics))
}

/// Update a day
/// List days in a date range
pub async fn list_days(
    pool: &PgPool,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Result<Vec<WikiDay>> {
    use sqlx::Row;

    let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
        r#"
        SELECT
            id, date, start_timezone,
            (SELECT dp.prose FROM wiki_day_prose dp WHERE dp.day_id = wiki_days.id) AS article,
            created_at, updated_at
        FROM wiki_days
        WHERE date >= $1 AND date <= $2
        ORDER BY date DESC
        "#,
    )
    .bind(start_date)
    .bind(end_date)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list days: {}", e)))?;

    rows.iter()
        .map(|row| {
            // `date` is a Postgres DATE — decode it as NaiveDate. (This used
            // to try_get::<String> inside a filter_map, which failed to decode
            // on every row and silently returned an empty list.)
            let date: NaiveDate = row
                .try_get("date")
                .map_err(|e| Error::Database(format!("Failed to decode day date: {}", e)))?;
            wiki_day_from_row(row, date)
        })
        .collect()
}

/// One day of the wiki activity calendar: how much recorded life the day
/// holds. Event count is the honest signal — it exists as soon as the day is
/// segmented, independent of whether the nightly narration has run yet.
#[derive(Debug, Serialize)]
pub struct DayActivity {
    pub date: NaiveDate,
    pub event_count: i64,
    pub narrated: bool,
}

/// Per-day activity for a date range, for the wiki's calendar heatmap.
/// Deliberately tiny — the full `WikiDay` list is heavyweight (narration
/// text, snapshots) and this gets called for a year at a time.
pub async fn day_activity(
    pool: &PgPool,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Result<Vec<DayActivity>> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"
        SELECT
            d.date,
            COUNT(e.id) FILTER (WHERE e.user_hidden = false) AS event_count,
            (d.narrated_at IS NOT NULL) AS narrated
        FROM wiki_days d
        LEFT JOIN wiki_events e ON e.day_id = d.id
        WHERE d.date >= $1 AND d.date <= $2
        GROUP BY d.id, d.date, d.narrated_at
        ORDER BY d.date
        "#,
    )
    .bind(start_date)
    .bind(end_date)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to load day activity: {}", e)))?;

    rows.iter()
        .map(|row| {
            Ok(DayActivity {
                date: row
                    .try_get("date")
                    .map_err(|e| Error::Database(format!("Failed to decode date: {}", e)))?,
                event_count: row
                    .try_get("event_count")
                    .map_err(|e| Error::Database(format!("Failed to decode event_count: {}", e)))?,
                narrated: row
                    .try_get("narrated")
                    .map_err(|e| Error::Database(format!("Failed to decode narrated: {}", e)))?,
            })
        })
        .collect()
}


/// A past year's entry for the same calendar date — the wiki front page's
/// "on this day" register.
#[derive(Debug, Serialize)]
pub struct OnThisDayEntry {
    pub date: NaiveDate,
    /// The day article's opening paragraph. Was `epigraph` — a column the
    /// narrate prompt forbids the model to produce, so it was NULL on every
    /// row of every box, and the overview rendered nothing for years.
    pub lede: Option<String>,
    pub narrated: bool,
    pub event_count: i64,
}

/// Days from earlier years sharing `date`'s month and day, newest first.
pub async fn on_this_day(pool: &PgPool, date: NaiveDate) -> Result<Vec<OnThisDayEntry>> {
    use sqlx::Row;

    let rows = sqlx::query(&format!(
        r#"
        SELECT
            d.date,
            {lede} AS lede,
            (d.narrated_at IS NOT NULL) AS narrated,
            COUNT(e.id) FILTER (WHERE e.user_hidden = false) AS event_count
        FROM wiki_days d
        LEFT JOIN wiki_day_prose dp ON dp.day_id = d.id
        LEFT JOIN wiki_events e ON e.day_id = d.id
        WHERE EXTRACT(MONTH FROM d.date) = $1
          AND EXTRACT(DAY FROM d.date) = $2
          AND d.date < $3
        GROUP BY d.id, d.date, dp.prose, d.narrated_at
        ORDER BY d.date DESC
        "#,
        lede = crate::api::wiki_editor::lede_sql("dp.prose")
    ))
    .bind(chrono::Datelike::month(&date) as i32)
    .bind(chrono::Datelike::day(&date) as i32)
    .bind(date)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to load on-this-day: {}", e)))?;

    rows.iter()
        .map(|row| {
            Ok(OnThisDayEntry {
                date: row
                    .try_get("date")
                    .map_err(|e| Error::Database(format!("Failed to decode date: {}", e)))?,
                lede: row
                    .try_get("lede")
                    .map_err(|e| Error::Database(format!("Failed to decode lede: {}", e)))?,
                narrated: row
                    .try_get("narrated")
                    .map_err(|e| Error::Database(format!("Failed to decode narrated: {}", e)))?,
                event_count: row
                    .try_get("event_count")
                    .map_err(|e| Error::Database(format!("Failed to decode event_count: {}", e)))?,
            })
        })
        .collect()
}


// ============================================================================
// Timeline Day - Location chunks for movement map
// ============================================================================

/// A location chunk for the timeline day view.
///
/// One chunk per `data_location_visit` row, joined to its canonical place
/// (via `wiki_refs` → `wiki_places`) when one exists. Visits with no
/// place link have `place_id`/`place_name` set to None and the frontend
/// renders them as "Unknown".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineChunk {
    #[serde(rename = "type")]
    pub chunk_type: String,
    pub start_time: String,
    pub end_time: String,
    pub place_name: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
    pub place_id: Option<String>,
    pub duration_minutes: Option<i32>,
    pub place_category: Option<String>,
}

/// A raw GPS point for the movement track polyline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelinePoint {
    pub latitude: f64,
    pub longitude: f64,
    pub timestamp: String,
}

/// Timeline day view response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineDayView {
    pub date: String,
    /// Visits (clustered) — used by DayLocationTimeline + map markers
    pub chunks: Vec<TimelineChunk>,
    /// Raw GPS points — used by the map polyline (the actual path you walked)
    pub points: Vec<TimelinePoint>,
}

/// Get location visits for a day, returned as timeline chunks with their
/// canonical place link (if any).
pub async fn get_timeline_day(pool: &PgPool, date: NaiveDate) -> Result<TimelineDayView> {
    let start_of_day = date.and_hms_opt(0, 0, 0).unwrap().and_utc();
    let end_of_day = date
        .succ_opt()
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();

    // JOIN visits → wiki_refs → wiki_places.
    // er.source_id is the visit's UUID; both sides are stored as TEXT UUIDs,
    // so the join is a straight text match.
    let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
        r#"
        SELECT
            v.started_at           AS started_at,
            v.ended_at         AS ended_at,
            v.duration_minutes       AS duration_minutes,
            v.latitude               AS visit_lat,
            v.longitude              AS visit_lon,
            er.entity_id             AS place_id,
            p.name                   AS place_name,
            p.latitude               AS place_lat,
            p.longitude              AS place_lon,
            p.category               AS place_category
        FROM data_location_visit v
        LEFT JOIN wiki_refs er
            ON er.source_table = 'data_location_visit'
           AND er.source_id    = v.id
           AND er.entity_type  = 'place'
        LEFT JOIN wiki_places p ON p.id = er.entity_id
        WHERE v.started_at >= $1 AND v.started_at < $2
        ORDER BY v.started_at ASC
        "#,
    )
    .bind(start_of_day)
    .bind(end_of_day)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get location visits: {}", e)))?;

    use sqlx::Row;
    let chunks: Vec<TimelineChunk> = rows
        .iter()
        .filter_map(|row| {
            // started_at / ended_at are TIMESTAMPTZ in Postgres; read
            // them as DateTime<Utc> and stringify with RFC-3339 for the JSON.
            let arrival_ts: DateTime<Utc> = row.try_get("started_at").ok()?;
            let arrival = arrival_ts.to_rfc3339();
            let departure: Option<String> = row
                // absent-ok: a visit with no `ended_at` has not ended yet.
                .try_get::<Option<DateTime<Utc>>, _>("ended_at")
                .ok()
                .flatten()
                .map(|d| d.to_rfc3339());
            let duration_minutes: Option<i32> = row.try_get("duration_minutes").ok();
            let visit_lat: f64 = row.try_get("visit_lat").ok()?;
            let visit_lon: f64 = row.try_get("visit_lon").ok()?;
            let place_id: Option<String> = row.try_get("place_id").ok();
            let place_name: Option<String> = row.try_get("place_name").ok();
            let place_lat: Option<f64> = row.try_get("place_lat").ok();
            let place_lon: Option<f64> = row.try_get("place_lon").ok();
            let place_category: Option<String> = row.try_get("place_category").ok();

            // Prefer canonical place coords over the visit centroid so all
            // visits to "Home" land on the same map pin regardless of GPS jitter.
            let lat = place_lat.unwrap_or(visit_lat);
            let lon = place_lon.unwrap_or(visit_lon);

            Some(TimelineChunk {
                chunk_type: "location".to_string(),
                start_time: arrival.clone(),
                end_time: departure.unwrap_or(arrival),
                place_name,
                latitude: lat,
                longitude: lon,
                place_id,
                duration_minutes,
                place_category,
            })
        })
        .collect();

    // Also fetch the raw GPS points so the map can render the actual path,
    // not just lines connecting visit centroids.
    let point_rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
        r#"
        SELECT latitude, longitude, occurred_at
        FROM data_location_point
        WHERE occurred_at >= $1 AND occurred_at < $2
        ORDER BY occurred_at ASC
        "#,
    )
    .bind(start_of_day)
    .bind(end_of_day)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get location points: {}", e)))?;

    let points: Vec<TimelinePoint> = point_rows
        .iter()
        // These read COLUMNS off rows already in hand — the query's own error
        // was handled above — and a point missing a coordinate or a timestamp
        // is one this timeline cannot place, so `filter_map` drops it rather
        // than inventing one.
        .filter_map(|row| {
            let lat: Option<f64> = row.try_get("latitude").ok();
            let lng: Option<f64> = row.try_get("longitude").ok();
            let ts: Option<String> = row
                // absent-ok: a row without a timestamp cannot be placed in time.
                .try_get::<Option<DateTime<Utc>>, _>("occurred_at")
                .ok()
                .flatten()
                .map(|t| t.to_rfc3339());
            match (lat, lng, ts) {
                (Some(lat), Some(lng), Some(ts)) => Some(TimelinePoint {
                    latitude: lat,
                    longitude: lng,
                    timestamp: ts,
                }),
                _ => None,
            }
        })
        .collect();

    Ok(TimelineDayView {
        date: date.to_string(),
        chunks,
        points,
    })
}

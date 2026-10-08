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
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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
        return wiki_day_from_row_with_counts(&row, date, ne, nt);
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
        created_at: row.try_get("created_at").unwrap_or_else(|_| Utc::now()),
        updated_at: row.try_get("updated_at").unwrap_or_else(|_| Utc::now()),
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
    /// The phone's own error radius, metres. Past ~100 m it is a cell tower's
    /// guess, not a GPS fix, and a reader drawing a path needs to know.
    pub horizontal_accuracy: Option<f64>,
    /// The phone's reported speed, m/s; null when it reported none.
    pub speed: Option<f64>,
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
///
/// The day is local midnight to local midnight in the day's own zone, the same
/// window the day page's events and streams use. A UTC day put a US evening's
/// visits and points on the next day's page.
pub async fn get_timeline_day(pool: &PgPool, date: NaiveDate) -> Result<TimelineDayView> {
    let (start_of_day, end_of_day) = crate::timezone::day_window(pool, date).await?;

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
        SELECT latitude, longitude, occurred_at, horizontal_accuracy, speed
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

    let points: Vec<TimelinePoint> = point_rows.iter().filter_map(timeline_point).collect();

    Ok(TimelineDayView {
        date: date.to_string(),
        chunks,
        points,
    })
}

/// A `data_location_point` row as a timeline point. This reads COLUMNS off a
/// row already in hand — the query's own error is handled by the caller — and
/// a point missing a coordinate or a timestamp is one this timeline cannot
/// place, so it is dropped rather than invented.
pub(crate) fn timeline_point(row: &sqlx::postgres::PgRow) -> Option<TimelinePoint> {
    use sqlx::Row;
    let lat: Option<f64> = row.try_get("latitude").ok();
    let lng: Option<f64> = row.try_get("longitude").ok();
    let ts: Option<String> = row
        // absent-ok: a row without a timestamp cannot be placed in time.
        .try_get::<Option<DateTime<Utc>>, _>("occurred_at")
        .ok()
        .flatten()
        .map(|t| t.to_rfc3339());
    // absent-ok: older rows and some sources carry no accuracy or speed.
    let horizontal_accuracy: Option<f64> = row.try_get("horizontal_accuracy").ok().flatten();
    let speed: Option<f64> = row.try_get("speed").ok().flatten();
    match (lat, lng, ts) {
        (Some(lat), Some(lng), Some(ts)) => Some(TimelinePoint {
            latitude: lat,
            longitude: lng,
            timestamp: ts,
            horizontal_accuracy,
            speed,
        }),
        _ => None,
    }
}

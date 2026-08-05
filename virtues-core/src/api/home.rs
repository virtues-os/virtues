//! Home-page loops that aren't "wiki": current weather (the environment
//! ontology), the next few calendar events, and the unnamed-place backlog the
//! box asks the owner to name. Each is a small read the home page composes.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

use crate::error::{Error, Result};

// ---------------------------------------------------------------------------
// Weather (Open-Meteo, written hourly by the weather_sync action)
// ---------------------------------------------------------------------------

/// Current conditions for the masthead: the freshest actual, plus today's
/// freshest forecast (high/low, sun). Null until the weather_sync cron runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherNow {
    pub temperature_c: Option<f64>,
    pub apparent_c: Option<f64>,
    pub humidity_pct: Option<f64>,
    pub wind_kph: Option<f64>,
    pub is_day: Option<bool>,
    pub weather_code: Option<i32>,
    pub condition: String,
    pub temp_max_c: Option<f64>,
    pub temp_min_c: Option<f64>,
    pub sunrise: Option<String>,
    pub sunset: Option<String>,
    pub valid_time: String,
}

fn wmo_condition(code: Option<i32>) -> String {
    match code.unwrap_or(-1) {
        0 => "Clear",
        1 | 2 => "Mostly clear",
        3 => "Overcast",
        45 | 48 => "Fog",
        51 | 53 | 55 | 56 | 57 => "Drizzle",
        61 | 63 | 65 | 66 | 67 | 80 | 81 | 82 => "Rain",
        71 | 73 | 75 | 77 | 85 | 86 => "Snow",
        95 | 96 | 99 => "Thunderstorm",
        _ => "\u{2014}",
    }
    .to_string()
}

pub async fn get_current_weather(pool: &PgPool) -> Result<Option<WeatherNow>> {
    let cur = sqlx::query(
        r#"SELECT temperature_c, apparent_c, humidity_pct, wind_kph, is_day, weather_code, valid_time
           FROM data_environment_weather WHERE is_forecast = FALSE
           ORDER BY valid_time DESC LIMIT 1"#,
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("weather current: {e}")))?;
    let cur = match cur {
        Some(r) => r,
        None => return Ok(None),
    };
    let fc = sqlx::query(
        r#"SELECT temp_max_c, temp_min_c, sunrise, sunset
           FROM data_environment_weather
           WHERE is_forecast = TRUE AND valid_time::date = (now() AT TIME ZONE 'UTC')::date
           ORDER BY issued_at DESC LIMIT 1"#,
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let code: Option<i32> = cur.try_get("weather_code").ok().flatten();
    let valid: DateTime<Utc> = cur.try_get("valid_time").ok().unwrap_or_else(Utc::now);
    Ok(Some(WeatherNow {
        temperature_c: cur.try_get("temperature_c").ok().flatten(),
        apparent_c: cur.try_get("apparent_c").ok().flatten(),
        humidity_pct: cur.try_get("humidity_pct").ok().flatten(),
        wind_kph: cur.try_get("wind_kph").ok().flatten(),
        is_day: cur.try_get("is_day").ok().flatten(),
        weather_code: code,
        condition: wmo_condition(code),
        temp_max_c: fc.as_ref().and_then(|r| r.try_get("temp_max_c").ok().flatten()),
        temp_min_c: fc.as_ref().and_then(|r| r.try_get("temp_min_c").ok().flatten()),
        sunrise: fc
            .as_ref()
            .and_then(|r| r.try_get::<Option<DateTime<Utc>>, _>("sunrise").ok().flatten())
            .map(|d| d.to_rfc3339()),
        sunset: fc
            .as_ref()
            .and_then(|r| r.try_get::<Option<DateTime<Utc>>, _>("sunset").ok().flatten())
            .map(|d| d.to_rfc3339()),
        valid_time: valid.to_rfc3339(),
    }))
}

// ---------------------------------------------------------------------------
// Upcoming calendar events
// ---------------------------------------------------------------------------

/// The next events on the calendar, with holidays/birthdays filtered out.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpcomingEvent {
    pub id: String,
    pub title: String,
    pub start_time: String,
    pub end_time: String,
    pub is_all_day: bool,
    pub location_name: Option<String>,
    pub is_sacred: bool,
}

pub async fn get_calendar_upcoming(pool: &PgPool, limit: i64) -> Result<Vec<UpcomingEvent>> {
    let rows = sqlx::query(
        r#"
        SELECT id, title, start_time, end_time, is_all_day, location_name,
               COALESCE(is_sacred, FALSE) AS is_sacred
        FROM data_calendar_event
        WHERE start_time > now()
          AND deleted_at_source IS NULL
          AND is_archived = FALSE
          AND (status IS NULL OR status <> 'cancelled')
          AND NOT (is_all_day = TRUE AND (calendar_name ILIKE '%holiday%' OR calendar_name ILIKE '%birthday%'))
        ORDER BY start_time ASC
        LIMIT $1
        "#,
    )
    .bind(limit.clamp(1, 20))
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("calendar upcoming: {e}")))?;
    Ok(rows
        .iter()
        .filter_map(|r| {
            let start: DateTime<Utc> = r.try_get("start_time").ok()?;
            let end: DateTime<Utc> = r.try_get("end_time").ok()?;
            Some(UpcomingEvent {
                id: r.try_get("id").ok()?,
                title: r.try_get("title").unwrap_or_default(),
                start_time: start.to_rfc3339(),
                end_time: end.to_rfc3339(),
                is_all_day: r.try_get("is_all_day").unwrap_or(false),
                location_name: r.try_get("location_name").ok().flatten(),
                is_sacred: r.try_get("is_sacred").unwrap_or(false),
            })
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Unnamed-place backlog ("you've stopped here 6 times — what is it?")
// ---------------------------------------------------------------------------

/// Places the box has visited but never named (stubbed "Location <lat>, <lon>").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnnamedPlace {
    pub id: String,
    pub name: String,
    pub visit_count: i64,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

/// The unnamed places worth asking about, busiest first.
///
/// `wiki_places.visit_count` is not the count to sort on: it is maintained for
/// named places and sits at 0 for every `Location %` row on both a dev box and
/// a nine-year one — which is exactly the set this returns, so ordering by it
/// ranked the backlog at random and reported every place as never visited.
///
/// The count is taken from the visit records instead, with each visit assigned
/// to the single nearest place. Counting every place within a radius would
/// credit one afternoon downtown to five stubs on the same block; five
/// neighbours each claiming ~330 of the same 462 visits is what that produced
/// here. The bounding box is a cheap prefilter, not the test — roughly two
/// kilometres, past which a visit belongs to no known place at all.
pub async fn get_unnamed_places(pool: &PgPool, limit: i64) -> Result<Vec<UnnamedPlace>> {
    let rows = sqlx::query(
        r#"
        WITH assigned AS (
            SELECT (
                SELECT p.id FROM wiki_places p
                WHERE p.latitude IS NOT NULL AND p.longitude IS NOT NULL
                  AND abs(p.latitude - v.latitude) < 0.02
                  AND abs(p.longitude - v.longitude) < 0.02
                ORDER BY (p.latitude - v.latitude) ^ 2 + (p.longitude - v.longitude) ^ 2
                LIMIT 1
            ) AS place_id
            FROM data_location_visit v
            WHERE v.is_archived = FALSE AND v.deleted_at_source IS NULL
        )
        SELECT c.id, c.name, c.latitude, c.longitude, count(a.place_id) AS visit_count
        FROM wiki_places c
        LEFT JOIN assigned a ON a.place_id = c.id
        WHERE c.name LIKE 'Location %'
        GROUP BY c.id, c.name, c.latitude, c.longitude
        HAVING count(a.place_id) > 0
        ORDER BY visit_count DESC
        LIMIT $1
        "#,
    )
    .bind(limit.clamp(1, 20))
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("unnamed places: {e}")))?;
    Ok(rows
        .iter()
        .filter_map(|r| {
            Some(UnnamedPlace {
                id: r.try_get("id").ok()?,
                name: r.try_get("name").ok()?,
                visit_count: r.try_get("visit_count").unwrap_or(0),
                latitude: r.try_get("latitude").ok().flatten(),
                longitude: r.try_get("longitude").ok().flatten(),
            })
        })
        .collect())
}

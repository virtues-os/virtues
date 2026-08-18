//! weather_sync — hourly Open-Meteo pull for the owner's location.
//!
//! Writes current conditions (actuals) + a short daily forecast into
//! `data_environment_weather`. No API key, no tracking; coordinates are rounded
//! to ~1 km before they ever leave the box. One row per actual hour and per
//! (forecast-day, issue-hour), deduped on `source_stream_id`.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use sqlx::{PgPool, Row};

use virtues_helpers::dedup::build_batch_insert_query;
use virtues_helpers::{connect_from_env, output, read_input};

const API: &str = "https://api.open-meteo.com/v1/forecast";

#[derive(Deserialize)]
struct OmResp {
    current: OmCurrent,
    daily: OmDaily,
}
#[derive(Deserialize)]
struct OmCurrent {
    time: String,
    temperature_2m: Option<f64>,
    relative_humidity_2m: Option<f64>,
    apparent_temperature: Option<f64>,
    is_day: Option<i32>,
    precipitation: Option<f64>,
    weather_code: Option<i32>,
    wind_speed_10m: Option<f64>,
}
#[derive(Deserialize)]
struct OmDaily {
    time: Vec<String>,
    weather_code: Vec<Option<i32>>,
    temperature_2m_max: Vec<Option<f64>>,
    temperature_2m_min: Vec<Option<f64>>,
    sunrise: Vec<String>,
    sunset: Vec<String>,
}

struct WeatherRow {
    id: String,
    latitude: f64,
    longitude: f64,
    occurred_at: DateTime<Utc>,
    issued_at: DateTime<Utc>,
    is_forecast: bool,
    temperature_c: Option<f64>,
    apparent_c: Option<f64>,
    humidity_pct: Option<f64>,
    precipitation_mm: Option<f64>,
    wind_kph: Option<f64>,
    is_day: Option<bool>,
    temp_max_c: Option<f64>,
    temp_min_c: Option<f64>,
    sunrise: Option<DateTime<Utc>>,
    sunset: Option<DateTime<Utc>>,
    weather_code: Option<i32>,
    source_stream_id: String,
}

fn uuid_v5(s: &str) -> String {
    uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, s.as_bytes()).to_string()
}
fn parse_dt(s: &str) -> Option<DateTime<Utc>> {
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .ok()
        .map(|nd| nd.and_utc())
}
fn parse_day_noon(s: &str) -> Option<DateTime<Utc>> {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(12, 0, 0))
        .map(|nd| nd.and_utc())
}

/// Where to fetch weather for: latest GPS fix, else the configured home place.
async fn resolve_location(pool: &PgPool) -> Option<(f64, f64)> {
    if let Ok(Some(r)) = sqlx::query(
        "SELECT latitude, longitude FROM data_location_point \
         WHERE latitude IS NOT NULL ORDER BY occurred_at DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    {
        if let (Some(la), Some(lo)) = (
            r.try_get::<Option<f64>, _>("latitude").ok().flatten(),
            r.try_get::<Option<f64>, _>("longitude").ok().flatten(),
        ) {
            return Some((la, lo));
        }
    }
    if let Ok(Some(r)) = sqlx::query(
        "SELECT wp.latitude, wp.longitude FROM app_user_profile p \
         LEFT JOIN wiki_places wp ON p.home_place_id = wp.id \
         WHERE p.id = '00000000-0000-0000-0000-000000000001'",
    )
    .fetch_optional(pool)
    .await
    {
        if let (Some(la), Some(lo)) = (
            r.try_get::<Option<f64>, _>("latitude").ok().flatten(),
            r.try_get::<Option<f64>, _>("longitude").ok().flatten(),
        ) {
            return Some((la, lo));
        }
    }
    None
}

async fn flush(pool: &PgPool, rows: &[WeatherRow]) -> Result<usize> {
    if rows.is_empty() {
        return Ok(0);
    }
    let cols = [
        "id", "latitude", "longitude", "occurred_at", "issued_at", "is_forecast",
        "temperature_c", "apparent_c", "humidity_pct", "precipitation_mm", "wind_kph",
        "is_day", "temp_max_c", "temp_min_c", "sunrise", "sunset", "weather_code",
        "source_stream_id",
    ];
    let sql = build_batch_insert_query("data_environment_weather", &cols, "source_stream_id", rows.len());
    let mut q = sqlx::query(&sql);
    for r in rows {
        q = q
            .bind(&r.id)
            .bind(r.latitude)
            .bind(r.longitude)
            .bind(r.occurred_at)
            .bind(r.issued_at)
            .bind(r.is_forecast)
            .bind(r.temperature_c)
            .bind(r.apparent_c)
            .bind(r.humidity_pct)
            .bind(r.precipitation_mm)
            .bind(r.wind_kph)
            .bind(r.is_day)
            .bind(r.temp_max_c)
            .bind(r.temp_min_c)
            .bind(r.sunrise)
            .bind(r.sunset)
            .bind(r.weather_code)
            .bind(&r.source_stream_id);
    }
    let res = q.execute(pool).await?;
    Ok(res.rows_affected() as usize)
}

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();
    let input = read_input()?;
    let pool = connect_from_env("virtues-action-weather_sync").await?;

    let (lat, lon) = match resolve_location(&pool).await {
        // Round to 2 decimals (~1 km) before anything leaves the box.
        Some((la, lo)) => ((la * 100.0).round() / 100.0, (lo * 100.0).round() / 100.0),
        None => return output("weather_sync: no known location yet", &input.config),
    };

    let url = format!(
        "{API}?latitude={lat}&longitude={lon}\
         &current=temperature_2m,relative_humidity_2m,apparent_temperature,is_day,precipitation,weather_code,wind_speed_10m\
         &daily=weather_code,temperature_2m_max,temperature_2m_min,sunrise,sunset\
         &timezone=GMT&forecast_days=2"
    );
    let resp: OmResp = virtues_applets::fetch_json(virtues_applets::http_client().get(&url)).await?;

    let issued = Utc::now();
    let issued_hour = issued.format("%Y-%m-%dT%H").to_string();
    let mut rows: Vec<WeatherRow> = Vec::new();

    // current actual
    let cur_valid = parse_dt(&resp.current.time).unwrap_or(issued);
    let cur_ssid = format!("open-meteo:cur:{lat:.2}:{lon:.2}:{}", resp.current.time);
    rows.push(WeatherRow {
        id: uuid_v5(&cur_ssid),
        latitude: lat,
        longitude: lon,
        occurred_at: cur_valid,
        issued_at: issued,
        is_forecast: false,
        temperature_c: resp.current.temperature_2m,
        apparent_c: resp.current.apparent_temperature,
        humidity_pct: resp.current.relative_humidity_2m,
        precipitation_mm: resp.current.precipitation,
        wind_kph: resp.current.wind_speed_10m,
        is_day: resp.current.is_day.map(|d| d == 1),
        temp_max_c: None,
        temp_min_c: None,
        sunrise: None,
        sunset: None,
        weather_code: resp.current.weather_code,
        source_stream_id: cur_ssid,
    });

    // daily forecast (today + tomorrow)
    for i in 0..resp.daily.time.len().min(2) {
        if let Some(valid) = parse_day_noon(&resp.daily.time[i]) {
            let ssid = format!("open-meteo:day:{lat:.2}:{lon:.2}:{}:{issued_hour}", resp.daily.time[i]);
            rows.push(WeatherRow {
                id: uuid_v5(&ssid),
                latitude: lat,
                longitude: lon,
                occurred_at: valid,
                issued_at: issued,
                is_forecast: true,
                temperature_c: None,
                apparent_c: None,
                humidity_pct: None,
                precipitation_mm: None,
                wind_kph: None,
                is_day: None,
                temp_max_c: resp.daily.temperature_2m_max.get(i).copied().flatten(),
                temp_min_c: resp.daily.temperature_2m_min.get(i).copied().flatten(),
                sunrise: resp.daily.sunrise.get(i).and_then(|s| parse_dt(s)),
                sunset: resp.daily.sunset.get(i).and_then(|s| parse_dt(s)),
                weather_code: resp.daily.weather_code.get(i).copied().flatten(),
                source_stream_id: ssid,
            });
        }
    }

    let written = flush(&pool, &rows).await?;
    output(&format!("weather_sync: {written} rows at {lat:.2},{lon:.2}"), &input.config)
}

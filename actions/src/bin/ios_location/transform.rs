//! iOS Location → `data_location_point` transform.
//!
//! Ported from `core/src/sources/ios/location/transform.rs`. Place clustering
//! (`location_visit`, `entities_place`) is no longer chained from here — it runs
//! as its own `entity_resolution` action on a 30-minute cron.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::SqlitePool;
use uuid::Uuid;
use virtues_action_helpers::dedup::{build_batch_insert_query, BATCH_SIZE};
use virtues_action_helpers::ios::{parse_timestamp, stream_id_or_new, IOS_PROVIDER, LOCATION_STREAM_TABLE};

#[allow(clippy::type_complexity)]
type LocationRow = (
    String,        // id
    f64,           // latitude
    f64,           // longitude
    Option<f64>,   // altitude
    Option<f64>,   // horizontal_accuracy
    Option<f64>,   // vertical_accuracy
    DateTime<Utc>, // timestamp
    String,        // source_stream_id
    Value,         // metadata
);

pub async fn write_locations(db: &SqlitePool, records: &[Value]) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    let mut pending: Vec<LocationRow> = Vec::new();
    let mut written = 0;

    for record in records {
        let Some(latitude) = record.get("latitude").and_then(|v| v.as_f64()) else {
            continue;
        };
        let Some(longitude) = record.get("longitude").and_then(|v| v.as_f64()) else {
            continue;
        };

        let timestamp = parse_timestamp(record, "timestamp");
        let stream_id = stream_id_or_new(record);

        let altitude = record.get("altitude").and_then(|v| v.as_f64());
        // GPS returns -1.0 when speed is invalid/unavailable — normalize to NULL
        let speed = record
            .get("speed")
            .and_then(|v| v.as_f64())
            .filter(|&s| s >= 0.0);
        let course = record.get("course").and_then(|v| v.as_f64());
        let horizontal_accuracy = record.get("horizontal_accuracy").and_then(|v| v.as_f64());
        let vertical_accuracy = record.get("vertical_accuracy").and_then(|v| v.as_f64());
        let activity_type = record
            .get("activity_type")
            .and_then(|v| v.as_str())
            .map(String::from);
        let activity_confidence = record
            .get("activity_confidence")
            .and_then(|v| v.as_str())
            .map(String::from);
        let floor_level = record
            .get("floor_level")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32);
        let raw_data = record.get("raw_data").cloned();

        let metadata = serde_json::json!({
            "speed": speed,
            "course": course,
            "activity_type": activity_type,
            "activity_confidence": activity_confidence,
            "floor_level": floor_level,
            "ios_raw": raw_data,
        });

        pending.push((
            Uuid::new_v4().to_string(),
            latitude,
            longitude,
            altitude,
            horizontal_accuracy,
            vertical_accuracy,
            timestamp,
            stream_id,
            metadata,
        ));

        if pending.len() >= BATCH_SIZE {
            written += flush_locations(db, &pending).await?;
            pending.clear();
        }
    }

    if !pending.is_empty() {
        written += flush_locations(db, &pending).await?;
    }

    Ok(written)
}

async fn flush_locations(db: &SqlitePool, records: &[LocationRow]) -> Result<usize> {
    let sql = build_batch_insert_query(
        "data_location_point",
        &[
            "id",
            "latitude",
            "longitude",
            "altitude",
            "horizontal_accuracy",
            "vertical_accuracy",
            "timestamp",
            "source_stream_id",
            "source_table",
            "source_provider",
            "metadata",
        ],
        "source_stream_id",
        records.len(),
    );

    let mut q = sqlx::query(&sql);
    for (id, lat, lon, alt, h_acc, v_acc, timestamp, stream_id, metadata) in records {
        q = q
            .bind(id)
            .bind(lat)
            .bind(lon)
            .bind(alt)
            .bind(h_acc)
            .bind(v_acc)
            .bind(timestamp)
            .bind(stream_id)
            .bind(LOCATION_STREAM_TABLE)
            .bind(IOS_PROVIDER)
            .bind(metadata);
    }

    let result = q.execute(db).await?;
    Ok(result.rows_affected() as usize)
}

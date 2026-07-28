//! Strava activities → `data_health_workout` transform.
//!
//! Adapted from the deleted `core/src/sources/strava/activities/transform.rs`.
//! The mapping logic is preserved (sport_type→workout_type, kJ→kcal, m→km).
//! The old code read from an intermediate `stream_strava_activities` table
//! via the deprecated pipeline; this version takes API JSON directly.

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;
use virtues_helpers::dedup::{build_batch_insert_query, BATCH_SIZE};

#[allow(clippy::type_complexity)]
type WorkoutRow = (
    String,         // id (deterministic UUIDv5)
    String,         // workout_type
    Option<i32>,    // duration_minutes
    Option<i32>,    // calories_burned
    Option<i32>,    // avg_heart_rate
    Option<i32>,    // max_heart_rate
    Option<f64>,    // distance_km
    DateTime<Utc>,  // start_time
    DateTime<Utc>,  // end_time
    String,         // source_stream_id (Strava activity_id)
    Value,          // metadata
);

/// Write a batch of Strava activity records to `data_health_workout`.
/// Each record is the raw JSON object Strava returns from
/// `GET /api/v3/athlete/activities`. Returns the number of newly-inserted rows
/// (existing rows skip via `ON CONFLICT(source_stream_id) DO NOTHING`).
pub async fn write_activities(db: &PgPool, activities: &[Value]) -> Result<usize> {
    let mut pending: Vec<WorkoutRow> = Vec::new();
    let mut written = 0;

    for activity in activities {
        let activity_id = activity.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
        if activity_id == 0 {
            continue;
        }

        let stream_id = activity_id.to_string();

        let workout_type = activity
            .get("sport_type")
            .or_else(|| activity.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();

        let start_time = activity
            .get("start_date")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<DateTime<Utc>>().ok())
            .unwrap_or_else(Utc::now);

        let elapsed_time = activity
            .get("elapsed_time")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let duration_minutes = if elapsed_time > 0 {
            Some((elapsed_time as f64 / 60.0).round() as i32)
        } else {
            None
        };
        let end_time = start_time + Duration::seconds(elapsed_time);

        // kJ → kcal (energy * 0.239)
        let calories_burned = activity
            .get("kilojoules")
            .and_then(|v| v.as_f64())
            .map(|kj| (kj * 0.239).round() as i32);

        let distance_km = activity
            .get("distance")
            .and_then(|v| v.as_f64())
            .map(|m| m / 1000.0);

        let avg_heart_rate = activity
            .get("average_heartrate")
            .and_then(|v| v.as_f64())
            .map(|hr| hr.round() as i32);

        let max_heart_rate = activity
            .get("max_heartrate")
            .and_then(|v| v.as_f64())
            .map(|hr| hr.round() as i32);

        let metadata = serde_json::json!({
            "strava_activity_id": activity_id,
            "name": activity.get("name"),
            "activity_type": activity.get("type"),
            "total_elevation_gain": activity.get("total_elevation_gain"),
            "average_speed": activity.get("average_speed"),
            "max_speed": activity.get("max_speed"),
            "suffer_score": activity.get("suffer_score"),
            "gear_id": activity.get("gear_id"),
            "summary_polyline": activity.get("map").and_then(|m| m.get("summary_polyline")),
        });

        let id = Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("strava:workout:{activity_id}").as_bytes(),
        )
        .to_string();

        pending.push((
            id,
            workout_type,
            duration_minutes,
            calories_burned,
            avg_heart_rate,
            max_heart_rate,
            distance_km,
            start_time,
            end_time,
            stream_id,
            metadata,
        ));

        if pending.len() >= BATCH_SIZE {
            written += flush(db, &pending).await?;
            pending.clear();
        }
    }

    if !pending.is_empty() {
        written += flush(db, &pending).await?;
    }

    Ok(written)
}

async fn flush(db: &PgPool, records: &[WorkoutRow]) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }
    let sql = build_batch_insert_query(
        "data_health_workout",
        &[
            "id",
            "workout_type",
            "duration_minutes",
            "calories_burned",
            "avg_heart_rate",
            "max_heart_rate",
            "distance_km",
            "start_time",
            "end_time",
            "source_stream_id",
            "source_table",
            "source_provider",
            "metadata",
        ],
        "source_stream_id",
        records.len(),
    );

    let mut q = sqlx::query(&sql);
    for r in records {
        q = q
            .bind(&r.0)
            .bind(&r.1)
            .bind(r.2)
            .bind(r.3)
            .bind(r.4)
            .bind(r.5)
            .bind(r.6)
            .bind(r.7)
            .bind(r.8)
            .bind(&r.9)
            .bind("strava_activities") // source_table
            .bind("strava")            // source_provider
            .bind(&r.10);
    }
    let result = q.execute(db).await?;
    Ok(result.rows_affected() as usize)
}

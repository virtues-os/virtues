//! HealthKit → ontology table transforms.
//!
//! Each `write_*` function takes a slice of raw HealthKit records and writes
//! them to the corresponding ontology table via batched `INSERT ON CONFLICT DO NOTHING`.
//! Dedup is enforced by the `source_stream_id UNIQUE` constraint.
//!
//! Ported from `core/src/sources/ios/healthkit/transform.rs` — logic is preserved,
//! the trait/registry plumbing is stripped.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::SqlitePool;
use uuid::Uuid;
use virtues_helpers::dedup::{build_batch_insert_query, BATCH_SIZE};
use virtues_helpers::ios::{parse_timestamp, stream_id_or_new, HEALTHKIT_STREAM_TABLE, IOS_PROVIDER};

// ─────────────────────────────────────────────────────────────────────────────
// Heart Rate
// ─────────────────────────────────────────────────────────────────────────────

pub async fn write_heart_rate(db: &SqlitePool, records: &[Value]) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    let mut pending: Vec<(String, i32, DateTime<Utc>, String, Value)> = Vec::new();
    let mut written = 0;

    for record in records {
        let Some(bpm) = record
            .get("value")
            .and_then(|v| v.as_f64())
            .or_else(|| record.get("heart_rate").and_then(|v| v.as_f64()))
        else {
            continue;
        };

        let timestamp = parse_timestamp(record, "timestamp");
        let stream_id = stream_id_or_new(record);
        let raw_data = record.get("raw_data").cloned();

        let measurement_context = raw_data
            .as_ref()
            .and_then(|d| d.get("context"))
            .and_then(|c| c.as_str())
            .map(String::from);

        let metadata = serde_json::json!({
            "healthkit_raw": raw_data,
            "measurement_context": measurement_context,
        });

        pending.push((
            Uuid::new_v4().to_string(),
            bpm as i32,
            timestamp,
            stream_id,
            metadata,
        ));

        if pending.len() >= BATCH_SIZE {
            written += flush_heart_rate(db, &pending).await?;
            pending.clear();
        }
    }

    if !pending.is_empty() {
        written += flush_heart_rate(db, &pending).await?;
    }

    Ok(written)
}

async fn flush_heart_rate(
    db: &SqlitePool,
    records: &[(String, i32, DateTime<Utc>, String, Value)],
) -> Result<usize> {
    let sql = build_batch_insert_query(
        "data_health_heart_rate",
        &[
            "id",
            "bpm",
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
    for (id, bpm, timestamp, stream_id, metadata) in records {
        q = q
            .bind(id)
            .bind(bpm)
            .bind(timestamp)
            .bind(stream_id)
            .bind(HEALTHKIT_STREAM_TABLE)
            .bind(IOS_PROVIDER)
            .bind(metadata);
    }
    let result = q.execute(db).await?;
    Ok(result.rows_affected() as usize)
}

// ─────────────────────────────────────────────────────────────────────────────
// HRV
// ─────────────────────────────────────────────────────────────────────────────

pub async fn write_hrv(db: &SqlitePool, records: &[Value]) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    let mut pending: Vec<(String, f64, DateTime<Utc>, String, Value)> = Vec::new();
    let mut written = 0;

    for record in records {
        let Some(hrv_ms) = record
            .get("value")
            .and_then(|v| v.as_f64())
            .or_else(|| record.get("hrv").and_then(|v| v.as_f64()))
        else {
            continue;
        };

        let timestamp = parse_timestamp(record, "timestamp");
        let stream_id = stream_id_or_new(record);
        let raw_data = record.get("raw_data").cloned();

        let measurement_type = raw_data
            .as_ref()
            .and_then(|d| d.get("hrv_type"))
            .and_then(|t| t.as_str())
            .unwrap_or("rmssd");

        let metadata = serde_json::json!({
            "healthkit_raw": raw_data,
            "measurement_type": measurement_type,
        });

        pending.push((
            Uuid::new_v4().to_string(),
            hrv_ms,
            timestamp,
            stream_id,
            metadata,
        ));

        if pending.len() >= BATCH_SIZE {
            written += flush_hrv(db, &pending).await?;
            pending.clear();
        }
    }

    if !pending.is_empty() {
        written += flush_hrv(db, &pending).await?;
    }

    Ok(written)
}

async fn flush_hrv(
    db: &SqlitePool,
    records: &[(String, f64, DateTime<Utc>, String, Value)],
) -> Result<usize> {
    let sql = build_batch_insert_query(
        "data_health_hrv",
        &[
            "id",
            "hrv_ms",
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
    for (id, hrv_ms, timestamp, stream_id, metadata) in records {
        q = q
            .bind(id)
            .bind(hrv_ms)
            .bind(timestamp)
            .bind(stream_id)
            .bind(HEALTHKIT_STREAM_TABLE)
            .bind(IOS_PROVIDER)
            .bind(metadata);
    }
    let result = q.execute(db).await?;
    Ok(result.rows_affected() as usize)
}

// ─────────────────────────────────────────────────────────────────────────────
// Steps
// ─────────────────────────────────────────────────────────────────────────────

pub async fn write_steps(db: &SqlitePool, records: &[Value]) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    let mut pending: Vec<(String, i32, DateTime<Utc>, String, Value)> = Vec::new();
    let mut written = 0;

    for record in records {
        let Some(step_count) = record
            .get("value")
            .and_then(|v| v.as_i64())
            .or_else(|| record.get("steps").and_then(|v| v.as_i64()))
        else {
            continue;
        };

        let timestamp = parse_timestamp(record, "timestamp");
        let stream_id = stream_id_or_new(record);
        let raw_data = record.get("raw_data").cloned();

        let metadata = serde_json::json!({ "healthkit_raw": raw_data });

        pending.push((
            Uuid::new_v4().to_string(),
            step_count as i32,
            timestamp,
            stream_id,
            metadata,
        ));

        if pending.len() >= BATCH_SIZE {
            written += flush_steps(db, &pending).await?;
            pending.clear();
        }
    }

    if !pending.is_empty() {
        written += flush_steps(db, &pending).await?;
    }

    Ok(written)
}

async fn flush_steps(
    db: &SqlitePool,
    records: &[(String, i32, DateTime<Utc>, String, Value)],
) -> Result<usize> {
    let sql = build_batch_insert_query(
        "data_health_steps",
        &[
            "id",
            "step_count",
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
    for (id, step_count, timestamp, stream_id, metadata) in records {
        q = q
            .bind(id)
            .bind(step_count)
            .bind(timestamp)
            .bind(stream_id)
            .bind(HEALTHKIT_STREAM_TABLE)
            .bind(IOS_PROVIDER)
            .bind(metadata);
    }
    let result = q.execute(db).await?;
    Ok(result.rows_affected() as usize)
}

// ─────────────────────────────────────────────────────────────────────────────
// Active Energy (kcal burned, derived by device)
// ─────────────────────────────────────────────────────────────────────────────

pub async fn write_active_energy(db: &SqlitePool, records: &[Value]) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    let mut pending: Vec<(String, f64, DateTime<Utc>, String, Value)> = Vec::new();
    let mut written = 0;

    for record in records {
        let Some(kcal) = record
            .get("value")
            .and_then(|v| v.as_f64())
            .or_else(|| record.get("active_energy").and_then(|v| v.as_f64()))
        else {
            continue;
        };

        let timestamp = parse_timestamp(record, "timestamp");
        let stream_id = stream_id_or_new(record);
        let raw_data = record.get("raw_data").cloned();
        let metadata = serde_json::json!({ "healthkit_raw": raw_data });

        pending.push((
            Uuid::new_v4().to_string(),
            kcal,
            timestamp,
            stream_id,
            metadata,
        ));

        if pending.len() >= BATCH_SIZE {
            written += flush_active_energy(db, &pending).await?;
            pending.clear();
        }
    }

    if !pending.is_empty() {
        written += flush_active_energy(db, &pending).await?;
    }

    Ok(written)
}

async fn flush_active_energy(
    db: &SqlitePool,
    records: &[(String, f64, DateTime<Utc>, String, Value)],
) -> Result<usize> {
    let sql = build_batch_insert_query(
        "data_health_active_energy",
        &[
            "id",
            "kcal",
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
    for (id, kcal, timestamp, stream_id, metadata) in records {
        q = q
            .bind(id)
            .bind(kcal)
            .bind(timestamp)
            .bind(stream_id)
            .bind(HEALTHKIT_STREAM_TABLE)
            .bind(IOS_PROVIDER)
            .bind(metadata);
    }
    let result = q.execute(db).await?;
    Ok(result.rows_affected() as usize)
}

// ─────────────────────────────────────────────────────────────────────────────
// Distance (meters travelled, derived by device)
// ─────────────────────────────────────────────────────────────────────────────

pub async fn write_distance(db: &SqlitePool, records: &[Value]) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    let mut pending: Vec<(String, f64, DateTime<Utc>, String, Value)> = Vec::new();
    let mut written = 0;

    for record in records {
        let Some(meters) = record
            .get("value")
            .and_then(|v| v.as_f64())
            .or_else(|| record.get("distance").and_then(|v| v.as_f64()))
        else {
            continue;
        };

        let timestamp = parse_timestamp(record, "timestamp");
        let stream_id = stream_id_or_new(record);
        let raw_data = record.get("raw_data").cloned();
        let metadata = serde_json::json!({ "healthkit_raw": raw_data });

        pending.push((
            Uuid::new_v4().to_string(),
            meters,
            timestamp,
            stream_id,
            metadata,
        ));

        if pending.len() >= BATCH_SIZE {
            written += flush_distance(db, &pending).await?;
            pending.clear();
        }
    }

    if !pending.is_empty() {
        written += flush_distance(db, &pending).await?;
    }

    Ok(written)
}

async fn flush_distance(
    db: &SqlitePool,
    records: &[(String, f64, DateTime<Utc>, String, Value)],
) -> Result<usize> {
    let sql = build_batch_insert_query(
        "data_health_distance",
        &[
            "id",
            "meters",
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
    for (id, meters, timestamp, stream_id, metadata) in records {
        q = q
            .bind(id)
            .bind(meters)
            .bind(timestamp)
            .bind(stream_id)
            .bind(HEALTHKIT_STREAM_TABLE)
            .bind(IOS_PROVIDER)
            .bind(metadata);
    }
    let result = q.execute(db).await?;
    Ok(result.rows_affected() as usize)
}

// ─────────────────────────────────────────────────────────────────────────────
// Sleep
// ─────────────────────────────────────────────────────────────────────────────

#[allow(clippy::type_complexity)]
type SleepRow = (
    String,              // id
    Option<Value>,       // sleep_stages
    i32,                 // duration_minutes
    Option<f64>,         // sleep_quality_score
    DateTime<Utc>,       // start_time
    DateTime<Utc>,       // end_time
    String,              // source_stream_id
    Value,               // metadata
);

pub async fn write_sleep(db: &SqlitePool, records: &[Value]) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    let mut pending: Vec<SleepRow> = Vec::new();
    let mut written = 0;

    for record in records {
        // iOS sends duration in metadata.duration_minutes for sleep
        let Some(sleep_duration) = record
            .get("metadata")
            .and_then(|m| m.get("duration_minutes"))
            .and_then(|v| v.as_i64())
            .or_else(|| record.get("sleep_duration").and_then(|v| v.as_i64()))
        else {
            continue;
        };

        let timestamp = parse_timestamp(record, "timestamp");
        let stream_id = stream_id_or_new(record);
        let raw_data = record.get("raw_data").cloned();

        let sleep_stage = record
            .get("sleep_stage")
            .and_then(|v| v.as_str())
            .map(String::from);

        // Build sleep_stages JSON from raw_data if available
        let sleep_stages = raw_data
            .as_ref()
            .and_then(|d| d.get("stages"))
            .cloned()
            .or_else(|| {
                sleep_stage.as_ref().map(|stage| {
                    serde_json::json!([{
                        "stage": stage,
                        "duration_minutes": sleep_duration
                    }])
                })
            });

        let end_time = timestamp + chrono::Duration::minutes(sleep_duration);
        let metadata = serde_json::json!({ "healthkit_raw": raw_data });

        pending.push((
            Uuid::new_v4().to_string(),
            sleep_stages,
            sleep_duration as i32,
            None,
            timestamp,
            end_time,
            stream_id,
            metadata,
        ));

        if pending.len() >= BATCH_SIZE {
            written += flush_sleep(db, &pending).await?;
            pending.clear();
        }
    }

    if !pending.is_empty() {
        written += flush_sleep(db, &pending).await?;
    }

    Ok(written)
}

async fn flush_sleep(db: &SqlitePool, records: &[SleepRow]) -> Result<usize> {
    let sql = build_batch_insert_query(
        "data_health_sleep",
        &[
            "id",
            "sleep_stages",
            "duration_minutes",
            "sleep_quality_score",
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
    for (id, sleep_stages, duration, quality, start, end, stream_id, metadata) in records {
        q = q
            .bind(id)
            .bind(sleep_stages)
            .bind(duration)
            .bind(quality)
            .bind(start)
            .bind(end)
            .bind(stream_id)
            .bind(HEALTHKIT_STREAM_TABLE)
            .bind(IOS_PROVIDER)
            .bind(metadata);
    }
    let result = q.execute(db).await?;
    Ok(result.rows_affected() as usize)
}

// ─────────────────────────────────────────────────────────────────────────────
// Workout
// ─────────────────────────────────────────────────────────────────────────────

#[allow(clippy::type_complexity)]
type WorkoutRow = (
    String,              // id
    String,              // workout_type
    Option<i32>,         // duration_minutes
    Option<i32>,         // calories_burned
    Option<i32>,         // avg_heart_rate
    Option<i32>,         // max_heart_rate
    Option<f64>,         // distance_km
    DateTime<Utc>,       // start_time
    DateTime<Utc>,       // end_time
    String,              // source_stream_id
    Value,               // metadata
);

pub async fn write_workout(db: &SqlitePool, records: &[Value]) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    let mut pending: Vec<WorkoutRow> = Vec::new();
    let mut written = 0;

    for record in records {
        let Some(workout_type) = record
            .get("workout_type")
            .and_then(|v| v.as_str())
            .or_else(|| record.get("value").and_then(|v| v.as_str()))
        else {
            continue;
        };

        let timestamp = parse_timestamp(record, "timestamp");
        let stream_id = stream_id_or_new(record);

        let workout_duration = record
            .get("workout_duration")
            .and_then(|v| v.as_i64())
            .map(|d| d as i32);

        let active_energy = record.get("active_energy").and_then(|v| v.as_f64());
        let distance = record.get("distance").and_then(|v| v.as_f64());
        let heart_rate = record.get("heart_rate").and_then(|v| v.as_f64());
        let raw_data = record.get("raw_data").cloned();

        let duration_minutes = workout_duration.unwrap_or(0);
        let end_time = timestamp + chrono::Duration::minutes(duration_minutes as i64);

        let max_heart_rate = raw_data
            .as_ref()
            .and_then(|d| d.get("max_heart_rate"))
            .and_then(|h| h.as_f64())
            .map(|h| h as i32);

        let intensity = raw_data
            .as_ref()
            .and_then(|d| d.get("intensity"))
            .and_then(|i| i.as_str())
            .map(String::from);

        let metadata = serde_json::json!({
            "healthkit_raw": raw_data,
            "intensity": intensity,
        });

        pending.push((
            Uuid::new_v4().to_string(),
            workout_type.to_string(),
            workout_duration,
            active_energy.map(|e| e as i32),
            heart_rate.map(|h| h as i32),
            max_heart_rate,
            distance.map(|d| d / 1000.0), // m → km
            timestamp,
            end_time,
            stream_id,
            metadata,
        ));

        if pending.len() >= BATCH_SIZE {
            written += flush_workout(db, &pending).await?;
            pending.clear();
        }
    }

    if !pending.is_empty() {
        written += flush_workout(db, &pending).await?;
    }

    Ok(written)
}

async fn flush_workout(db: &SqlitePool, records: &[WorkoutRow]) -> Result<usize> {
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
    for (
        id,
        workout_type,
        duration,
        calories,
        avg_hr,
        max_hr,
        distance_km,
        start,
        end,
        stream_id,
        metadata,
    ) in records
    {
        q = q
            .bind(id)
            .bind(workout_type)
            .bind(duration)
            .bind(calories)
            .bind(avg_hr)
            .bind(max_hr)
            .bind(distance_km)
            .bind(start)
            .bind(end)
            .bind(stream_id)
            .bind(HEALTHKIT_STREAM_TABLE)
            .bind(IOS_PROVIDER)
            .bind(metadata);
    }
    let result = q.execute(db).await?;
    Ok(result.rows_affected() as usize)
}

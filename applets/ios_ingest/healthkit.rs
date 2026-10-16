//! HealthKit → ontology table transforms.
//!
//! Each `write_*` function takes a slice of raw HealthKit records and writes
//! them to the corresponding ontology table via batched `INSERT ON CONFLICT DO NOTHING`.
//! Dedup is enforced by the `source_stream_id UNIQUE` constraint.
//!
//! Ported from `core/src/sources/ios/healthkit/transform.rs` — logic is preserved,
//! the trait/registry plumbing is stripped.

use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;
use sqlx::PgPool;
use virtues_helpers::dedup::{build_batch_insert_query, BATCH_SIZE};
use virtues_helpers::ios::{
    parse_timestamp, row_id, stream_id_or_hash, HEALTHKIT_STREAM_TABLE, IOS_PROVIDER,
};

// ─────────────────────────────────────────────────────────────────────────────
// Heart Rate
// ─────────────────────────────────────────────────────────────────────────────

pub async fn write_heart_rate(db: &PgPool, records: &[Value]) -> Result<usize> {
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
        let stream_id = stream_id_or_hash(record, HEALTHKIT_STREAM_TABLE);
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
            row_id(HEALTHKIT_STREAM_TABLE, &stream_id),
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
    db: &PgPool,
    records: &[(String, i32, DateTime<Utc>, String, Value)],
) -> Result<usize> {
    let sql = build_batch_insert_query(
        "data_health_heart_rate",
        &[
            "id",
            "bpm",
            "occurred_at",
            "source_stream_id",
            "source_table",
            "source_provider",
            "metadata",
        ],
        "source_stream_id",
        records.len(),
    );

    let mut q = sqlx::query(&sql);
    for (id, bpm, occurred_at, stream_id, metadata) in records {
        q = q
            .bind(id)
            .bind(bpm)
            .bind(occurred_at)
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

pub async fn write_hrv(db: &PgPool, records: &[Value]) -> Result<usize> {
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
        let stream_id = stream_id_or_hash(record, HEALTHKIT_STREAM_TABLE);
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
            row_id(HEALTHKIT_STREAM_TABLE, &stream_id),
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
    db: &PgPool,
    records: &[(String, f64, DateTime<Utc>, String, Value)],
) -> Result<usize> {
    let sql = build_batch_insert_query(
        "data_health_hrv",
        &[
            "id",
            "hrv_ms",
            "occurred_at",
            "source_stream_id",
            "source_table",
            "source_provider",
            "metadata",
        ],
        "source_stream_id",
        records.len(),
    );

    let mut q = sqlx::query(&sql);
    for (id, hrv_ms, occurred_at, stream_id, metadata) in records {
        q = q
            .bind(id)
            .bind(hrv_ms)
            .bind(occurred_at)
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

pub async fn write_steps(db: &PgPool, records: &[Value]) -> Result<usize> {
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
        let stream_id = stream_id_or_hash(record, HEALTHKIT_STREAM_TABLE);
        let raw_data = record.get("raw_data").cloned();

        let metadata = serde_json::json!({ "healthkit_raw": raw_data });

        pending.push((
            row_id(HEALTHKIT_STREAM_TABLE, &stream_id),
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
    db: &PgPool,
    records: &[(String, i32, DateTime<Utc>, String, Value)],
) -> Result<usize> {
    let sql = build_batch_insert_query(
        "data_health_steps",
        &[
            "id",
            "step_count",
            "occurred_at",
            "source_stream_id",
            "source_table",
            "source_provider",
            "metadata",
        ],
        "source_stream_id",
        records.len(),
    );

    let mut q = sqlx::query(&sql);
    for (id, step_count, occurred_at, stream_id, metadata) in records {
        q = q
            .bind(id)
            .bind(step_count)
            .bind(occurred_at)
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

pub async fn write_active_energy(db: &PgPool, records: &[Value]) -> Result<usize> {
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
        let stream_id = stream_id_or_hash(record, HEALTHKIT_STREAM_TABLE);
        let raw_data = record.get("raw_data").cloned();
        let metadata = serde_json::json!({ "healthkit_raw": raw_data });

        pending.push((
            row_id(HEALTHKIT_STREAM_TABLE, &stream_id),
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
    db: &PgPool,
    records: &[(String, f64, DateTime<Utc>, String, Value)],
) -> Result<usize> {
    let sql = build_batch_insert_query(
        "data_health_active_energy",
        &[
            "id",
            "kcal",
            "occurred_at",
            "source_stream_id",
            "source_table",
            "source_provider",
            "metadata",
        ],
        "source_stream_id",
        records.len(),
    );

    let mut q = sqlx::query(&sql);
    for (id, kcal, occurred_at, stream_id, metadata) in records {
        q = q
            .bind(id)
            .bind(kcal)
            .bind(occurred_at)
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

pub async fn write_distance(db: &PgPool, records: &[Value]) -> Result<usize> {
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
        let stream_id = stream_id_or_hash(record, HEALTHKIT_STREAM_TABLE);
        let raw_data = record.get("raw_data").cloned();
        let metadata = serde_json::json!({ "healthkit_raw": raw_data });

        pending.push((
            row_id(HEALTHKIT_STREAM_TABLE, &stream_id),
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
    db: &PgPool,
    records: &[(String, f64, DateTime<Utc>, String, Value)],
) -> Result<usize> {
    let sql = build_batch_insert_query(
        "data_health_distance",
        &[
            "id",
            "meters",
            "occurred_at",
            "source_stream_id",
            "source_table",
            "source_provider",
            "metadata",
        ],
        "source_stream_id",
        records.len(),
    );

    let mut q = sqlx::query(&sql);
    for (id, meters, occurred_at, stream_id, metadata) in records {
        q = q
            .bind(id)
            .bind(meters)
            .bind(occurred_at)
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
    String,        // id
    Option<Value>, // sleep_stages
    i32,           // duration_minutes
    Option<f64>,   // sleep_quality_score
    DateTime<Utc>, // start_time
    DateTime<Utc>, // end_time
    String,        // source_stream_id
    Value,         // metadata
);

pub async fn write_sleep(db: &PgPool, records: &[Value]) -> Result<usize> {
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
        let stream_id = stream_id_or_hash(record, HEALTHKIT_STREAM_TABLE);
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
            row_id(HEALTHKIT_STREAM_TABLE, &stream_id),
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

async fn flush_sleep(db: &PgPool, records: &[SleepRow]) -> Result<usize> {
    let sql = build_batch_insert_query(
        "data_health_sleep",
        &[
            "id",
            "sleep_stages",
            "duration_minutes",
            "sleep_quality_score",
            "started_at",
            "ended_at",
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
    String,        // id
    String,        // workout_type
    Option<i32>,   // duration_minutes
    Option<i32>,   // calories_burned
    Option<i32>,   // avg_heart_rate
    Option<i32>,   // max_heart_rate
    Option<f64>,   // distance_km
    DateTime<Utc>, // start_time
    DateTime<Utc>, // end_time
    String,        // source_stream_id
    Value,         // metadata
);

pub async fn write_workout(db: &PgPool, records: &[Value]) -> Result<usize> {
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
        let stream_id = stream_id_or_hash(record, HEALTHKIT_STREAM_TABLE);

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
            row_id(HEALTHKIT_STREAM_TABLE, &stream_id),
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

async fn flush_workout(db: &PgPool, records: &[WorkoutRow]) -> Result<usize> {
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
            "started_at",
            "ended_at",
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

// ─────────────────────────────────────────────────────────────────────────────
// Cycle, symptoms, wrist temperature
//
// Raw samples only; cycles and estimates are transforms over these rows (see
// agents/plan/cycle-tracking-plan.md). Each record carries the phone's calendar
// days beside the instants, because these are all-day samples and the box
// cannot recover the right day from a UTC midnight.
//
// Every row is checked against the vocabulary here before it is inserted. The
// tables have the same CHECKs, but a CHECK failure rejects the whole batch,
// and the phone retries a rejected batch forever.
// ─────────────────────────────────────────────────────────────────────────────

/// Each cycle `kind` and the values it may carry. An empty list means the kind
/// carries no value (spotting, a factor's interval, a temperature reading).
const CYCLE_KINDS: &[(&str, &[&str])] = &[
    ("flow", &["unspecified", "light", "medium", "heavy", "none"]),
    ("spotting", &[]),
    ("basal_temperature", &[]),
    ("ovulation_test", &["negative", "lh_surge", "estrogen_surge", "indeterminate"]),
    ("cervical_mucus", &["dry", "sticky", "creamy", "watery", "egg_white"]),
    ("contraceptive", &["unspecified", "implant", "injection", "iud", "ring", "oral", "patch"]),
    ("pregnancy", &[]),
    ("lactation", &[]),
    ("deviation", &["irregular", "infrequent", "prolonged", "persistent_spotting"]),
];

/// Cycle factors are intervals and may still be running (no end yet).
const OPEN_INTERVAL_KINDS: &[&str] = &["contraceptive", "pregnancy", "lactation"];

/// Every HealthKit symptom category type, as the phone names it.
const SYMPTOMS: &[&str] = &[
    "abdominal_cramps", "acne", "appetite_changes", "bladder_incontinence", "bloating",
    "breast_pain", "chest_tightness_or_pain", "chills", "constipation", "coughing",
    "diarrhea", "dizziness", "dry_skin", "fainting", "fatigue", "fever",
    "generalized_body_ache", "hair_loss", "headache", "heartburn", "hot_flashes",
    "loss_of_smell", "loss_of_taste", "lower_back_pain", "memory_lapse", "mood_changes",
    "nausea", "night_sweats", "pelvic_pain", "rapid_pounding_or_fluttering_heartbeat",
    "runny_nose", "shortness_of_breath", "sinus_congestion", "skipped_heartbeat",
    "sleep_changes", "sore_throat", "vaginal_dryness", "vomiting", "wheezing",
];

/// The values a symptom may carry, which follow its HealthKit value enum.
fn symptom_values(symptom: &str) -> &'static [&'static str] {
    match symptom {
        "mood_changes" | "sleep_changes" => &["present", "not_present"],
        "appetite_changes" => &["unspecified", "no_change", "decreased", "increased"],
        _ => &["unspecified", "not_present", "mild", "moderate", "severe"],
    }
}

fn str_field<'a>(record: &'a Value, field: &str) -> Option<&'a str> {
    record.get(field).and_then(|v| v.as_str()).filter(|s| !s.is_empty())
}

fn date_field(record: &Value, field: &str) -> Option<NaiveDate> {
    str_field(record, field).and_then(|s| s.parse::<NaiveDate>().ok())
}

fn instant_field(record: &Value, field: &str) -> Option<DateTime<Utc>> {
    str_field(record, field).and_then(|s| s.parse::<DateTime<Utc>>().ok())
}

#[derive(Debug, PartialEq)]
struct CycleSample {
    kind: String,
    value: Option<String>,
    temperature_celsius: Option<f64>,
    is_cycle_start: Option<bool>,
    started_on: NaiveDate,
    ended_on: Option<NaiveDate>,
    started_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
}

/// One cycle record, or `Err` naming why it cannot be stored.
fn parse_cycle(record: &Value) -> std::result::Result<CycleSample, String> {
    let kind = str_field(record, "kind").ok_or("no kind")?;
    let allowed = CYCLE_KINDS
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, v)| *v)
        .ok_or_else(|| format!("unknown kind {kind:?}"))?;

    let value = str_field(record, "value");
    match (allowed.is_empty(), value) {
        (true, None) => {}
        (false, Some(v)) if allowed.contains(&v) => {}
        (true, Some(v)) => return Err(format!("{kind} carries no value, got {v:?}")),
        (false, v) => return Err(format!("bad {kind} value {v:?}")),
    }

    let temperature_celsius = record.get("temperature_celsius").and_then(|v| v.as_f64());
    if (kind == "basal_temperature") != temperature_celsius.is_some() {
        return Err(format!("{kind}: temperature_celsius must be present only for basal_temperature"));
    }

    let started_on = date_field(record, "local_start_date").ok_or("no local_start_date")?;
    let started_at = instant_field(record, "timestamp").ok_or("no timestamp")?;
    let ended_at = instant_field(record, "end_timestamp");
    let open = OPEN_INTERVAL_KINDS.contains(&kind);
    // A point sample with no end date is a one-day sample; only a factor may
    // still be running.
    let ended_on = match date_field(record, "local_end_date") {
        Some(d) => Some(d),
        None if open => None,
        None => Some(started_on),
    };
    if ended_on.is_some_and(|e| e < started_on) {
        return Err("local_end_date before local_start_date".into());
    }

    Ok(CycleSample {
        kind: kind.to_string(),
        value: value.map(String::from),
        temperature_celsius,
        is_cycle_start: record.get("is_cycle_start").and_then(|v| v.as_bool()),
        started_on,
        ended_on,
        started_at,
        ended_at,
    })
}

#[derive(Debug, PartialEq)]
struct SymptomSample {
    symptom: String,
    value: String,
    started_on: NaiveDate,
    ended_on: NaiveDate,
    started_at: DateTime<Utc>,
    ended_at: DateTime<Utc>,
}

fn parse_symptom(record: &Value) -> std::result::Result<SymptomSample, String> {
    let symptom = str_field(record, "symptom").ok_or("no symptom")?;
    if !SYMPTOMS.contains(&symptom) {
        return Err(format!("unknown symptom {symptom:?}"));
    }
    let value = str_field(record, "value").ok_or("no value")?;
    if !symptom_values(symptom).contains(&value) {
        return Err(format!("bad {symptom} value {value:?}"));
    }
    let started_on = date_field(record, "local_start_date").ok_or("no local_start_date")?;
    let ended_on = date_field(record, "local_end_date").unwrap_or(started_on);
    if ended_on < started_on {
        return Err("local_end_date before local_start_date".into());
    }
    let started_at = instant_field(record, "timestamp").ok_or("no timestamp")?;
    let ended_at = instant_field(record, "end_timestamp").unwrap_or(started_at);
    Ok(SymptomSample {
        symptom: symptom.to_string(),
        value: value.to_string(),
        started_on,
        ended_on,
        started_at,
        ended_at,
    })
}

#[derive(Debug, PartialEq)]
struct WristTemperatureSample {
    temperature_celsius: f64,
    night_of: NaiveDate,
    started_at: DateTime<Utc>,
    ended_at: DateTime<Utc>,
}

fn parse_wrist_temperature(record: &Value) -> std::result::Result<WristTemperatureSample, String> {
    let temperature_celsius = record
        .get("temperature_celsius")
        .and_then(|v| v.as_f64())
        .ok_or("no temperature_celsius")?;
    let night_of = date_field(record, "local_start_date").ok_or("no local_start_date")?;
    let started_at = instant_field(record, "timestamp").ok_or("no timestamp")?;
    let ended_at = instant_field(record, "end_timestamp").unwrap_or(started_at);
    Ok(WristTemperatureSample { temperature_celsius, night_of, started_at, ended_at })
}

/// Parse every record, logging and dropping the ones that cannot be stored.
fn parse_all<T>(
    records: &[Value],
    what: &str,
    parse: fn(&Value) -> std::result::Result<T, String>,
) -> Vec<(T, String, Value)> {
    records
        .iter()
        .filter_map(|record| match parse(record) {
            Ok(sample) => Some((
                sample,
                stream_id_or_hash(record, HEALTHKIT_STREAM_TABLE),
                serde_json::json!({ "healthkit_raw": record.get("raw_data").cloned() }),
            )),
            Err(reason) => {
                tracing::warn!(what, %reason, "skipping healthkit record");
                None
            }
        })
        .collect()
}

pub async fn write_cycle(db: &PgPool, records: &[Value]) -> Result<usize> {
    let rows = parse_all(records, "cycle", parse_cycle);
    let mut written = 0;
    for chunk in rows.chunks(BATCH_SIZE) {
        let sql = build_batch_insert_query(
            "data_health_cycle_log",
            &[
                "id", "kind", "value", "temperature_celsius", "is_cycle_start",
                "started_on", "ended_on", "started_at", "ended_at",
                "source_stream_id", "source_table", "source_provider", "metadata",
            ],
            "source_stream_id",
            chunk.len(),
        );
        let mut q = sqlx::query(&sql);
        for (s, stream_id, metadata) in chunk {
            q = q
                .bind(row_id(HEALTHKIT_STREAM_TABLE, stream_id))
                .bind(&s.kind)
                .bind(&s.value)
                .bind(s.temperature_celsius)
                .bind(s.is_cycle_start)
                .bind(s.started_on)
                .bind(s.ended_on)
                .bind(s.started_at)
                .bind(s.ended_at)
                .bind(stream_id)
                .bind(HEALTHKIT_STREAM_TABLE)
                .bind(IOS_PROVIDER)
                .bind(metadata);
        }
        written += q.execute(db).await?.rows_affected() as usize;
    }
    Ok(written)
}

pub async fn write_symptom(db: &PgPool, records: &[Value]) -> Result<usize> {
    let rows = parse_all(records, "symptom", parse_symptom);
    let mut written = 0;
    for chunk in rows.chunks(BATCH_SIZE) {
        let sql = build_batch_insert_query(
            "data_health_symptom",
            &[
                "id", "symptom", "value", "started_on", "ended_on", "started_at", "ended_at",
                "source_stream_id", "source_table", "source_provider", "metadata",
            ],
            "source_stream_id",
            chunk.len(),
        );
        let mut q = sqlx::query(&sql);
        for (s, stream_id, metadata) in chunk {
            q = q
                .bind(row_id(HEALTHKIT_STREAM_TABLE, stream_id))
                .bind(&s.symptom)
                .bind(&s.value)
                .bind(s.started_on)
                .bind(s.ended_on)
                .bind(s.started_at)
                .bind(s.ended_at)
                .bind(stream_id)
                .bind(HEALTHKIT_STREAM_TABLE)
                .bind(IOS_PROVIDER)
                .bind(metadata);
        }
        written += q.execute(db).await?.rows_affected() as usize;
    }
    Ok(written)
}

pub async fn write_wrist_temperature(db: &PgPool, records: &[Value]) -> Result<usize> {
    let rows = parse_all(records, "wrist_temperature", parse_wrist_temperature);
    let mut written = 0;
    for chunk in rows.chunks(BATCH_SIZE) {
        let sql = build_batch_insert_query(
            "data_health_wrist_temperature",
            &[
                "id", "temperature_celsius", "night_of", "started_at", "ended_at",
                "source_stream_id", "source_table", "source_provider", "metadata",
            ],
            "source_stream_id",
            chunk.len(),
        );
        let mut q = sqlx::query(&sql);
        for (s, stream_id, metadata) in chunk {
            q = q
                .bind(row_id(HEALTHKIT_STREAM_TABLE, stream_id))
                .bind(s.temperature_celsius)
                .bind(s.night_of)
                .bind(s.started_at)
                .bind(s.ended_at)
                .bind(stream_id)
                .bind(HEALTHKIT_STREAM_TABLE)
                .bind(IOS_PROVIDER)
                .bind(metadata);
        }
        written += q.execute(db).await?.rows_affected() as usize;
    }
    Ok(written)
}

// ─────────────────────────────────────────────────────────────────────────────
// Deletions
//
// A sample deleted in Apple Health is tombstoned here: `deleted_at_source` is
// stamped and every reader skips the row. The row itself stays, as for every
// other source, and a re-send of the sample cannot revive it because the
// insert is ON CONFLICT DO NOTHING.
//
// The phone names only the sample's UUID, not its type (HealthKit's deleted
// object carries nothing else), so the tombstone is tried on every table a
// HealthKit sample can land in. The record's own `id` is not the sample's:
// the outbox drops a record whose id it has already seen, so the deletion
// travels as `deleted:<uuid>` with the UUID in `deleted_id`.
// ─────────────────────────────────────────────────────────────────────────────

/// Every table `healthkit_ingest` writes.
const HEALTHKIT_TABLES: &[&str] = &[
    "data_health_heart_rate",
    "data_health_hrv",
    "data_health_steps",
    "data_health_active_energy",
    "data_health_distance",
    "data_health_sleep",
    "data_health_workout",
    "data_health_cycle_log",
    "data_health_symptom",
    "data_health_wrist_temperature",
];

pub async fn mark_deleted(db: &PgPool, records: &[Value]) -> Result<usize> {
    let ids: Vec<String> = records
        .iter()
        .filter_map(|r| str_field(r, "deleted_id"))
        .map(String::from)
        .collect();
    if ids.is_empty() {
        return Ok(0);
    }
    let mut marked = 0;
    for table in HEALTHKIT_TABLES {
        // Table names are compile-time constants. Only the phone's own rows:
        // a workout from another provider never shares a HealthKit UUID, but
        // the provider check keeps that from being a matter of luck.
        let sql = format!(
            "UPDATE {table} SET deleted_at_source = now(), updated_at = now() \
             WHERE source_stream_id = ANY($1) AND source_provider = $2 \
               AND deleted_at_source IS NULL"
        );
        marked += sqlx::query(&sql)
            .bind(&ids)
            .bind(IOS_PROVIDER)
            .execute(db)
            .await?
            .rows_affected() as usize;
    }
    Ok(marked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn flow(value: &str) -> Value {
        json!({"id": format!("hk-flow-{value}"), "metric_type": "cycle", "kind": "flow",
               "value": value, "is_cycle_start": true,
               "local_start_date": "2026-09-29", "local_end_date": "2026-09-29",
               "timestamp": "2026-09-29T07:00:00Z", "end_timestamp": "2026-09-30T07:00:00Z"})
    }

    /// The calendar day comes from the phone, never from the instant: a sample
    /// logged at local midnight in UTC-7 is the 29th, though its instant is
    /// 07:00Z.
    #[test]
    fn a_flow_day_keeps_the_phones_calendar_day() {
        let s = parse_cycle(&flow("medium")).unwrap();
        assert_eq!(s.started_on, NaiveDate::from_ymd_opt(2026, 9, 29).unwrap());
        assert_eq!(s.ended_on, Some(s.started_on));
        assert_eq!(s.value.as_deref(), Some("medium"));
        assert_eq!(s.is_cycle_start, Some(true));
    }

    /// Anything the table would reject is dropped here, so one bad record can
    /// never fail its whole batch.
    #[test]
    fn values_outside_the_vocabulary_are_refused() {
        assert!(parse_cycle(&flow("gushing")).is_err());
        let mut r = flow("medium");
        r["kind"] = json!("sexual_activity");
        assert!(parse_cycle(&r).is_err(), "never read, so never stored");
        r["kind"] = json!("spotting");
        assert!(parse_cycle(&r).is_err(), "spotting carries no value");
        r.as_object_mut().unwrap().remove("value");
        assert!(parse_cycle(&r).is_ok());
    }

    #[test]
    fn temperature_belongs_to_basal_temperature_only() {
        let bbt = json!({"kind": "basal_temperature", "temperature_celsius": 36.4,
                         "local_start_date": "2026-10-01", "timestamp": "2026-10-01T13:00:00Z"});
        assert_eq!(parse_cycle(&bbt).unwrap().temperature_celsius, Some(36.4));
        let mut no_temp = bbt.clone();
        no_temp.as_object_mut().unwrap().remove("temperature_celsius");
        assert!(parse_cycle(&no_temp).is_err());
        let mut flow_with_temp = flow("light");
        flow_with_temp["temperature_celsius"] = json!(36.4);
        assert!(parse_cycle(&flow_with_temp).is_err());
    }

    /// A contraception factor with no end yet is still running; a flow sample
    /// with no end date is a single day.
    #[test]
    fn only_a_factor_may_be_open() {
        let iud = json!({"kind": "contraceptive", "value": "iud",
                         "local_start_date": "2026-01-10", "timestamp": "2026-01-10T08:00:00Z"});
        assert_eq!(parse_cycle(&iud).unwrap().ended_on, None);
        let mut f = flow("heavy");
        f.as_object_mut().unwrap().remove("local_end_date");
        assert_eq!(parse_cycle(&f).unwrap().ended_on, Some(NaiveDate::from_ymd_opt(2026, 9, 29).unwrap()));
    }

    /// Each symptom family has its own enum; zero means "unspecified" for
    /// severity and "present" for presence, so the phone must send names.
    #[test]
    fn symptom_values_follow_their_family() {
        let s = |symptom: &str, value: &str| {
            parse_symptom(&json!({"symptom": symptom, "value": value,
                                  "local_start_date": "2026-09-30",
                                  "timestamp": "2026-09-30T07:00:00Z"}))
        };
        assert!(s("abdominal_cramps", "moderate").is_ok());
        assert!(s("abdominal_cramps", "present").is_err());
        assert!(s("mood_changes", "present").is_ok());
        assert!(s("mood_changes", "mild").is_err());
        assert!(s("appetite_changes", "increased").is_ok());
        assert!(s("toe_tingling", "mild").is_err());
    }

    /// A sample deleted on the phone is tombstoned wherever it landed, and a
    /// later re-send of the same sample does not bring it back.
    #[sqlx::test(migrations = "../virtues-core/migrations")]
    async fn a_deleted_sample_is_tombstoned_and_stays_so(pool: PgPool) {
        let steps = json!({"id": "hk-steps-1", "metric_type": "steps", "value": 420,
                           "timestamp": "2026-10-01T15:00:00Z"});
        write_steps(&pool, &[steps.clone()]).await.unwrap();
        write_cycle(&pool, &[flow("light")]).await.unwrap();

        let deletions = [
            json!({"id": "deleted:hk-steps-1", "metric_type": "deleted", "deleted_id": "hk-steps-1"}),
            json!({"id": "deleted:hk-flow-light", "metric_type": "deleted", "deleted_id": "hk-flow-light"}),
            json!({"id": "deleted:never-sent", "metric_type": "deleted", "deleted_id": "never-sent"}),
        ];
        assert_eq!(mark_deleted(&pool, &deletions).await.unwrap(), 2);
        assert_eq!(mark_deleted(&pool, &deletions).await.unwrap(), 0, "already tombstoned");

        write_steps(&pool, &[steps]).await.unwrap();
        let live: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM data_health_steps WHERE deleted_at_source IS NULL",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(live, 0, "a re-send must not revive a deleted sample");
    }

    #[sqlx::test(migrations = "../virtues-core/migrations")]
    async fn raw_samples_land_once(pool: PgPool) {
        let batch = vec![
            flow("medium"),
            flow("not-a-value"),
            json!({"id": "hk-sym-1", "symptom": "headache", "value": "mild",
                   "local_start_date": "2026-09-30", "timestamp": "2026-09-30T07:00:00Z"}),
            json!({"id": "hk-wt-1", "temperature_celsius": 35.91, "local_start_date": "2026-09-29",
                   "timestamp": "2026-09-30T05:10:00Z", "end_timestamp": "2026-09-30T13:02:00Z"}),
        ];
        assert_eq!(write_cycle(&pool, &batch[..2]).await.unwrap(), 1);
        assert_eq!(write_cycle(&pool, &batch[..2]).await.unwrap(), 0, "deduped on the sample id");
        assert_eq!(write_symptom(&pool, &batch[2..3]).await.unwrap(), 1);
        assert_eq!(write_wrist_temperature(&pool, &batch[3..]).await.unwrap(), 1);

        let (day, value): (NaiveDate, String) =
            sqlx::query_as("SELECT started_on, value FROM data_health_cycle_log")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!((day.to_string(), value.as_str()), ("2026-09-29".into(), "medium"));
    }
}

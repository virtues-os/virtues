//! iOS microphone audio → file + `data_audio_recording` row.
//!
//! This is the FAST receive path. The action does NOT transcribe audio inline
//! — that would block the iOS request long enough to time out. Instead:
//!
//! 1. Decode the base64 audio bytes from the iOS payload
//! 2. Write the bytes to `data/lake/ios_microphone/{stream_id}.{ext}` (cwd-relative)
//! 3. INSERT a row into `data_audio_recording` with `audio_url` pointing at that file
//! 4. Return success in well under 1s per chunk
//!
//! Transcription happens asynchronously via the device-agnostic
//! `transcription_resolution` cron action, which LEFT JOINs
//! `data_audio_recording` against `data_communication_transcription` and
//! fills in the missing transcripts. Same resolver works for any device.
//!
//! Silent chunks (`is_silent=1`) still land in this table — they're real
//! recordings — but the transcribe drainer skips them.

use anyhow::{anyhow, Context, Result};
use base64::Engine;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::SqlitePool;
use std::path::PathBuf;
use uuid::Uuid;

/// Where audio files live on local disk. Relative to the server's cwd
/// (which is also the subprocess's cwd, since we don't override `current_dir`).
const AUDIO_DIR: &str = "data/lake/ios_microphone";

pub async fn ingest_all(db: &SqlitePool, records: &[Value]) -> Result<(usize, usize)> {
    if records.is_empty() {
        return Ok((0, 0));
    }

    // Ensure the audio directory exists once per invocation
    let audio_dir = PathBuf::from(AUDIO_DIR);
    std::fs::create_dir_all(&audio_dir)
        .with_context(|| format!("failed to create audio dir {}", audio_dir.display()))?;

    let mut written = 0;
    let mut failed = 0;

    for record in records {
        match ingest_one(db, &audio_dir, record).await {
            Ok(true) => written += 1,
            Ok(false) => {
                // Conflict (UNIQUE source_stream_id) — already ingested. Not a failure.
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to ingest audio record");
                failed += 1;
            }
        }
    }

    Ok((written, failed))
}

/// Ingest a single audio record. Returns `Ok(true)` if a new row was inserted,
/// `Ok(false)` if the record was a duplicate.
async fn ingest_one(db: &SqlitePool, audio_dir: &PathBuf, record: &Value) -> Result<bool> {
    let stream_id = record
        .get("id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let audio_b64 = record
        .get("audio_data")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("record missing audio_data"))?;

    let audio_format = record
        .get("audio_format")
        .and_then(|v| v.as_str())
        .unwrap_or("m4a");

    let start_time = record
        .get("timestamp_start")
        .or_else(|| record.get("timestamp"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
        .unwrap_or_else(Utc::now);
    let end_time = record
        .get("timestamp_end")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<DateTime<Utc>>().ok());
    let duration_seconds = record.get("duration_seconds").and_then(|v| v.as_f64());

    let is_silent = record
        .get("is_silent")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let average_db_level = record.get("average_db_level").and_then(|v| v.as_f64());

    // Decode and write the audio bytes to disk first. If this fails, we
    // never insert a row — keeps the ontology consistent.
    let audio_bytes = base64::engine::general_purpose::STANDARD
        .decode(audio_b64)
        .with_context(|| format!("failed to decode base64 audio for {stream_id}"))?;

    let filename = format!("{stream_id}.{audio_format}");
    let audio_path = audio_dir.join(&filename);
    std::fs::write(&audio_path, &audio_bytes)
        .with_context(|| format!("failed to write audio file {}", audio_path.display()))?;

    // Store path as a relative string so it survives moves between dev/prod.
    // The transcribe action resolves it relative to its own cwd (same as ours).
    let audio_url = format!("{AUDIO_DIR}/{filename}");

    let id = Uuid::new_v4().to_string();

    let result = sqlx::query(
        r#"INSERT INTO data_audio_recording (
            id, source_stream_id,
            started_at, ended_at, duration_seconds,
            audio_url, audio_format,
            is_silent, average_db_level,
            source_table, source_provider, metadata
        ) VALUES (
            $1, $2,
            $3, $4, $5,
            $6, $7,
            $8, $9,
            $10, $11, $12
        ) ON CONFLICT (source_stream_id) DO NOTHING"#,
    )
    .bind(&id)
    .bind(&stream_id)
    .bind(start_time.to_rfc3339())
    .bind(end_time.map(|t| t.to_rfc3339()))
    .bind(duration_seconds)
    .bind(&audio_url)
    .bind(audio_format)
    .bind(is_silent as i64)
    .bind(average_db_level)
    .bind("stream_ios_microphone")
    .bind("ios")
    .bind("{}")
    .execute(db)
    .await
    .with_context(|| format!("failed to insert data_audio_recording for {stream_id}"))?;

    Ok(result.rows_affected() > 0)
}

//! iOS microphone audio → media lake object + `data_audio_recording` row.
//!
//! This is the FAST receive path. The action does NOT transcribe audio inline
//! — that would block the iOS request long enough to time out. Instead:
//!
//! 1. [`externalize_blobs`] decodes the base64 audio out of each record and
//!    stores it ONCE as a `kind='media'` lake object, replacing `audio_data`
//!    with an `audio_ref` pointing at it
//! 2. INSERT a row into `data_audio_recording` with `audio_url` = that ref
//! 3. Return success in well under 1s per chunk
//!
//! Why the split: this stream's payload *is* the audio, and audio is by far the
//! box's largest data class (763 MB against 65 MB for every ontology table put
//! together). Archiving the raw payload verbatim would store all of it a second
//! time, at 1.33× for the base64 — so the blob goes to the lake once and the
//! archived record references it. That keeps the archived object replayable
//! without doubling the only thing here that actually costs anything.
//!
//! This also retires the old cwd-relative `data/lake/ios_microphone` path, which
//! ignored STORAGE_PATH and therefore parked ~763 MB *outside* the configured
//! lake, invisible to every accounting and GC pass.
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
use sqlx::PgPool;
use uuid::Uuid;
use virtues::storage::{lake, Storage};

const PROVIDER: &str = "ios";
const STREAM: &str = "microphone";

/// Pull each record's base64 audio out into a media lake object, returning the
/// records with `audio_data` replaced by `audio_ref`.
///
/// Runs BEFORE the raw records are archived, so an archived object never
/// references a blob that isn't there yet — otherwise a transform failure would
/// leave behind an object that can't be replayed.
///
/// A record whose base64 will not decode is passed through UNTOUCHED rather than
/// failing the call. That distinction is the whole ballgame: one corrupt chunk
/// used to be tolerated (`ingest_all` counted it failed and the batch still
/// returned 200), and briefly wasn't — a single bad chunk 500'd the entire push,
/// so the device retried the same poisoned batch every 5 minutes and the *nine
/// good chunks beside it* never landed either. Bad data is per-record and
/// survivable; only infrastructure failures (upload, DB) are fatal here.
///
/// The undecodable record keeps its `audio_data`, so the bytes we could not parse
/// are still archived verbatim and can be examined later — which is the entire
/// point of a raw lake.
pub async fn externalize_blobs(
    db: &PgPool,
    storage: &Storage,
    records: &[Value],
) -> Result<Vec<Value>> {
    let mut out = Vec::with_capacity(records.len());

    for record in records {
        let Some(audio_b64) = record.get("audio_data").and_then(|v| v.as_str()) else {
            // Already externalized (a replay) or malformed — pass through untouched
            // and let ingest_one decide.
            out.push(record.clone());
            continue;
        };

        let stream_id = stream_id_of(record);
        let format = audio_format_of(record);

        let bytes = match base64::engine::general_purpose::STANDARD.decode(audio_b64) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(
                    stream_id = %stream_id,
                    error = %e,
                    "undecodable base64 audio — archiving the record as-is and skipping the blob"
                );
                out.push(record.clone());
                continue;
            }
        };

        // Upload/DB errors are NOT per-record: they mean the box is broken, and the
        // right answer is to fail loudly so the device holds the batch and retries.
        let key = lake::put_media(
            db,
            storage,
            PROVIDER,
            STREAM,
            &format!("{stream_id}.{format}"),
            &bytes,
        )
        .await
        .with_context(|| format!("failed to store audio blob for {stream_id}"))?;

        let mut sanitized = record.clone();
        if let Some(obj) = sanitized.as_object_mut() {
            obj.remove("audio_data");
            obj.insert("audio_ref".into(), Value::String(key));
            // Pin the id we just used. `stream_id_of` mints a fresh UUID when the
            // record has none, so without this the blob's filename and the row's
            // source_stream_id would be two different UUIDs.
            obj.insert("id".into(), Value::String(stream_id));
        }
        out.push(sanitized);
    }

    Ok(out)
}

fn stream_id_of(record: &Value) -> String {
    record
        .get("id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

fn audio_format_of(record: &Value) -> String {
    record
        .get("audio_format")
        .and_then(|v| v.as_str())
        .unwrap_or("m4a")
        .to_string()
}

pub async fn ingest_all(db: &PgPool, records: &[Value]) -> Result<(usize, usize)> {
    if records.is_empty() {
        return Ok((0, 0));
    }

    let mut written = 0;
    let mut failed = 0;

    for record in records {
        match ingest_one(db, record).await {
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
async fn ingest_one(db: &PgPool, record: &Value) -> Result<bool> {
    let stream_id = stream_id_of(record);

    // The bytes were already stored by `externalize_blobs`; we only record where.
    let audio_url = record
        .get("audio_ref")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("record missing audio_ref (externalize_blobs must run first)"))?
        .to_string();

    let audio_format = audio_format_of(record);

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
    .bind(start_time)
    .bind(end_time)
    .bind(duration_seconds)
    .bind(&audio_url)
    .bind(audio_format)
    .bind(is_silent)
    .bind(average_db_level)
    .bind("stream_ios_microphone")
    .bind("ios")
    .bind(serde_json::json!({}))
    .execute(db)
    .await
    .with_context(|| format!("failed to insert data_audio_recording for {stream_id}"))?;

    Ok(result.rows_affected() > 0)
}

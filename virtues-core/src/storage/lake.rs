//! The lake: raw, replayable landing zone for ingested records.
//!
//! Ingest actions call [`archive`] BEFORE they transform. That ordering is the
//! whole point: if the transform is buggy the action still fails loudly (500,
//! the device retries) — but the bytes are already durable on the box, so the
//! fix is a re-run instead of a re-collection. Device pushes (iOS/Mac webhooks)
//! have no upstream to re-fetch from, so if we drop them here they are gone.
//!
//! THE INVARIANT: an archived object is itself a valid, self-contained,
//! REPLAYABLE action payload. `metadata.replay` records how to rebuild the
//! envelope the action expects:
//!
//!   mac → {"imessages": [<records>]}                 (replay.key)
//!   ios → {"stream": "location", "records": [...]}   (replay.stream)
//!
//! So replay re-runs the real action rather than a second copy of the transform
//! logic, and the archive is self-validating: if it cannot replay, we find out
//! immediately rather than on the day we need it.

use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use super::models::StreamKeyBuilder;
use super::Storage;
use crate::{Error, Result};

/// How a replayed object rebuilds the payload its action expects.
pub enum Envelope<'a> {
    /// mac_ingest: records live under a top-level key (`app_events`, `imessages`, …).
    MacKey(&'a str),
    /// ios_ingest: `{"stream": …, "records": […]}`.
    IosStream(&'a str),
}

impl Envelope<'_> {
    fn stream_name(&self) -> &str {
        match self {
            Envelope::MacKey(k) => k,
            Envelope::IosStream(s) => s,
        }
    }

    fn replay_spec(&self) -> Value {
        match self {
            Envelope::MacKey(k) => json!({"provider": "mac", "key": k}),
            Envelope::IosStream(s) => json!({"provider": "ios", "stream": s}),
        }
    }
}

/// A landed object.
pub struct LakeRef {
    pub id: String,
    pub storage_key: String,
    pub record_count: usize,
}

/// Resolve the storage root the same way `VirtuesBuilder` does (STORAGE_PATH,
/// else ./data/lake). Actions are separate processes that inherit the box's env
/// but never build a `Virtues` client, so they need this to reach the lake.
pub fn storage_from_env() -> Result<Storage> {
    let path = std::env::var("STORAGE_PATH").unwrap_or_else(|_| "./data/lake".to_string());
    Storage::file(path)
}

/// Archive one stream's records as a replayable object.
///
/// Returns `Ok(None)` when this exact content is already in the lake. That check
/// is load-bearing, not a nicety: a batch whose transform is failing is retried
/// by the device every 5 minutes for as long as its 7-day queue holds it, so
/// without content-dedupe a single broken batch archives thousands of identical
/// copies of itself. (We watched precisely that loop run for hours.)
pub async fn archive(
    pool: &PgPool,
    storage: &Storage,
    provider: &str,
    source_id: &str,
    envelope: Envelope<'_>,
    records: &[Value],
    residual: Value,
) -> Result<Option<LakeRef>> {
    if records.is_empty() {
        return Ok(None);
    }

    let stream = envelope.stream_name();

    // JSONL, uncompressed for now: at this volume (all data_* tables together are
    // 65 MB, against 763 MB of audio) compression buys nothing, and plain text
    // keeps the archive greppable while we learn what it actually contains.
    let mut bytes = Vec::new();
    for record in records {
        let line = serde_json::to_string(record)
            .map_err(|e| Error::Other(format!("lake: failed to serialize record: {e}")))?;
        bytes.extend_from_slice(line.as_bytes());
        bytes.push(b'\n');
    }

    let sha256 = hex_digest(&bytes);

    // Check before writing, so a duplicate doesn't leave an orphan file behind.
    let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM lake_objects WHERE sha256 = $1")
        .bind(&sha256)
        .fetch_optional(pool)
        .await?;
    if existing.is_some() {
        return Ok(None);
    }

    let now = Utc::now();
    let (min_ts, max_ts) = time_window(records);

    let storage_key = StreamKeyBuilder::new(provider, source_id, stream, now.date_naive())
        .build_with_timestamp(now.timestamp_micros());

    storage.upload(&storage_key, bytes.clone()).await?;

    let id = Uuid::new_v4().to_string();
    let metadata = json!({ "replay": envelope.replay_spec(), "residual": residual });

    // ON CONFLICT is the race backstop only; the SELECT above is the real gate.
    let inserted: Option<(String,)> = sqlx::query_as(
        "INSERT INTO lake_objects (
             id, kind, storage_key, provider, source_id, stream_name,
             record_count, size_bytes, sha256, content_encoding,
             min_timestamp, max_timestamp, metadata
         ) VALUES ($1, 'raw_stream', $2, $3, $4, $5, $6, $7, $8, 'none', $9, $10, $11)
         ON CONFLICT (sha256) DO NOTHING
         RETURNING id",
    )
    .bind(&id)
    .bind(&storage_key)
    .bind(provider)
    .bind(source_id)
    .bind(stream)
    .bind(records.len() as i32)
    .bind(bytes.len() as i64)
    .bind(&sha256)
    .bind(min_ts)
    .bind(max_ts)
    .bind(&metadata)
    .fetch_optional(pool)
    .await?;

    Ok(inserted.map(|_| LakeRef {
        id,
        storage_key,
        record_count: records.len(),
    }))
}

/// Store a blob once and reference it, rather than inline in a raw payload.
///
/// The microphone stream's payload IS its audio (base64), so archiving it
/// verbatim would store the box's single largest data class a second time, at
/// 1.33× for the base64. Instead the blob lands here and the raw record keeps an
/// `audio_ref` pointing at this key.
pub async fn put_media(
    pool: &PgPool,
    storage: &Storage,
    provider: &str,
    stream: &str,
    filename: &str,
    bytes: &[u8],
) -> Result<String> {
    let storage_key = format!("media/{provider}/{stream}/{filename}");
    let sha256 = hex_digest(bytes);

    let existing: Option<(String,)> =
        sqlx::query_as("SELECT storage_key FROM lake_objects WHERE sha256 = $1")
            .bind(&sha256)
            .fetch_optional(pool)
            .await?;
    if let Some((key,)) = existing {
        return Ok(key);
    }

    storage.upload(&storage_key, bytes.to_vec()).await?;

    sqlx::query(
        "INSERT INTO lake_objects (
             id, kind, storage_key, provider, source_id, stream_name,
             record_count, size_bytes, sha256, content_encoding
         ) VALUES ($1, 'media', $2, $3, $3, $4, 0, $5, $6, 'none')
         ON CONFLICT (sha256) DO NOTHING",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&storage_key)
    .bind(provider)
    .bind(stream)
    .bind(bytes.len() as i64)
    .bind(&sha256)
    .execute(pool)
    .await?;

    Ok(storage_key)
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// The object's time window, used by re-projection to scope its delete. An object
/// with no window can never be safely re-projected, so cast a wide net over the
/// timestamp keys the various collectors actually emit.
fn time_window(records: &[Value]) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
    const KEYS: [&str; 5] = ["timestamp", "date", "start_time", "startDate", "start_date"];

    let mut min: Option<DateTime<Utc>> = None;
    let mut max: Option<DateTime<Utc>> = None;

    for record in records {
        let ts = KEYS
            .iter()
            .find_map(|k| record.get(*k).and_then(|v| v.as_str()))
            .and_then(|s| s.parse::<DateTime<Utc>>().ok());
        if let Some(ts) = ts {
            min = Some(min.map_or(ts, |m| m.min(ts)));
            max = Some(max.map_or(ts, |m| m.max(ts)));
        }
    }

    (min, max)
}

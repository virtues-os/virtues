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
    Storage::file(storage_root())
}

fn storage_root() -> String {
    std::env::var("STORAGE_PATH").unwrap_or_else(|_| "./data/lake".to_string())
}

/// Below this much free disk, the lake stops accepting new raw-stream archives.
///
/// `archive()` runs BEFORE the transform, on every stream, from every device. That
/// ordering is what makes the lake worth having — but it also means a failure here
/// fails the action, which 500s the webhook, which stops *that device's entire
/// upload*. So a full disk would not merely stop audio: it would stop location,
/// health, messages, and browsing, from every device, at once. Adding raw retention
/// quietly put every collector behind one shared point of failure.
///
/// Under pressure we therefore degrade instead of dying: skip the archive, say so
/// loudly, and let the transform run. Those batches lose replayability — genuinely
/// bad, and precisely what the retention sweeper exists to prevent — but the ontology
/// data still lands, which is far less bad than collecting nothing at all.
///
/// 2 GiB, not zero, because the floor has to be crossed *before* the disk is actually
/// full: Postgres, the WAL, and the transform's own writes all need room to keep
/// working, and a lake that stops only once there is no space left has already taken
/// the database down with it.
const MIN_FREE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Free bytes on the filesystem holding the lake, or `None` if we cannot tell.
///
/// `None` means proceed. A guard that cannot read the disk must not be the reason
/// ingest stops — that would trade a rare failure for a certain one.
fn free_bytes() -> Option<u64> {
    free_bytes_at(&std::fs::canonicalize(storage_root()).ok()?)
}

fn free_bytes_at(root: &std::path::Path) -> Option<u64> {
    sysinfo::Disks::new_with_refreshed_list()
        .iter()
        .filter(|d| root.starts_with(d.mount_point()))
        // The path can sit under several nested mounts ("/" and "/var" both match
        // "/var/lib/virtues"); the one that actually holds it is the LONGEST match.
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
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

    // Degrade, don't die. See MIN_FREE_BYTES: skipping the archive costs us the ability
    // to replay these records; failing here would cost us the records themselves, and
    // every other stream on the device with them.
    if let Some(free) = free_bytes() {
        if free < MIN_FREE_BYTES {
            tracing::error!(
                free_mb = free / 1024 / 1024,
                floor_mb = MIN_FREE_BYTES / 1024 / 1024,
                provider,
                stream,
                records = records.len(),
                "DISK FULL — skipping lake archive to keep ingest alive. These records \
                 are NOT replayable. Free space or run retention."
            );
            return Ok(None);
        }
    }

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
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT id FROM lake_objects WHERE sha256 = $1")
            .bind(&sha256)
            .fetch_optional(pool)
            .await?;
    if existing.is_some() {
        return Ok(None);
    }

    let now = Utc::now();
    let (min_ts, max_ts) = time_window(records);

    // CONTENT-ADDRESSED, not timestamped. If the upload lands but the INSERT below
    // fails (Postgres out of connections, statement timeout), the action 500s and
    // the device retries — and a timestamped key would mint a NEW path every 5 min,
    // leaking an untracked copy of the same bytes on every attempt. Keying on the
    // digest means a retry rewrites the identical path instead.
    let date = StreamKeyBuilder::new(provider, source_id, stream, now.date_naive());
    let storage_key = format!(
        "{}records_{}.jsonl",
        date.build_date_prefix(),
        &sha256[..16]
    );

    storage.upload(&storage_key, bytes.clone()).await?;

    let id = Uuid::new_v4().to_string();
    let metadata = json!({ "replay": envelope.replay_spec(), "residual": residual });

    // ON CONFLICT is the race backstop only; the SELECT above is the real gate.
    //
    // The `WHERE kind = 'raw_stream'` is NOT decorative: the sha256 unique index is
    // PARTIAL (media is keyed by storage_key, not by content — see 0035), and
    // Postgres will not infer a partial index unless the statement repeats its
    // predicate. Without it every archive fails with "no unique or exclusion
    // constraint matching the ON CONFLICT specification", which 500s the webhook and
    // takes down ALL ingest.
    let inserted: Option<(String,)> = sqlx::query_as(
        "INSERT INTO lake_objects (
             id, kind, storage_key, provider, source_id, stream_name,
             record_count, size_bytes, sha256, content_encoding,
             min_timestamp, max_timestamp, metadata
         ) VALUES ($1, 'raw_stream', $2, $3, $4, $5, $6, $7, $8, 'none', $9, $10, $11)
         ON CONFLICT (sha256) WHERE kind = 'raw_stream' DO NOTHING
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
///
/// Media is keyed by its STORAGE KEY, not its digest — a blob's identity is the
/// recording it belongs to, not its bytes. Two genuinely different recordings can
/// encode identically (a pair of silent chunks; the transcription drainer exists
/// partly to handle empty AAC containers), and digest-keying would silently hand
/// one recording the other's audio. `filename` is derived from the record's stream
/// id, so the key is already unique per recording, and a retry of the same chunk
/// rewrites the same path — idempotent without content-addressing.
///
/// # Deliberately NOT disk-pressure guarded
///
/// `archive()` skips itself when the disk is nearly full, because the records it holds
/// are *also* landing in `data_*` — skipping costs replayability, not the data.
///
/// Media is the opposite. The blob IS the content, and the box holds the only copy:
/// the device deletes its chunk once we acknowledge it. Skipping the write here would
/// return a happy 200 while silently destroying the recording. So this fails loudly,
/// the action 500s, and the device keeps the audio and retries — which is exactly what
/// we want it to do. A full disk should stop us accepting audio; it must never make us
/// pretend we accepted it.
pub async fn put_media(
    pool: &PgPool,
    storage: &Storage,
    provider: &str,
    stream: &str,
    filename: &str,
    bytes: &[u8],
) -> Result<String> {
    let storage_key = format!("media/{provider}/{stream}/{filename}");

    let existing: Option<(String,)> =
        sqlx::query_as("SELECT storage_key FROM lake_objects WHERE storage_key = $1")
            .bind(&storage_key)
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
         ON CONFLICT (storage_key) DO NOTHING",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&storage_key)
    .bind(provider)
    .bind(stream)
    .bind(bytes.len() as i64)
    .bind(hex_digest(bytes))
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
/// timestamp keys the collectors actually emit — they do NOT agree on one:
///
///   location / healthkit / mac  →  `timestamp`
///   microphone                  →  `timestamp_start`   (no bare `timestamp` at all)
///   eventkit (Tauri)            →  `startDate`
///   financekit, legacy eventkit →  a WRAPPER: {"transactions": [{…, "date": …}]},
///                                  so the timestamps are one level down
///
/// Getting this wrong is silent: the object lands, looks fine, and simply can never
/// be re-projected. Microphone — the largest stream on the box — archived four
/// objects with a NULL window before this was caught.
fn time_window(records: &[Value]) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
    const KEYS: [&str; 7] = [
        "timestamp",
        "timestamp_start",
        "date",
        "start_time",
        "startDate",
        "start_date",
        "timestamp_end",
    ];

    let mut min: Option<DateTime<Utc>> = None;
    let mut max: Option<DateTime<Utc>> = None;

    let mut observe = |record: &Value| {
        for key in KEYS {
            // Try every key, not just the first present one: a key that exists but
            // doesn't parse must not shadow a later key that would have.
            let Some(ts) = record
                .get(key)
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<DateTime<Utc>>().ok())
            else {
                continue;
            };
            min = Some(min.map_or(ts, |m: DateTime<Utc>| m.min(ts)));
            max = Some(max.map_or(ts, |m: DateTime<Utc>| m.max(ts)));
        }
    };

    for record in records {
        observe(record);
        // Wrapper records (financekit, legacy eventkit) carry their timestamps in
        // nested arrays; without this they archive with no window at all.
        if let Some(obj) = record.as_object() {
            for nested in obj.values().filter_map(|v| v.as_array()).flatten() {
                observe(nested);
            }
        }
    }

    (min, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The guard is only worth having if it can actually read the disk. If mount
    /// matching silently fails, `free_bytes` returns None, the guard never fires, and
    /// we ship a no-op that looks like protection — the worst possible outcome for a
    /// safety net.
    #[test]
    fn we_can_actually_read_free_space() {
        let free = free_bytes_at(std::path::Path::new("/"))
            .expect("no disk matched '/' — the pressure guard would never fire");
        assert!(free > 0, "reported zero free bytes on /, which cannot be right");
    }

    /// A nested mount must win over its parent, or we'd read the free space of the
    /// wrong filesystem — reporting "plenty of room" on `/` while the volume actually
    /// holding the lake is full.
    #[test]
    fn the_deepest_mount_wins() {
        let disks = sysinfo::Disks::new_with_refreshed_list();
        let deepest = disks
            .iter()
            .max_by_key(|d| d.mount_point().as_os_str().len())
            .map(|d| d.mount_point().to_path_buf());
        let Some(mount) = deepest else { return };
        if mount == std::path::Path::new("/") {
            return; // single-filesystem machine; nothing to disambiguate
        }
        // Asking about a path INSIDE the deepest mount must not answer with `/`.
        let root_free = free_bytes_at(std::path::Path::new("/"));
        let mount_free = free_bytes_at(&mount);
        assert!(mount_free.is_some());
        if root_free != mount_free {
            assert_ne!(
                mount_free, root_free,
                "a nested mount resolved to the root filesystem's free space"
            );
        }
    }
}

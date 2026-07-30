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
    /// A cloud sync (`google_gmail_sync`, `plaid_transactions_sync`, …).
    ///
    /// These PULL rather than receive, so there is no webhook body to archive — what
    /// we keep is the raw API response, one record per page. See [`archive_cloud`].
    Cloud {
        /// The action that fetched it, e.g. `plaid_transactions_sync`. Replay, when it
        /// exists, has to hand these pages back to the action that understands them.
        action: &'a str,
        /// The logical stream, e.g. `transactions`.
        stream: &'a str,
    },
}

impl Envelope<'_> {
    fn stream_name(&self) -> &str {
        match self {
            Envelope::MacKey(k) => k,
            Envelope::IosStream(s) => s,
            Envelope::Cloud { stream, .. } => stream,
        }
    }

    fn replay_spec(&self) -> Value {
        match self {
            Envelope::MacKey(k) => json!({"provider": "mac", "key": k}),
            Envelope::IosStream(s) => json!({"provider": "ios", "stream": s}),
            Envelope::Cloud { action, stream } => {
                json!({"provider": "cloud", "action": action, "stream": stream, "pages": true})
            }
        }
    }
}

/// A landed object.
pub struct LakeRef {
    pub id: String,
    pub storage_key: String,
    pub record_count: usize,
}

/// Resolve the storage root. Actions are separate processes that inherit the
/// box's env but never build a `Virtues` client, so they need this to reach the
/// lake.
pub fn storage_from_env() -> Result<Storage> {
    Storage::file(lake_root().to_string_lossy().into_owned())
}

/// Well-known lake location on an installed box, alongside `models/` and
/// `applets/` under the service's data dir. Kept in sync with the installer,
/// which writes `STORAGE_PATH=<data_dir>/lake` into the env file
/// (`tools/virtues-installer/src/install.rs`).
const WELL_KNOWN_LAKE_DIR: &str = "/var/lib/virtues/lake";

/// Dev-only lake, relative to virtues-core. Resolved against
/// `CARGO_MANIFEST_DIR`, never the cwd — see `lake_root`.
const DEV_LAKE_DIR_FROM_CORE: &str = "../data/lake";

/// The one place the lake's location is decided.
///
/// **Every reader and every writer must resolve this identically, default
/// included.** That is not style advice; skipping it has cost real data twice:
///
/// - A collector that built its own cwd-relative path parked ~763 MB of audio
///   *outside* the configured lake, invisible to every accounting and GC pass
///   (`cli/lake_adopt.rs` exists solely to rescue it).
/// - A reader that omitted the default while the writer applied it looked for
///   recordings relative to the cwd that had been written to the lake — so on
///   any box without `STORAGE_PATH` set, audio landed fine and then silently
///   never transcribed.
///
/// Both are the same bug: two path expressions that agree on one machine and
/// diverge on another. One function is the fix.
///
/// Precedence:
/// 1. `STORAGE_PATH` — what the installer writes and systemd hands every
///    process, server and applet subprocess alike.
/// 2. The well-known box path, chosen when `/var/lib/virtues` exists. Keying on
///    the **parent** matters: the lake legitimately does not exist yet on a box
///    that has never ingested anything, and falling back to a source path there
///    would write data somewhere production never reads. (Same reasoning as
///    `applet_templates::state_root`.)
/// 3. A dev path fixed relative to this crate. Manifest-relative rather than
///    cwd-relative so that `cargo run`, a `cargo test` with its own working
///    directory, and an applet binary invoked from anywhere all name the same
///    directory — which is precisely what the cwd-relative default did not do.
pub fn lake_root() -> std::path::PathBuf {
    let on_a_box = std::path::Path::new(WELL_KNOWN_LAKE_DIR)
        .parent()
        .is_some_and(|p| p.is_dir());
    resolve_lake_root(std::env::var("STORAGE_PATH").ok().as_deref(), on_a_box)
}

/// The decision itself, with both inputs passed in so it can be tested. Reading
/// the environment and stat-ing the disk stay in `lake_root`; a resolver whose
/// only coverage is the machine it happens to run on is how the two historical
/// divergences went unnoticed in the first place.
fn resolve_lake_root(configured: Option<&str>, on_a_box: bool) -> std::path::PathBuf {
    if let Some(dir) = configured {
        if !dir.is_empty() {
            return std::path::PathBuf::from(dir);
        }
    }
    if on_a_box {
        return std::path::PathBuf::from(WELL_KNOWN_LAKE_DIR);
    }
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DEV_LAKE_DIR_FROM_CORE)
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
    free_bytes_at(&std::fs::canonicalize(lake_root()).ok()?)
}

/// Shared rather than reimplemented: `upgrade`'s pre-migration dump needs the
/// same "which mount actually holds this path" answer, and a second copy of the
/// longest-prefix logic would be a third one in this tree.
pub(crate) fn free_bytes_at(root: &std::path::Path) -> Option<u64> {
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

/// Archive a cloud sync's RAW API responses, before anything parses them.
///
/// # Archive the response, not the records you extracted from it
///
/// The tempting thing is to hand this the `Vec<Value>` the action already built on its
/// way to the transform. Don't. That is a cache of today's schema, not evidence: it
/// keeps exactly the fields the current transform happens to read, so the lake could
/// only ever tell us what we already understood. The whole premise — you can
/// re-integrate new stories from evidence, but never evidence from stories — dies at
/// that line.
///
/// What the raw response carries that our parsers currently drop is not hypothetical:
/// Gmail's headers, labels and MIME structure; Notion's block tree; and Plaid's
/// `removed` list, which today is read by nobody and thrown away on arrival.
///
/// So `pages` is one entry per API response body, verbatim.
///
/// # Cloud syncs pull, so there is no payload to preserve
///
/// Device streams archive the webhook body and get replay for free — the object IS the
/// payload. A cloud sync has no such thing; it has a cursor and a network call. Making
/// these objects replayable means teaching each action to accept its pages back instead
/// of fetching, and that is deliberately NOT done yet. Storing the bytes is the
/// irreversible half; replaying them is not, and can be built any time. There is no
/// reason to hold the evidence hostage to the machinery that reads it.
///
/// # Known gap: `min_timestamp` / `max_timestamp` may be NULL here
///
/// [`time_window`] recognises the timestamp keys our *collectors* emit. Cloud providers
/// use their own (`internalDate` in epoch millis, `last_edited_time`, nested
/// `start.dateTime`), so some cloud objects will land with no window. That costs
/// nothing today — a window is only needed to scope a re-projection's delete — but it
/// must be closed before replay ships, and it is silent, so it is written down here.
pub async fn archive_cloud(
    pool: &PgPool,
    storage: &Storage,
    provider: &str,
    action: &str,
    stream: &str,
    pages: &[Value],
) -> Result<Option<LakeRef>> {
    archive(
        pool,
        storage,
        provider,
        action,
        Envelope::Cloud { action, stream },
        pages,
        json!({}),
    )
    .await
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

    #[test]
    fn configured_path_wins_everywhere() {
        for on_a_box in [true, false] {
            assert_eq!(
                resolve_lake_root(Some("/mnt/archive/lake"), on_a_box),
                std::path::Path::new("/mnt/archive/lake"),
                "STORAGE_PATH must win regardless of what is on disk"
            );
        }
    }

    #[test]
    fn empty_configured_value_falls_through() {
        // systemd hands down `STORAGE_PATH=` as an empty string, not an absent
        // var. Treating that as a configured path would resolve the lake to the
        // process's working directory.
        assert_eq!(
            resolve_lake_root(Some(""), true),
            std::path::Path::new(WELL_KNOWN_LAKE_DIR)
        );
    }

    #[test]
    fn box_uses_the_well_known_path() {
        assert_eq!(
            resolve_lake_root(None, true),
            std::path::Path::new(WELL_KNOWN_LAKE_DIR)
        );
    }

    #[test]
    fn dev_fallback_is_absolute_and_cwd_independent() {
        let root = resolve_lake_root(None, false);
        // The property that matters. A cwd-relative default is what parked
        // ~763 MB outside the lake and what made a reader and a writer disagree
        // about where audio lived.
        assert!(
            root.is_absolute(),
            "dev fallback must not depend on the working directory, got {}",
            root.display()
        );
        assert!(root.ends_with("data/lake"), "unexpected dev path: {root:?}");
    }
}

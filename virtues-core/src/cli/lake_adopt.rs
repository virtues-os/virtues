//! `virtues lake-adopt` — pull orphaned media into the lake.
//!
//! `ios_ingest::microphone` used to write audio with a CWD-RELATIVE path
//! (`data/lake/ios_microphone/…`), bypassing the `Storage` abstraction entirely.
//! systemd sets `WorkingDirectory=/var/lib/virtues`, so ~750 MB of recordings
//! landed in `/var/lib/virtues/data/lake/` — a sibling of the configured lake at
//! `/var/lib/virtues/lake`, and outside it.
//!
//! The consequence isn't cosmetic. Those files are invisible to `lake_objects`,
//! so the lake API undercounts the box's disk by an order of magnitude, and any
//! GC or retention pass — which the box badly needs, since nothing has ever
//! deleted a recording and they accrue at ~260 MB/day — has nothing to count.
//!
//! This command is idempotent: it adopts only recordings whose `audio_url` is
//! still legacy-shaped, and it copies (never moves) before rewriting the DB, so a
//! failure leaves the original in place and the row still pointing at it. Removing
//! the old directory is deliberately NOT automatic — verify first, then delete.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use super::ui;
use crate::Result;

/// Rows written before the lake existed. `read_audio` in the transcription action
/// resolves both shapes, which is why the box kept transcribing throughout.
const LEGACY_PREFIX: &str = "data/lake/ios_microphone/";

pub async fn run(dry_run: bool) -> Result<()> {
    let database_url = crate::database::normalize_database_url()?;
    let db = crate::database::Database::new(&database_url)?;
    db.initialize().await?;
    let pool = db.pool();

    let storage_root = crate::storage::lake::lake_root();
    // The legacy path is relative to the service's working directory, which is the
    // PARENT of the storage root — that adjacency is the whole bug.
    let cwd = std::env::current_dir()?;

    ui::section("lake: adopt orphaned media");

    let rows = sqlx::query(
        "SELECT source_stream_id, audio_url
         FROM data_audio_recording
         WHERE audio_url NOT LIKE 'media/%'
         ORDER BY created_at",
    )
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        ui::ok("nothing to adopt — every recording is already in the lake");
        return Ok(());
    }

    ui::kv("recordings to adopt", &rows.len().to_string());
    ui::kv("storage root", &storage_root.to_string_lossy());
    if dry_run {
        ui::warn("dry run — no files copied, no rows rewritten");
    }

    let mut adopted = 0usize;
    let mut bytes_total = 0u64;
    let mut missing = 0usize;

    for row in &rows {
        let stream_id: String = row.try_get("source_stream_id")?;
        let audio_url: String = row.try_get("audio_url")?;

        let src = resolve_legacy(&cwd, &audio_url);
        let Ok(bytes) = std::fs::read(&src) else {
            missing += 1;
            continue;
        };

        let filename = Path::new(&audio_url)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("{stream_id}.m4a"));
        let new_key = format!("media/ios/microphone/{filename}");
        let dest = PathBuf::from(&storage_root).join(&new_key);

        if dry_run {
            adopted += 1;
            bytes_total += bytes.len() as u64;
            continue;
        }

        // COPY, then rewrite, then (manually, later) delete. A move that fails
        // halfway leaves a row pointing at a file that no longer exists.
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, &bytes)?;

        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let sha256 = format!("{:x}", hasher.finalize());

        sqlx::query(
            "INSERT INTO lake_objects (
                 id, kind, storage_key, provider, source_id, stream_name,
                 record_count, size_bytes, sha256, content_encoding
             ) VALUES ($1, 'media', $2, 'ios', 'ios', 'microphone', 0, $3, $4, 'none')
             ON CONFLICT (storage_key) DO NOTHING",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&new_key)
        .bind(bytes.len() as i64)
        .bind(&sha256)
        .execute(pool)
        .await?;

        // Only now does the row stop pointing at the legacy file.
        sqlx::query("UPDATE data_audio_recording SET audio_url = $1 WHERE source_stream_id = $2")
            .bind(&new_key)
            .bind(&stream_id)
            .execute(pool)
            .await?;

        adopted += 1;
        bytes_total += bytes.len() as u64;
    }

    println!();
    ui::kv("adopted", &format!("{adopted} recordings"));
    ui::kv("bytes", &format!("{:.1} MB", bytes_total as f64 / 1_048_576.0));
    if missing > 0 {
        ui::warn(&format!(
            "{missing} recordings reference a file that no longer exists — left untouched"
        ));
    }

    if !dry_run && adopted > 0 {
        println!();
        ui::ok("adopted. The originals are UNTOUCHED.");
        ui::skip("verify transcription still reads them, then remove the legacy directory");
    }

    Ok(())
}

/// The legacy `audio_url` is relative to the service's cwd. Resolve it there, and
/// fall back to the literal path so this also works when run from elsewhere.
fn resolve_legacy(cwd: &Path, audio_url: &str) -> PathBuf {
    debug_assert!(audio_url.starts_with(LEGACY_PREFIX) || !audio_url.starts_with("media/"));
    let joined = cwd.join(audio_url);
    if joined.exists() {
        joined
    } else {
        PathBuf::from(audio_url)
    }
}

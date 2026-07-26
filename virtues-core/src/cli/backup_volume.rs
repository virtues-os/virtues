//! Shipping backups to a registered volume.
//!
//! Two artifact kinds land on the drive, and they have opposite lifetimes:
//!
//! - **`full-<ts>.tar.gz.age`** — database, applet state, env file. Written
//!   every run, complete every time, and pruned freely: the newest one
//!   supersedes the rest.
//! - **`lake-<ts>.tar.gz.age`** — the lake files added since the last run.
//!   **Never pruned.** The lake is append-only, so each file exists in exactly
//!   one increment; deleting an increment permanently loses everything it holds.
//!
//! That split is the whole design. Re-archiving the entire lake every run would
//! rewrite hundreds of gigabytes to capture a day's change — hours over USB, on
//! a bus that drops under sustained load. Mirroring it file-by-file instead
//! would mean an age header and an encrypt call per object, which on a lake of
//! many small stream files is both slower and the worst possible USB write
//! pattern.

use std::path::{Path, PathBuf};

use sqlx::PgPool;

use super::backup::{self, Sources};
use crate::storage::volumes::Volume;

/// Below this much free space, stop writing and say so.
///
/// Distinct from a retention target: this is the point at which a run refuses
/// rather than prunes, because the only things left to prune would be lake
/// increments, and those are irreplaceable.
const MIN_FREE_BYTES: u64 = 1024 * 1024 * 1024;

pub struct VolumeBackup {
    pub full: PathBuf,
    pub increment: Option<PathBuf>,
    pub new_files: usize,
    pub new_bytes: u64,
    pub pruned: usize,
}

/// Run one backup against `volume`.
///
/// Returns `Ok(None)` when the volume is not attached. An absent drive is a
/// skipped run, never a failure — a box whose backup destination is unplugged
/// must keep working, which is the entire reason removable media is acceptable
/// for backups and not for live storage.
pub async fn run(
    pool: &PgPool,
    volume: &Volume,
    sources: &Sources,
    recipient: &age::x25519::Recipient,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Option<VolumeBackup>, crate::Error> {
    let Some(root) = volume.root() else {
        return Ok(None);
    };
    run_at(pool, volume, &root, sources, recipient, now)
        .await
        .map(Some)
}

/// The write itself, against an already-resolved root.
///
/// Split from `run` so resolution — which needs real block devices under
/// `/dev/disk/by-uuid` — is not in the way of testing what the backup actually
/// does.
pub(crate) async fn run_at(
    pool: &PgPool,
    volume: &Volume,
    root: &Path,
    sources: &Sources,
    recipient: &age::x25519::Recipient,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<VolumeBackup, crate::Error> {
    let archives = root.join("archives");
    std::fs::create_dir_all(&archives)
        .map_err(|e| crate::Error::Other(format!("create {}: {e}", archives.display())))?;

    reconcile(pool, volume, &archives).await?;

    let stamp = now.format("%Y%m%dT%H%M%SZ").to_string();

    // The lake increment first: it is the artifact that cannot be recreated, so
    // if space runs out it should be the one that already landed.
    let pending = pending_files(pool, volume, &sources.lake).await?;
    let new_bytes: u64 = pending.iter().map(|(_, _, size)| size).sum();
    ensure_room(&archives, new_bytes)?;

    let mut increment = None;
    if !pending.is_empty() {
        let name = format!("lake-{stamp}.tar.gz.age");
        let dest = archives.join(&name);
        let members: Vec<(PathBuf, String)> = pending
            .iter()
            .map(|(abs, rel, _)| (abs.clone(), rel.clone()))
            .collect();
        backup::write_members(&members, &dest, recipient, &format!("lake increment {stamp}"))?;
        record_increment(pool, volume, &name, &pending).await?;
        increment = Some(dest);
    }

    // Then the full artifact. Cheap relative to the lake and complete every
    // time, so a restore only ever needs the newest one plus every increment.
    let full = archives.join(format!("full-{stamp}.tar.gz.age"));
    backup::write_archive_to(pool, &full, sources, recipient).await?;

    let pruned = prune_full_archives(&archives)?;

    Ok(VolumeBackup {
        full,
        increment,
        new_files: pending.len(),
        new_bytes,
        pruned,
    })
}

/// Drop bookkeeping for increments that are no longer on the volume.
///
/// The drive is authoritative about which increments exist. If one is missing —
/// wiped drive, swapped drive, operator deleted it — the files it carried are
/// gone, so the rows claiming they are backed up are wrong. Dropping them makes
/// the next run re-send those files instead of leaving a permanent hole nobody
/// would notice until a restore came up short.
async fn reconcile(pool: &PgPool, volume: &Volume, archives: &Path) -> Result<(), crate::Error> {
    let mut present: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(archives) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with("lake-") {
                present.push(name);
            }
        }
    }
    let dropped = sqlx::query(
        "DELETE FROM backup_archived_file \
         WHERE volume_id = $1 AND increment <> ALL($2)",
    )
    .bind(&volume.id)
    .bind(&present)
    .execute(pool)
    .await
    .map_err(|e| crate::Error::Database(format!("reconcile increments: {e}")))?
    .rows_affected();
    if dropped > 0 {
        eprintln!(
            "warning: {dropped} file(s) were recorded as backed up to {} but their \
             increment is no longer on the volume — they will be re-sent",
            volume.name
        );
    }
    Ok(())
}

/// Lake files not yet on this volume, as `(absolute, relative, size)`.
async fn pending_files(
    pool: &PgPool,
    volume: &Volume,
    lake: &Path,
) -> Result<Vec<(PathBuf, String, u64)>, crate::Error> {
    let archived: Vec<String> =
        sqlx::query_scalar("SELECT rel_path FROM backup_archived_file WHERE volume_id = $1")
            .bind(&volume.id)
            .fetch_all(pool)
            .await
            .map_err(|e| crate::Error::Database(format!("read archived files: {e}")))?;
    let archived: std::collections::HashSet<String> = archived.into_iter().collect();

    let mut all = Vec::new();
    backup::collect_files_pub(lake, "lake", &mut all)?;
    let mut pending = Vec::new();
    for (abs, rel) in all {
        if archived.contains(&rel) {
            continue;
        }
        let size = std::fs::metadata(&abs).map(|m| m.len()).unwrap_or(0);
        pending.push((abs, rel, size));
    }
    Ok(pending)
}

async fn record_increment(
    pool: &PgPool,
    volume: &Volume,
    increment: &str,
    files: &[(PathBuf, String, u64)],
) -> Result<(), crate::Error> {
    let rels: Vec<String> = files.iter().map(|(_, r, _)| r.clone()).collect();
    let sizes: Vec<i64> = files.iter().map(|(_, _, s)| *s as i64).collect();
    sqlx::query(
        "INSERT INTO backup_archived_file (volume_id, rel_path, increment, size_bytes) \
         SELECT $1, r, $3, s FROM UNNEST($2::TEXT[], $4::BIGINT[]) AS t(r, s) \
         ON CONFLICT (volume_id, rel_path) DO NOTHING",
    )
    .bind(&volume.id)
    .bind(&rels)
    .bind(increment)
    .bind(&sizes)
    .execute(pool)
    .await
    .map_err(|e| crate::Error::Database(format!("record increment: {e}")))?;
    Ok(())
}

/// Refuse before writing rather than fill the volume.
///
/// A run that dies partway leaves a `.partial` and a volume with no room to
/// retry, which is a worse position than not having started.
fn ensure_room(archives: &Path, need: u64) -> Result<(), crate::Error> {
    let Some(free) = crate::storage::lake::free_bytes_at(archives) else {
        return Ok(());
    };
    if free < need.saturating_add(MIN_FREE_BYTES) {
        return Err(crate::Error::Other(format!(
            "not enough room on the backup volume: {} MB free, need {} MB plus \
             headroom. Lake increments are never pruned — each file exists in \
             exactly one, so deleting them would lose data permanently. This \
             volume needs more space, or a larger one.",
            free / (1024 * 1024),
            need / (1024 * 1024),
        )));
    }
    Ok(())
}

/// Keep the newest full archive and drop older ones while space is short.
///
/// Only ever touches `full-*`. Increments are excluded by construction, not by
/// policy — see the module doc.
fn prune_full_archives(archives: &Path) -> Result<usize, crate::Error> {
    let mut fulls: Vec<(std::time::SystemTime, PathBuf, u64)> = std::fs::read_dir(archives)
        .map_err(|e| crate::Error::Other(format!("read {}: {e}", archives.display())))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("full-"))
        })
        .filter_map(|p| {
            let m = std::fs::metadata(&p).ok()?;
            Some((m.modified().ok()?, p, m.len()))
        })
        .collect();
    fulls.sort_by(|a, b| b.0.cmp(&a.0)); // newest first

    let mut pruned = 0;
    for (_, path, _) in fulls.iter().skip(1) {
        let free = crate::storage::lake::free_bytes_at(archives).unwrap_or(u64::MAX);
        if free >= MIN_FREE_BYTES * 4 {
            break; // comfortable; keep the history
        }
        if std::fs::remove_file(path).is_ok() {
            pruned += 1;
        }
    }
    Ok(pruned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("virtues-vol-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write(p: &Path, s: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, s).unwrap();
    }

    fn count_increments(archives: &Path) -> usize {
        std::fs::read_dir(archives)
            .map(|d| {
                d.flatten()
                    .filter(|e| e.file_name().to_string_lossy().starts_with("lake-"))
                    .count()
            })
            .unwrap_or(0)
    }

    async fn seed_volume(pool: &sqlx::PgPool, id: &str) -> Volume {
        sqlx::query(
            "INSERT INTO storage_volume (id, name, kind, fs_uuid, prefix) \
             VALUES ($1, 'Test Drive', 'removable', $2, 'virtues/box')",
        )
        .bind(id)
        .bind(format!("uuid-{id}"))
        .execute(pool)
        .await
        .unwrap();
        Volume {
            id: id.into(),
            name: "Test Drive".into(),
            kind: "removable".into(),
            roles: vec![crate::storage::volumes::ROLE_BACKUP.into()],
            fs_uuid: format!("uuid-{id}"),
            mount_path: None,
            prefix: "virtues/box".into(),
            state: "present".into(),
            last_ok_at: None,
            last_error: None,
        }
    }

    #[sqlx::test]
    async fn increments_carry_only_what_is_new_and_heal_when_one_vanishes(pool: sqlx::PgPool) {
        let base = std::env::var("DATABASE_URL").expect("DATABASE_URL");
        let db = format!(
            "{}/{}",
            base.split('?').next().unwrap().rsplit_once('/').unwrap().0,
            pool.connect_options().get_database().unwrap()
        );

        let state = scratch("state");
        let drive = scratch("drive");
        let lake = state.join("lake");
        let env_file = state.join("virtues.env");
        write(&env_file, "VIRTUES_ENCRYPTION_KEY=k\n");
        write(&lake.join("streams/a.jsonl"), "a");
        write(&lake.join("streams/b.jsonl"), "b");

        let sources = Sources {
            database_url: db,
            lake: lake.clone(),
            applets: state.join("applets"),
            env_file: Some(env_file),
        };
        let identity = age::x25519::Identity::generate();
        let recipient = identity.to_public();
        let volume = seed_volume(&pool, "vol_test").await;
        let archives = drive.join("archives");
        let t0 = chrono::DateTime::parse_from_rfc3339("2026-07-25T03:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        // Run 1: both files are new.
        let r1 = run_at(&pool, &volume, &drive, &sources, &recipient, t0)
            .await
            .unwrap();
        assert_eq!(r1.new_files, 2);
        assert!(r1.increment.is_some());
        assert!(r1.full.exists());

        // Run 2: nothing changed, so no increment at all — the point of the
        // whole design is that a quiet day costs nothing.
        let r2 = run_at(&pool, &volume, &drive, &sources, &recipient, t0 + chrono::Duration::days(1))
            .await
            .unwrap();
        assert_eq!(r2.new_files, 0);
        assert!(r2.increment.is_none(), "an empty increment was written");

        // Run 3: one new file, and ONLY that file.
        write(&lake.join("streams/c.jsonl"), "c");
        let r3 = run_at(&pool, &volume, &drive, &sources, &recipient, t0 + chrono::Duration::days(2))
            .await
            .unwrap();
        assert_eq!(r3.new_files, 1, "an unchanged file was re-sent");
        assert_eq!(count_increments(&archives), 2);

        // Run 4: the first increment disappears — wiped drive, swapped drive,
        // operator tidying up. Its files have no other copy, so the box must
        // notice and re-send them rather than believe its own bookkeeping.
        let first = std::fs::read_dir(&archives)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("lake-"))
            .min_by_key(|p| p.metadata().unwrap().modified().unwrap())
            .unwrap();
        std::fs::remove_file(&first).unwrap();

        let r4 = run_at(&pool, &volume, &drive, &sources, &recipient, t0 + chrono::Duration::days(3))
            .await
            .unwrap();
        assert_eq!(
            r4.new_files, 2,
            "reconciliation did not re-send files whose increment was gone"
        );

        let _ = std::fs::remove_dir_all(&state);
        let _ = std::fs::remove_dir_all(&drive);
    }
}

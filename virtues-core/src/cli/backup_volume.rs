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

#[derive(Debug)]
pub(crate) struct VolumeBackup {
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
pub(crate) async fn run(
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
    verify_volume_marker(pool, volume, root).await?;

    reconcile(pool, volume, &archives).await?;

    let stamp = now.format("%Y%m%dT%H%M%SZ").to_string();
    let pending = pending_files(pool, volume, &sources.lake).await?;
    let new_bytes: u64 = pending.iter().map(|(_, _, size)| size).sum();

    // Prune BEFORE writing, not after. Pruning afterwards can never free space
    // for the write that needed it: a run that fails with ENOSPC returns early
    // and never reaches the cleanup that would have made room, so the next run
    // hits the same wall. Safe to do first because `prune_full_archives` always
    // keeps the newest, so the previous good full survives until the new one
    // lands.
    let pruned = prune_full_archives(&archives)?;

    // Budget BOTH artifacts. Counting only the lake increment left the full
    // archive — which carries the entire pg_dump — completely unaccounted, so
    // the guard would wave through a run that then filled the volume.
    ensure_room(&archives, new_bytes + estimate_full_bytes(pool, sources).await)?;

    // The full archive FIRST. It is what makes a restore possible at all: an
    // increment without one is unusable, since nothing describes the lake it
    // holds. Writing it first means a failure here leaves the volume exactly as
    // it was rather than holding lake data that cannot be restored.
    let full = archives.join(format!("full-{stamp}.tar.gz.age"));
    backup::write_archive_to(pool, &full, sources, recipient).await?;

    let mut increment = None;
    if !pending.is_empty() {
        let name = format!("lake-{stamp}.tar.gz.age");
        let dest = archives.join(&name);
        let members: Vec<(PathBuf, String)> = pending
            .iter()
            .map(|(abs, rel, _)| (abs.clone(), rel.clone()))
            .collect();
        // Record what the writer ACTUALLY archived. Files that vanish mid-run
        // are skipped by `stream_archive`; recording the planned list instead
        // would mark them shipped forever and they would never be re-sent.
        let written = backup::write_members(
            &members,
            &dest,
            recipient,
            &format!("lake increment {stamp}"),
        )?;
        record_increment(pool, volume, &name, &pending, &written).await?;
        increment = Some(dest);
    }

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
    written: &[String],
) -> Result<(), crate::Error> {
    let written: std::collections::HashSet<&str> = written.iter().map(String::as_str).collect();
    let kept: Vec<&(PathBuf, String, u64)> = files
        .iter()
        .filter(|(_, rel, _)| written.contains(rel.as_str()))
        .collect();
    let rels: Vec<String> = kept.iter().map(|(_, r, _)| r.clone()).collect();
    let sizes: Vec<i64> = kept.iter().map(|(_, _, s)| *s as i64).collect();
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
        // By NAME, not mtime. Both increments are written seconds apart, so on a
        // filesystem with coarse timestamps their mtimes can tie and `min_by_key`
        // picks arbitrarily — which made this test flaky, deleting the newer
        // increment and changing how many files reconciliation re-sent. Increment
        // names are fixed-width UTC stamps, so lexical order IS chronological;
        // this is the same reason `survey_volume` orders by name in production.
        let mut names: Vec<PathBuf> = std::fs::read_dir(&archives)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("lake-"))
            .collect();
        names.sort();
        let first = names.into_iter().next().unwrap();
        std::fs::remove_file(&first).unwrap();

        let r4 = run_at(&pool, &volume, &drive, &sources, &recipient, t0 + chrono::Duration::days(3))
            .await
            .unwrap();
        assert_eq!(
            r4.new_files, 2,
            "reconciliation did not re-send files whose increment was gone"
        );

        // ── The half that was missing: can any of this be restored? ──────
        // Files are spread across three increments plus whatever the full
        // archive carries, so a restore that only reads one artifact would
        // come back short and look like it worked.
        std::fs::remove_dir_all(&lake).unwrap();

        crate::cli::restore::apply_from_volume(
            &drive,
            &crate::cli::restore::Targets {
                database_url: sources.database_url.clone(),
                lake: lake.clone(),
                applets: state.join("applets"),
                env_file: state.join("virtues.env"),
            },
            Some(&identity),
        )
        .expect("restore from volume");

        for name in ["a", "b", "c"] {
            let f = lake.join(format!("streams/{name}.jsonl"));
            assert_eq!(
                std::fs::read_to_string(&f).unwrap_or_default(),
                name,
                "{name} did not come back — an increment was not replayed"
            );
        }

        let _ = std::fs::remove_dir_all(&state);
        let _ = std::fs::remove_dir_all(&drive);
    }

    /// Regression: a volume restore must REPLACE the lake, not merge into it.
    /// A volume `full-*` carries no lake/ member, so `apply` never reaches its
    /// own clearing branch — the volume path has to clear it explicitly. Without
    /// that, files belonging to no archive survived and were indistinguishable
    /// from restored ones.
    #[sqlx::test]
    async fn volume_restore_replaces_the_lake_rather_than_merging(pool: sqlx::PgPool) {
        let base = std::env::var("DATABASE_URL").expect("DATABASE_URL");
        let db = format!(
            "{}/{}",
            base.split('?').next().unwrap().rsplit_once('/').unwrap().0,
            pool.connect_options().get_database().unwrap()
        );
        let state = scratch("replace-state");
        let drive = scratch("replace-drive");
        let lake = state.join("lake");
        let env_file = state.join("virtues.env");
        write(&env_file, "K=1\n");
        write(&lake.join("streams/archived.jsonl"), "archived");

        let sources = Sources {
            database_url: db.clone(),
            lake: lake.clone(),
            applets: state.join("applets"),
            env_file: Some(env_file.clone()),
        };
        let identity = age::x25519::Identity::generate();
        let volume = seed_volume(&pool, "vol_replace").await;
        let t0 = chrono::DateTime::parse_from_rfc3339("2026-07-25T03:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        run_at(&pool, &volume, &drive, &sources, &identity.to_public(), t0)
            .await
            .unwrap();

        // Present on the box, in no archive on the drive.
        write(&lake.join("streams/never_archived.jsonl"), "stale");

        crate::cli::restore::apply_from_volume(
            &drive,
            &crate::cli::restore::Targets {
                database_url: db,
                lake: lake.clone(),
                applets: state.join("applets"),
                env_file,
            },
            Some(&identity),
        )
        .unwrap();

        assert!(
            lake.join("streams/archived.jsonl").exists(),
            "the archived file did not come back"
        );
        assert!(
            !lake.join("streams/never_archived.jsonl").exists(),
            "restore merged: a file belonging to no archive survived"
        );
        let _ = std::fs::remove_dir_all(&state);
        let _ = std::fs::remove_dir_all(&drive);
    }

    /// Regression: resolution happens once, up front. If the drive is pulled
    /// before the write, the mount point is still a directory on the ROOT
    /// filesystem — so the run would recreate the tree there, conclude every
    /// increment had vanished, and write the whole lake onto the box's own disk.
    #[sqlx::test]
    async fn a_vanished_drive_is_refused_not_written_to_the_root_filesystem(
        pool: sqlx::PgPool,
    ) {
        let base = std::env::var("DATABASE_URL").expect("DATABASE_URL");
        let db = format!(
            "{}/{}",
            base.split('?').next().unwrap().rsplit_once('/').unwrap().0,
            pool.connect_options().get_database().unwrap()
        );
        let state = scratch("marker-state");
        let drive = scratch("marker-drive");
        let lake = state.join("lake");
        let env_file = state.join("virtues.env");
        write(&env_file, "K=1\n");
        write(&lake.join("streams/a.jsonl"), "a");

        let sources = Sources {
            database_url: db,
            lake: lake.clone(),
            applets: state.join("applets"),
            env_file: Some(env_file),
        };
        let identity = age::x25519::Identity::generate();
        let volume = seed_volume(&pool, "vol_marker").await;
        let t0 = chrono::DateTime::parse_from_rfc3339("2026-07-25T03:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        run_at(&pool, &volume, &drive, &sources, &identity.to_public(), t0)
            .await
            .expect("first backup");
        assert!(drive.join(MARKER).exists(), "no marker written on first use");

        // The drive is gone; what remains is a bare mount point.
        std::fs::remove_dir_all(&drive).unwrap();
        std::fs::create_dir_all(&drive).unwrap();

        let err = run_at(
            &pool,
            &volume,
            &drive,
            &sources,
            &identity.to_public(),
            t0 + chrono::Duration::days(1),
        )
        .await
        .expect_err("writing to a vanished drive must be refused");
        assert!(
            format!("{err}").contains("probably not mounted"),
            "unexpected error: {err}"
        );
        // And nothing was written to the bare directory.
        assert!(!drive.join("archives").join("full-20260726T030000Z.tar.gz.age").exists());

        let _ = std::fs::remove_dir_all(&state);
        let _ = std::fs::remove_dir_all(&drive);
    }

    /// Regression: `--from-volume` takes a path and must locate the box
    /// directory whether the operator points at it or at the mount point above
    /// it. On replacement hardware they know where they plugged the drive in,
    /// not what the old box called itself.
    #[test]
    fn volume_root_resolves_from_the_mount_point_or_the_box_directory() {
        use crate::cli::restore::resolve_volume_root;
        let mount = scratch("resolve");
        let box_dir = mount.join("virtues/dragon");
        std::fs::create_dir_all(box_dir.join("archives")).unwrap();

        assert_eq!(resolve_volume_root(&box_dir).unwrap(), box_dir);
        assert_eq!(resolve_volume_root(&mount).unwrap(), box_dir);

        // Two boxes sharing a drive must be named, not guessed between.
        let second = mount.join("virtues/other");
        std::fs::create_dir_all(second.join("archives")).unwrap();
        let err = resolve_volume_root(&mount).unwrap_err();
        assert!(format!("{err}").contains("more than one box"), "{err}");

        let empty = scratch("resolve-empty");
        let err = resolve_volume_root(&empty).unwrap_err();
        assert!(format!("{err}").contains("no backups under"), "{err}");

        let _ = std::fs::remove_dir_all(&mount);
        let _ = std::fs::remove_dir_all(&empty);
    }

    #[sqlx::test]
    async fn a_volume_with_no_full_archive_refuses_rather_than_half_restores(
        _pool: sqlx::PgPool,
    ) {
        let drive = scratch("empty-drive");
        std::fs::create_dir_all(drive.join("archives")).unwrap();
        let err = crate::cli::restore::apply_from_volume(
            &drive,
            &crate::cli::restore::Targets {
                database_url: "postgres:///nope".into(),
                lake: drive.join("lake"),
                applets: drive.join("applets"),
                env_file: drive.join("env"),
            },
            None,
        )
        .unwrap_err();
        assert!(
            format!("{err}").contains("no full archive"),
            "unexpected error: {err}"
        );
        let _ = std::fs::remove_dir_all(&drive);
    }
}

/// `virtues backup --volume <id|all>`.
///
/// A volume that is not attached is skipped, never an error. The box has to keep
/// working with its drive unplugged — that asymmetry is the reason removable
/// media is acceptable for backups and not for live storage.
pub async fn run_cli(
    pool: PgPool,
    target: &str,
    allow_missing_key: bool,
) -> Result<(), crate::Error> {
    let sources = Sources::from_env(allow_missing_key)?;
    let recipient = backup::load_or_create_recipient()?;
    let all = crate::storage::volumes::backup_volumes(&pool).await?;
    let selected: Vec<_> = if target == "all" {
        all
    } else {
        all.into_iter().filter(|v| v.id == target).collect()
    };
    if selected.is_empty() {
        return Err(crate::Error::Other(format!(
            "no registered backup volume matches `{target}`. \
             Register one with `virtues volumes add <path>`."
        )));
    }

    let now = chrono::Utc::now();
    let mut any = false;
    for volume in &selected {
        match run(&pool, volume, &sources, &recipient, now).await {
            Ok(None) => {
                super::ui::warn(&format!("{} is not attached — skipped", volume.name));
                // Clear any error from a previous run. "Not attached" is the
                // current truth; leaving a stale error in place pinned the UI to
                // `failing` (which derive_state ranks above everything) for as
                // long as the drive stayed unplugged, and made the nightly
                // applet exit non-zero forever. Staleness is already carried
                // honestly by the backup's age.
                mark_detached(&pool, &volume.id).await;
            }
            Ok(Some(r)) => {
                any = true;
                super::ui::ok(&format!(
                    "{}: {} new lake file(s), {:.1} MB{}",
                    volume.name,
                    r.new_files,
                    r.new_bytes as f64 / (1024.0 * 1024.0),
                    if r.pruned > 0 {
                        format!(", pruned {} old full archive(s)", r.pruned)
                    } else {
                        String::new()
                    }
                ));
                mark_ok(&pool, &volume.id).await;
            }
            Err(e) => {
                // Record and keep going: one failing drive must not stop the
                // others, and the error has to outlive this terminal session
                // to be worth anything.
                mark_error(&pool, &volume.id, &format!("{e}")).await;
                super::ui::warn(&format!("{}: {e}", volume.name));
            }
        }
    }
    if !any {
        super::ui::warn("no volume was written to — nothing is backed up off this box");
    }
    Ok(())
}

async fn mark_ok(pool: &PgPool, id: &str) {
    let _ = sqlx::query(
        "UPDATE storage_volume SET last_ok_at = NOW(), last_error = NULL, \
         last_error_at = NULL, updated_at = NOW() WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await;
}

async fn mark_detached(pool: &PgPool, id: &str) {
    let _ = sqlx::query(
        "UPDATE storage_volume SET state = 'absent', last_error = NULL, \
         last_error_at = NULL, updated_at = NOW() WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await;
}

async fn mark_error(pool: &PgPool, id: &str, err: &str) {
    let _ = sqlx::query(
        "UPDATE storage_volume SET last_error = $2, last_error_at = NOW(), \
         updated_at = NOW() WHERE id = $1",
    )
    .bind(id)
    .bind(err)
    .execute(pool)
    .await;
}

/// Marker file proving this directory is the volume we think it is.
const MARKER: &str = ".virtues-volume";

/// Refuse to write unless the drive is still the one that was resolved.
///
/// Resolution happens once, up front, via `/proc/self/mountinfo`. If the drive
/// is pulled between then and the write, the mount point remains as an ordinary
/// directory on the ROOT filesystem — so `create_dir_all` cheerfully recreates
/// the tree there, `read_dir` reports it empty, reconciliation concludes every
/// increment is gone, and the run proceeds to write the entire lake onto the
/// box's own disk. On a box whose lake is most of its storage, that fills the
/// disk that Postgres and every collector depend on.
///
/// A marker written on first use closes it: if this box has ever backed up here
/// the marker must be present, and its absence means the directory is not the
/// volume — whatever else it may be.
async fn verify_volume_marker(
    pool: &PgPool,
    volume: &Volume,
    root: &Path,
) -> Result<(), crate::Error> {
    let marker = root.join(MARKER);
    match std::fs::read_to_string(&marker) {
        Ok(found) if found.trim() == volume.fs_uuid => return Ok(()),
        Ok(found) => {
            return Err(crate::Error::Other(format!(
                "{} belongs to volume {} , not {}. Refusing to write: this is \
                 not the drive that was registered.",
                root.display(),
                found.trim(),
                volume.fs_uuid
            )))
        }
        Err(_) => {}
    }

    // No marker. Either this is genuinely the first backup, or the drive is not
    // mounted and we are looking at a bare mount point.
    let shipped: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM backup_archived_file WHERE volume_id = $1",
    )
    .bind(&volume.id)
    .fetch_one(pool)
    .await
    .map_err(|e| crate::Error::Database(format!("check prior backups: {e}")))?;

    if shipped > 0 || volume.last_ok_at.is_some() {
        return Err(crate::Error::Other(format!(
            "{} has no {MARKER}, but this box has backed up to {} before. The \
             drive is probably not mounted — writing here would fill the box's \
             own disk instead. Check the mount and retry.",
            root.display(),
            volume.name
        )));
    }
    std::fs::write(&marker, format!("{}\n", volume.fs_uuid))
        .map_err(|e| crate::Error::Other(format!("write {}: {e}", marker.display())))?;
    Ok(())
}

/// Conservative size estimate for the full archive, for the space guard.
///
/// `pg_database_size` plus the applet tree. Both are pre-compression figures and
/// the archive is gzipped, so this over-estimates — which is the correct
/// direction for a guard. Falls back to a fixed allowance rather than zero when
/// the database cannot be measured: a guard that silently becomes a no-op is
/// worse than one that is merely approximate.
async fn estimate_full_bytes(pool: &PgPool, sources: &Sources) -> u64 {
    let db: u64 = sqlx::query_scalar::<_, i64>("SELECT pg_database_size(current_database())")
        .fetch_one(pool)
        .await
        .ok()
        .and_then(|v| u64::try_from(v).ok())
        .unwrap_or(MIN_FREE_BYTES);

    let mut applets = Vec::new();
    let _ = backup::collect_files_pub(&sources.applets, "applets", &mut applets);
    let applet_bytes: u64 = applets
        .iter()
        .filter_map(|(abs, _)| std::fs::metadata(abs).ok().map(|m| m.len()))
        .sum();

    db.saturating_add(applet_bytes)
}

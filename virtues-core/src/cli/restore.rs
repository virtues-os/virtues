//! `virtues restore` — destructively restore the box state from a backup
//! tarball produced by `virtues backup`.
//!
//! Three checks gate the actual restore:
//!
//!   1. The `virtues` systemd service must be inactive (unless `--force`).
//!   2. The manifest's schema migration version must NOT be newer than the
//!      current binary's. We never restore-into-older-schema — that's an
//!      unfixable data loss surface. The user should `virtues upgrade` to
//!      the matching binary first. (Not `--force`-able.)
//!   3. Every artifact's sha256 must match the manifest. (Not `--force`-able.)

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};

use super::backup::{Artifact, Manifest};

/// Where to put the restored env file. Must be the path the running service
/// actually reads, not the FHS-correct one — restoring the key to a file
/// systemd never loads leaves a box that boots fine (the unit's
/// `EnvironmentFile=-` prefix makes a missing file non-fatal) and cannot
/// decrypt anything. Prefer an env file that already exists on this box;
/// fall back to the FHS path for a bare-metal restore onto a fresh machine.
fn restore_env_target() -> PathBuf {
    crate::cli::backup::find_env_file()
        .unwrap_or_else(|| PathBuf::from(crate::cli::backup::ENV_CANDIDATES[0]))
}
/// Directories that only ever exist inside a lake. Presence of any one of them
/// is what distinguishes "this is the lake, replace it" from "the resolver
/// pointed somewhere else entirely".
const LAKE_MARKERS: &[&str] = &["streams", "media", ".media", ".uploads"];

/// Refuse to delete a directory that does not look like a lake.
///
/// Restore replaces the lake wholesale, and the path now comes from
/// configuration rather than a constant. Configuration is precisely the thing
/// that can be wrong: on a box where `STORAGE_PATH` was set to a home directory
/// or a mount root, an unguarded `remove_dir_all` would destroy it — during the
/// one command an operator runs *to recover data*.
///
/// Empty or absent is fine; we are about to create it. Non-empty with no marker
/// is refused. A lake that has only ever held Drive files and no ingested
/// streams would be refused too — the cost of that false positive is a message
/// telling the operator to check `STORAGE_PATH`, which is survivable in a way
/// that deleting the wrong tree is not.
fn assert_replaceable_lake(lake: &Path) -> Result<(), crate::Error> {
    let Ok(entries) = fs::read_dir(lake) else {
        return Ok(()); // absent — restore creates it
    };
    let mut names = Vec::new();
    for entry in entries.flatten() {
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    if names.is_empty() || names.iter().any(|n| LAKE_MARKERS.contains(&n.as_str())) {
        return Ok(());
    }
    Err(crate::Error::Other(format!(
        "refusing to replace {}: it is not empty and contains none of {:?}, so it \
         does not look like a data lake. Check STORAGE_PATH — restore replaces \
         this directory wholesale.",
        lake.display(),
        LAKE_MARKERS
    )))
}

/// Where a restore writes. The mirror of `backup::Sources`, and testable for the
/// same reason: the drill has to be able to aim a real restore at scratch paths
/// and a scratch database instead of at the box it is running on.
pub(crate) struct Targets {
    pub database_url: String,
    pub lake: PathBuf,
    pub applets: PathBuf,
    pub env_file: PathBuf,
}

impl Targets {
    pub(crate) fn from_env() -> Result<Self, crate::Error> {
        Ok(Self {
            database_url: crate::database::normalize_database_url()
                .map_err(|e| crate::Error::Other(format!("DATABASE_URL: {e}")))?,
            lake: crate::storage::lake::lake_root(),
            applets: crate::action_templates::state_root(),
            env_file: restore_env_target(),
        })
    }
}

pub async fn run(path: PathBuf, force: bool) -> Result<(), crate::Error> {
    if !path.exists() {
        return Err(crate::Error::Other(format!(
            "{} not found",
            path.display()
        )));
    }

    if !force {
        check_service_inactive()?;
    }

    // Extract into a staging dir we can inspect.
    let stage = mkstage(&path)?;
    let staging: &Path = stage.as_ref();
    println!("→ extracting {}…", path.display());
    extract_into(&path, staging)?;

    let manifest = read_manifest(staging)?;
    println!(
        "→ manifest: binary {}, schema {}, created {}",
        manifest.binary_version, manifest.schema_version, manifest.created_at
    );

    check_schema_compatible(&manifest)?;
    verify_sha256(staging, &manifest.artifacts)?;

    // We've validated everything. Past this line is destructive.
    println!();
    println!("⚠  About to overwrite live box state. Press Ctrl-C in 5s to abort.");
    std::thread::sleep(std::time::Duration::from_secs(5));

    let targets = Targets::from_env()?;
    apply(staging, &targets)?;

    // Everything above was written by root (restore requires it — it stops the
    // unit, writes the env file, and drives pg_restore). `copy_tree` is
    // create_dir_all + fs::copy, neither of which preserves ownership, so the
    // restored trees land root-owned while the service runs as `virtues`. Left
    // that way, the box comes back up unable to write its own state: applet
    // authoring fails with `mkdir failed: Permission denied` and lake writes
    // fail the same way. Hand ownership back before declaring success.
    //
    // Runs last, and never fatally: aborting here would leave a fully restored
    // box refusing to finish over a fixable permission, so a failure prints the
    // exact command instead.
    for dir in [targets.lake.as_path(), targets.applets.as_path()] {
        if dir.is_dir() {
            give_to_service_user(dir);
        }
    }

    print_next_steps();
    Ok(())
}

/// The destructive half: everything that writes box state, and nothing else.
///
/// Split out from `run` so the round-trip drill can exercise the real code
/// rather than a reimplementation of it. `run` keeps the parts a test must not
/// perform — the service check, the abort window, and the ownership handback.
pub(crate) fn apply(staging: &Path, targets: &Targets) -> Result<(), crate::Error> {
    println!("→ restoring data lake at {}…", targets.lake.display());
    let staged_lake = staging.join("lake");
    if staged_lake.exists() {
        assert_replaceable_lake(&targets.lake)?;
        let _ = fs::remove_dir_all(&targets.lake);
        fs::create_dir_all(&targets.lake)
            .map_err(|e| crate::Error::Other(format!("create lake dir: {e}")))?;
        copy_tree(&staged_lake, &targets.lake)?;
    }

    // Authored applets. Replace wholesale like the lake: the tarball is the
    // intended state, and a merge would resurrect applets the user deleted.
    let staged_applets = staging.join("applets");
    if staged_applets.is_dir() {
        println!(
            "→ restoring authored applets at {}…",
            targets.applets.display()
        );
        let _ = fs::remove_dir_all(&targets.applets);
        fs::create_dir_all(&targets.applets)
            .map_err(|e| crate::Error::Other(format!("create applets dir: {e}")))?;
        copy_tree(&staged_applets, &targets.applets)?;
    }

    let staged_env = staging.join("env/virtues.env");
    if staged_env.exists() {
        println!("→ restoring env at {}…", targets.env_file.display());
        if let Some(parent) = targets.env_file.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| crate::Error::Other(format!("env parent: {e}")))?;
        }
        fs::copy(&staged_env, &targets.env_file)
            .map_err(|e| crate::Error::Other(format!("write env: {e}")))?;
        // Lock the env down — encryption key inside.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&targets.env_file, fs::Permissions::from_mode(0o600));
        }
    } else {
        // The tarball carries no key. Everything encrypted in the dump we are
        // about to load is unreadable; say so plainly rather than reporting a
        // clean restore.
        eprintln!(
            "warning: backup contains no env file — restored credentials will \
             NOT be decryptable without VIRTUES_ENCRYPTION_KEY from the source box"
        );
    }

    println!("→ restoring Postgres database…");
    let dump = staging.join("db/virtues.dump");
    if !dump.exists() {
        return Err(crate::Error::Other(
            "manifest declared no db dump — refusing to leave you with empty data".to_string(),
        ));
    }
    pg_restore(&dump, &targets.database_url)?;
    Ok(())
}

fn print_next_steps() {

    // Everything above was written by root (restore requires it — it stops the
    // unit, writes the env file, and drives pg_restore). `copy_tree` is
    // create_dir_all + fs::copy, neither of which preserves ownership, so the
    // restored trees land root-owned while the service runs as `virtues`. Left
    // that way, the box comes back up unable to write its own state: applet
    // authoring fails with `mkdir failed: Permission denied` and lake writes
    // fail the same way. Hand ownership back before declaring success.
    //
    // Runs last, and never fatally: aborting here would leave a fully restored
    // box refusing to finish over a fixable permission, so a failure prints the
    // exact command instead.
    println!();
    println!("✓ restore complete.");
    println!("  Next steps:");
    println!("    sudo systemctl start virtues");
    println!("    sudo -u virtues virtues pair");
}

fn check_service_inactive() -> Result<(), crate::Error> {
    // `systemctl is-active virtues` exits 0 if active, 3 (or other) otherwise.
    // If `systemctl` itself is missing (e.g. dev macOS), assume inactive.
    let out = Command::new("systemctl").arg("is-active").arg("virtues").output();
    match out {
        Ok(o) if o.status.success() => Err(crate::Error::Other(
            "virtues.service is running. Stop it first (`sudo systemctl stop virtues`) \
             or re-run with --force."
                .to_string(),
        )),
        _ => Ok(()),
    }
}

fn read_manifest(staging: &Path) -> Result<Manifest, crate::Error> {
    let raw = fs::read(staging.join("manifest.json"))
        .map_err(|e| crate::Error::Other(format!("read manifest: {e}")))?;
    serde_json::from_slice::<Manifest>(&raw)
        .map_err(|e| crate::Error::Other(format!("parse manifest: {e}")))
}

fn check_schema_compatible(manifest: &Manifest) -> Result<(), crate::Error> {
    // The manifest's `schema_version` is the largest sqlx migration ID
    // applied to the backup's DB. Compare against the migrations we ship.
    // If the backup's schema is newer, refuse — restoring would leave the
    // user on a binary that can't read its own DB.
    let backup_v: u64 = manifest
        .schema_version
        .parse()
        .map_err(|_| crate::Error::Other("manifest schema_version not numeric".to_string()))?;
    let current_v = current_migration_max().unwrap_or(0);
    if backup_v > current_v {
        return Err(crate::Error::Other(format!(
            "backup schema (v{backup_v}) is newer than this binary's schema (v{current_v}). \
             Upgrade the binary first: `sudo virtues upgrade`."
        )));
    }
    Ok(())
}

/// The largest migration id this binary ships — read from the EMBEDDED
/// migration set, so it can never drift from reality. (This replaced a
/// hand-bumped `KNOWN_MAX_MIGRATION` constant that was still at 9 while the
/// real set was at 49 — the exact silent-drift failure a derived value
/// cannot have. Works cold: the embedded set needs no DB connection.)
fn current_migration_max() -> Option<u64> {
    crate::database::embedded_migration_max().map(|v| v as u64)
}

fn verify_sha256(staging: &Path, artifacts: &[Artifact]) -> Result<(), crate::Error> {
    println!("→ verifying sha256 of {} artifact(s)…", artifacts.len());
    for art in artifacts {
        let abs = staging.join(&art.path);
        let bytes = fs::read(&abs).map_err(|e| {
            crate::Error::Other(format!("read artifact {}: {e}", art.path))
        })?;
        if bytes.len() as u64 != art.size_bytes {
            return Err(crate::Error::Other(format!(
                "{} size mismatch (manifest {}, actual {})",
                art.path,
                art.size_bytes,
                bytes.len()
            )));
        }
        let mut h = Sha256::new();
        h.update(&bytes);
        let got = format!("{:x}", h.finalize());
        if got != art.sha256 {
            return Err(crate::Error::Other(format!(
                "{} sha256 mismatch — backup is corrupt or modified",
                art.path
            )));
        }
    }
    Ok(())
}

fn pg_restore(dump: &Path, database_url: &str) -> Result<(), crate::Error> {
    // Drop + recreate the schema cleanly. `--clean --if-exists` makes the
    // pg_restore drop all existing objects before recreating. `--no-owner`
    // + `--no-acl` skip privilege replays that would fail in a peer-auth
    // setup.
    let status = Command::new("pg_restore")
        .arg("--clean")
        .arg("--if-exists")
        .arg("--no-owner")
        .arg("--no-acl")
        .arg("-d")
        .arg(database_url)
        .arg(dump)
        .status()
        .map_err(|e| crate::Error::Other(format!("invoke pg_restore: {e}")))?;
    if !status.success() {
        return Err(crate::Error::Other(format!(
            "pg_restore exited {status}; backup not fully restored. \
             Service should remain stopped until the cause is fixed."
        )));
    }
    Ok(())
}

// ─── Tar extraction + copy helpers ─────────────────────────────────────────

fn extract_into(archive: &Path, dest: &Path) -> Result<(), crate::Error> {
    let file = File::open(archive)
        .map_err(|e| crate::Error::Other(format!("open {}: {e}", archive.display())))?;
    let gz = GzDecoder::new(file);
    let mut t = tar::Archive::new(gz);
    t.unpack(dest)
        .map_err(|e| crate::Error::Other(format!("tar unpack: {e}")))?;
    Ok(())
}

/// `chown -R virtues:virtues <dir>`. Shells out to match how the installer
/// does the same job; a pure-Rust walk would need the uid/gid lookup anyway.
fn give_to_service_user(dir: &Path) {
    let out = Command::new("chown")
        .args(["-R", "virtues:virtues"])
        .arg(dir)
        .output();
    match out {
        Ok(o) if o.status.success() => {
            println!("→ handed {} back to the virtues user", dir.display());
        }
        Ok(o) => eprintln!(
            "warning: could not chown {} ({}). The service runs as `virtues` and \
             will not be able to write there. Fix with:\n    sudo chown -R virtues:virtues {}",
            dir.display(),
            String::from_utf8_lossy(&o.stderr).trim(),
            dir.display()
        ),
        Err(e) => eprintln!(
            "warning: could not run chown on {dir:?} ({e}). Fix with:\n    \
             sudo chown -R virtues:virtues {}",
            dir.display()
        ),
    }
}

fn copy_tree(src: &Path, dst: &Path) -> Result<(), crate::Error> {
    fs::create_dir_all(dst)
        .map_err(|e| crate::Error::Other(format!("mkdir {}: {e}", dst.display())))?;
    for entry in fs::read_dir(src)
        .map_err(|e| crate::Error::Other(format!("read {}: {e}", src.display())))?
    {
        let entry = entry.map_err(|e| crate::Error::Other(format!("dir entry: {e}")))?;
        let path = entry.path();
        let dest = dst.join(entry.file_name());
        if path.is_dir() {
            copy_tree(&path, &dest)?;
        } else {
            fs::copy(&path, &dest)
                .map_err(|e| crate::Error::Other(format!("copy {}: {e}", path.display())))?;
        }
    }
    Ok(())
}

struct Stage(PathBuf);
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
impl AsRef<Path> for Stage {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

fn mkstage(near: &Path) -> Result<Stage, crate::Error> {
    let base = near
        .parent()
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| Path::new("/tmp"))
        .to_path_buf();
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%f");
    let path = base.join(format!(".virtues-restore-stage-{stamp}"));
    fs::create_dir_all(&path)
        .map_err(|e| crate::Error::Other(format!("staging dir: {e}")))?;
    Ok(Stage(path))
}

// Tiny helper: read into Vec; not currently used, but kept for future
// in-memory comparison paths.
#[allow(dead_code)]
fn read_all(path: &Path) -> Result<Vec<u8>, crate::Error> {
    let mut f = File::open(path)
        .map_err(|e| crate::Error::Other(format!("open {}: {e}", path.display())))?;
    let mut buf = vec![];
    f.read_to_end(&mut buf)
        .map_err(|e| crate::Error::Other(format!("read {}: {e}", path.display())))?;
    Ok(buf)
}

/// The round-trip drill.
///
/// Backup and restore had no test coverage of any kind: no unit tests, and no
/// integration test referencing either command. That is the worst possible
/// place to have none, because the failure is silent by construction — a broken
/// backup reports success, and you learn otherwise at restore, which is the one
/// moment you cannot afford to.
///
/// So this exercises the real functions, not a reimplementation: seed a box,
/// `backup::write_archive`, destroy the state, run the actual manifest gates,
/// `apply`, and assert the bytes came back.
///
/// Requires a live Postgres (`#[sqlx::test]` provisions a scratch DB and applies
/// `migrations/`; set `DATABASE_URL`) and `pg_dump`/`pg_restore` on PATH. Missing
/// tools panic rather than skip: a drill that quietly passes when it did not run
/// is the same false comfort this test exists to remove.
#[cfg(test)]
mod drill {
    use super::*;
    use std::process::Command as Cmd;

    /// Rebuild a URL for the scratch database `#[sqlx::test]` made for us.
    ///
    /// `pg_dump` and `pg_restore` are separate processes, so they need a URL
    /// rather than the pool. Swapping the database component of `DATABASE_URL`
    /// keeps whatever credentials the test environment uses, which reconstructing
    /// from `PgConnectOptions` would drop (it does not expose the password).
    fn scratch_url(pool: &sqlx::PgPool) -> String {
        let base =
            std::env::var("DATABASE_URL").expect("DATABASE_URL must be set to run the drill");
        let base = base.split('?').next().unwrap().to_string();
        let db = pool
            .connect_options()
            .get_database()
            .expect("scratch database name")
            .to_string();
        let (prefix, _) = base
            .rsplit_once('/')
            .expect("DATABASE_URL with a database path");
        format!("{prefix}/{db}")
    }

    fn require_pg_tools() {
        for tool in ["pg_dump", "pg_restore"] {
            let ok = Cmd::new(tool).arg("--version").output().is_ok();
            assert!(ok, "{tool} must be on PATH to run the backup drill");
        }
    }

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("virtues-drill-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    fn write(path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    #[sqlx::test]
    async fn round_trip_restores_database_lake_applets_and_env(pool: sqlx::PgPool) {
        require_pg_tools();
        let url = scratch_url(&pool);
        let root = scratch_dir("state");

        // ── A box with something worth losing ────────────────────────────────
        let lake = root.join("lake");
        let applets = root.join("applets");
        let env_file = root.join("virtues.env");
        // `streams/` is also what `assert_replaceable_lake` looks for, so this
        // doubles as coverage that a real lake passes the guard.
        write(&lake.join("streams/ios/records_deadbeef.jsonl"), "{\"v\":1}\n");
        write(&applets.join("user/drill/manifest.json"), "{\"name\":\"drill\"}");
        write(&env_file, "VIRTUES_ENCRYPTION_KEY=drill-key\n");
        sqlx::query(
            "INSERT INTO credentials (id, source_id, name, status, secrets_ciphertext) \
             VALUES ($1, 'ios', 'drill', 'active', 'x')",
        )
        .bind("cred_drill")
        .execute(&pool)
        .await
        .expect("seed credential");

        // ── Back it up ───────────────────────────────────────────────────────
        let sources = crate::cli::backup::Sources {
            database_url: url.clone(),
            lake: lake.clone(),
            applets: applets.clone(),
            env_file: Some(env_file.clone()),
        };
        let archive = scratch_dir("out").join("backup.tar.gz");
        crate::cli::backup::write_archive(&pool, Some(archive.clone()), false, &sources)
            .await
            .expect("backup");
        assert!(archive.exists(), "archive was not written");

        // ── Lose it ──────────────────────────────────────────────────────────
        sqlx::query("DELETE FROM credentials WHERE id = 'cred_drill'")
            .execute(&pool)
            .await
            .unwrap();
        fs::remove_dir_all(&lake).unwrap();
        fs::remove_dir_all(&applets).unwrap();
        fs::remove_file(&env_file).unwrap();

        // ── Restore, through the real gates ──────────────────────────────────
        let stage = mkstage(&archive).expect("stage");
        let staging: &Path = stage.as_ref();
        extract_into(&archive, staging).expect("extract");
        let manifest = read_manifest(staging).expect("manifest");
        check_schema_compatible(&manifest).expect("schema gate");
        verify_sha256(staging, &manifest.artifacts).expect("digest gate");
        assert!(
            !manifest.artifacts.is_empty(),
            "manifest recorded no artifacts, so the digest gate proved nothing"
        );

        apply(
            staging,
            &Targets {
                database_url: url,
                lake: lake.clone(),
                applets: applets.clone(),
                env_file: env_file.clone(),
            },
        )
        .expect("restore");

        // ── Assert it came back ──────────────────────────────────────────────
        let (name,): (String,) = sqlx::query_as("SELECT name FROM credentials WHERE id = $1")
            .bind("cred_drill")
            .fetch_one(&pool)
            .await
            .expect("credential row did not survive the round trip");
        assert_eq!(name, "drill");

        assert_eq!(
            fs::read_to_string(lake.join("streams/ios/records_deadbeef.jsonl")).unwrap(),
            "{\"v\":1}\n",
            "lake bytes differ after restore"
        );
        assert_eq!(
            fs::read_to_string(applets.join("user/drill/manifest.json")).unwrap(),
            "{\"name\":\"drill\"}",
            "applet state differs after restore"
        );
        assert!(
            fs::read_to_string(&env_file).unwrap().contains("drill-key"),
            "env file (and therefore the encryption key) did not survive"
        );

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(archive.parent().unwrap());
    }

    #[test]
    fn refuses_to_replace_a_directory_that_is_not_a_lake() {
        // The guard that stands between a mis-set STORAGE_PATH and someone's
        // home directory.
        let dir = scratch_dir("not-a-lake");
        write(&dir.join("thesis.pdf"), "important");
        assert!(assert_replaceable_lake(&dir).is_err());

        let lake = scratch_dir("real-lake");
        write(&lake.join("streams/ios/x.jsonl"), "{}");
        assert!(assert_replaceable_lake(&lake).is_ok());

        let empty = scratch_dir("empty");
        assert!(assert_replaceable_lake(&empty).is_ok(), "an empty lake is replaceable");
        assert!(
            assert_replaceable_lake(&empty.join("absent")).is_ok(),
            "an absent lake is created, not refused"
        );

        for d in [dir, lake, empty] {
            let _ = fs::remove_dir_all(d);
        }
    }
}

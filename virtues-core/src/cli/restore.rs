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
const LAKE_DIR: &str = "/var/lib/virtues/lake";

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

    println!("→ restoring data lake at {LAKE_DIR}…");
    let staged_lake = staging.join("lake");
    if staged_lake.exists() {
        let _ = fs::remove_dir_all(LAKE_DIR);
        fs::create_dir_all(LAKE_DIR)
            .map_err(|e| crate::Error::Other(format!("create lake dir: {e}")))?;
        copy_tree(&staged_lake, Path::new(LAKE_DIR))?;
    }

    // Authored applets. Replace wholesale like the lake: the tarball is the
    // intended state, and a merge would resurrect applets the user deleted.
    let staged_applets = staging.join("applets");
    if staged_applets.is_dir() {
        let applets_dst = crate::action_templates::state_root();
        println!("→ restoring authored applets at {}…", applets_dst.display());
        let _ = fs::remove_dir_all(&applets_dst);
        fs::create_dir_all(&applets_dst)
            .map_err(|e| crate::Error::Other(format!("create applets dir: {e}")))?;
        copy_tree(&staged_applets, &applets_dst)?;
    }

    let staged_env = staging.join("env/virtues.env");
    if staged_env.exists() {
        let env_file = restore_env_target();
        println!("→ restoring env at {}…", env_file.display());
        if let Some(parent) = env_file.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| crate::Error::Other(format!("env parent: {e}")))?;
        }
        fs::copy(&staged_env, &env_file)
            .map_err(|e| crate::Error::Other(format!("write env: {e}")))?;
        // Lock the env down — encryption key inside.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(env_file, fs::Permissions::from_mode(0o600));
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
    pg_restore(&dump)?;

    println!();
    println!("✓ restore complete.");
    println!("  Next steps:");
    println!("    sudo systemctl start virtues");
    println!("    sudo -u virtues virtues pair");
    Ok(())
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

fn pg_restore(dump: &Path) -> Result<(), crate::Error> {
    let database_url = crate::database::normalize_database_url()
        .map_err(|e| crate::Error::Other(format!("DATABASE_URL: {e}")))?;
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
        .arg(&database_url)
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

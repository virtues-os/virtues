//! `virtues backup` — produce a single tarball of the box state.
//!
//! Layout inside the tarball:
//!
//!   manifest.json
//!   db/virtues.dump          (pg_dump --format=custom output)
//!   env/virtues.env          (a copy of /etc/virtues/env)
//!   lake/<file...>           (rsync-style copy of the data lake)
//!
//! Manifest records the binary version, the schema migration version, distro
//! info, UTC timestamp, and sha256 of every artifact. `virtues restore`
//! verifies the manifest before touching anything live.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::Utc;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

const DEFAULT_BACKUP_DIR: &str = "/var/lib/virtues/backups";
const ENV_FILE: &str = "/etc/virtues/env";
const LAKE_DIR: &str = "/var/lib/virtues/lake";
const MANIFEST_VERSION: u32 = 1;

/// Public manifest persisted at the root of the tarball. Format is stable —
/// `virtues restore` of older binaries must parse it. Bump `manifest_version`
/// + add fields with serde defaults if the shape evolves.
#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub manifest_version: u32,
    pub binary_version: String,
    pub schema_version: String,
    pub created_at: String,
    pub distro: Option<String>,
    pub artifacts: Vec<Artifact>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Artifact {
    pub path: String,
    pub size_bytes: u64,
    pub sha256: String,
}

pub async fn run(
    pool: &PgPool,
    output: Option<PathBuf>,
    force: bool,
) -> Result<PathBuf, crate::Error> {
    let now = Utc::now();
    let out_path = match output {
        Some(p) => p,
        None => {
            let stamp = now.format("%Y%m%dT%H%M%SZ");
            let dir = Path::new(DEFAULT_BACKUP_DIR);
            fs::create_dir_all(dir)
                .map_err(|e| crate::Error::Other(format!("creating backup dir: {e}")))?;
            dir.join(format!("virtues-{stamp}.tar.gz"))
        }
    };
    if out_path.exists() && !force {
        return Err(crate::Error::Other(format!(
            "{} already exists; pass --force to overwrite",
            out_path.display()
        )));
    }

    // Stage into a temp directory we can inspect + checksum before tarring.
    // Live writes happen inside this dir; nothing touches the final path
    // until the tar is closed cleanly.
    let staging = tempfile_dir(&out_path)?;
    let staging_path = staging.path();
    fs::create_dir_all(staging_path.join("db"))
        .and_then(|_| fs::create_dir_all(staging_path.join("env")))
        .and_then(|_| fs::create_dir_all(staging_path.join("lake")))
        .map_err(|e| crate::Error::Other(format!("staging dirs: {e}")))?;

    println!("→ pg_dump (full database)…");
    pg_dump_into(staging_path.join("db/virtues.dump").as_path())?;

    println!("→ copying /etc/virtues/env…");
    if Path::new(ENV_FILE).exists() {
        fs::copy(ENV_FILE, staging_path.join("env/virtues.env"))
            .map_err(|e| crate::Error::Other(format!("copying env: {e}")))?;
    } else {
        // Non-fatal in dev (env may live in repo .env). Restore will warn
        // if it sees no env file and the DB has encrypted credentials.
        tracing::warn!("no {ENV_FILE} found; backup will not include encryption key");
    }

    println!("→ copying data lake at {LAKE_DIR}…");
    let lake_src = Path::new(LAKE_DIR);
    if lake_src.exists() {
        copy_tree_recursive(lake_src, &staging_path.join("lake"))?;
    }

    println!("→ building manifest…");
    let schema_version = current_schema_version(pool).await?;
    let mut artifacts = vec![];
    for rel in [
        "db/virtues.dump",
        "env/virtues.env",
    ] {
        let abs = staging_path.join(rel);
        if abs.exists() {
            artifacts.push(artifact_for(&abs, rel)?);
        }
    }
    // Lake is many files; record one entry per file so restore can verify
    // each one individually. Skip if the lake is empty.
    walk_for_artifacts(staging_path.join("lake").as_path(), "lake", &mut artifacts)?;

    let manifest = Manifest {
        manifest_version: MANIFEST_VERSION,
        binary_version: env!("CARGO_PKG_VERSION").to_string(),
        schema_version,
        created_at: now.to_rfc3339(),
        distro: read_distro(),
        artifacts,
    };
    let manifest_json = serde_json::to_vec_pretty(&manifest)
        .map_err(|e| crate::Error::Other(format!("encode manifest: {e}")))?;
    fs::write(staging_path.join("manifest.json"), &manifest_json)
        .map_err(|e| crate::Error::Other(format!("write manifest: {e}")))?;

    println!("→ writing tarball at {}…", out_path.display());
    create_tarball(staging_path, &out_path)?;

    let size = fs::metadata(&out_path)
        .map(|m| m.len())
        .unwrap_or(0);
    println!(
        "✓ backup written: {} ({:.1} MB)",
        out_path.display(),
        size as f64 / (1024.0 * 1024.0)
    );
    Ok(out_path)
}

fn pg_dump_into(dest: &Path) -> Result<(), crate::Error> {
    // `pg_dump --format=custom` produces a binary, compressed, parallel-
    // restorable archive. Connection uses the same DATABASE_URL the daemon
    // uses; on a normally-installed box that's peer-auth as the `virtues`
    // user against the local socket.
    let database_url = crate::database::normalize_database_url()
        .map_err(|e| crate::Error::Other(format!("DATABASE_URL: {e}")))?;
    let status = Command::new("pg_dump")
        .arg("--format=custom")
        .arg("--no-owner")
        .arg("--no-acl")
        .arg("-f")
        .arg(dest)
        .arg(&database_url)
        .status()
        .map_err(|e| crate::Error::Other(format!("invoking pg_dump: {e}")))?;
    if !status.success() {
        return Err(crate::Error::Other(format!(
            "pg_dump exited with status {status}"
        )));
    }
    Ok(())
}

async fn current_schema_version(pool: &PgPool) -> Result<String, crate::Error> {
    // sqlx's migration tracker writes to `_sqlx_migrations`. The largest
    // applied version is our authoritative schema version. If the table
    // doesn't exist yet (shouldn't, given the daemon runs migrations on
    // startup) we report "unknown".
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT version FROM _sqlx_migrations \
         WHERE success = TRUE ORDER BY version DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| crate::Error::Database(format!("read schema version: {e}")))?;
    Ok(row.map(|(v,)| v.to_string()).unwrap_or_else(|| "unknown".to_string()))
}

fn read_distro() -> Option<String> {
    let content = fs::read_to_string("/etc/os-release").ok()?;
    let mut name = None;
    let mut version = None;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("NAME=") {
            name = Some(rest.trim_matches('"').to_string());
        }
        if let Some(rest) = line.strip_prefix("VERSION_ID=") {
            version = Some(rest.trim_matches('"').to_string());
        }
    }
    match (name, version) {
        (Some(n), Some(v)) => Some(format!("{n} {v}")),
        (Some(n), None) => Some(n),
        _ => None,
    }
}

fn artifact_for(abs: &Path, rel: &str) -> Result<Artifact, crate::Error> {
    let bytes = fs::read(abs)
        .map_err(|e| crate::Error::Other(format!("reading {}: {e}", abs.display())))?;
    let size_bytes = bytes.len() as u64;
    let mut h = Sha256::new();
    h.update(&bytes);
    let sha256 = format!("{:x}", h.finalize());
    Ok(Artifact {
        path: rel.to_string(),
        size_bytes,
        sha256,
    })
}

fn walk_for_artifacts(
    abs: &Path,
    rel_prefix: &str,
    out: &mut Vec<Artifact>,
) -> Result<(), crate::Error> {
    if !abs.exists() {
        return Ok(());
    }
    let read = fs::read_dir(abs)
        .map_err(|e| crate::Error::Other(format!("read {}: {e}", abs.display())))?;
    for entry in read {
        let entry =
            entry.map_err(|e| crate::Error::Other(format!("dir entry: {e}")))?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let rel = format!("{rel_prefix}/{name}");
        if path.is_dir() {
            walk_for_artifacts(&path, &rel, out)?;
        } else {
            out.push(artifact_for(&path, &rel)?);
        }
    }
    Ok(())
}

fn copy_tree_recursive(src: &Path, dst: &Path) -> Result<(), crate::Error> {
    fs::create_dir_all(dst)
        .map_err(|e| crate::Error::Other(format!("mkdir {}: {e}", dst.display())))?;
    for entry in fs::read_dir(src)
        .map_err(|e| crate::Error::Other(format!("read {}: {e}", src.display())))?
    {
        let entry = entry.map_err(|e| crate::Error::Other(format!("dir entry: {e}")))?;
        let path = entry.path();
        let dest = dst.join(entry.file_name());
        if path.is_dir() {
            copy_tree_recursive(&path, &dest)?;
        } else {
            fs::copy(&path, &dest)
                .map_err(|e| crate::Error::Other(format!("copy {}: {e}", path.display())))?;
        }
    }
    Ok(())
}

fn create_tarball(staging: &Path, out: &Path) -> Result<(), crate::Error> {
    let tmp = out.with_extension("tar.gz.partial");
    {
        let file = File::create(&tmp)
            .map_err(|e| crate::Error::Other(format!("create {}: {e}", tmp.display())))?;
        let gz = GzEncoder::new(file, Compression::default());
        let mut builder = tar::Builder::new(gz);
        builder
            .append_dir_all(".", staging)
            .map_err(|e| crate::Error::Other(format!("tar append: {e}")))?;
        let mut gz = builder
            .into_inner()
            .map_err(|e| crate::Error::Other(format!("tar finalize: {e}")))?;
        gz.flush()
            .map_err(|e| crate::Error::Other(format!("flush gz: {e}")))?;
        gz.finish()
            .map_err(|e| crate::Error::Other(format!("finish gz: {e}")))?;
    }
    // Atomic rename so a crashed backup never leaves a half-written file
    // at the final path.
    fs::rename(&tmp, out)
        .map_err(|e| crate::Error::Other(format!("rename to {}: {e}", out.display())))?;
    Ok(())
}

// ─── Local staging dir helper (no extra dep) ───────────────────────────────

struct LocalTempDir {
    path: PathBuf,
}
impl Drop for LocalTempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
impl LocalTempDir {
    fn path(&self) -> &Path {
        &self.path
    }
}

fn tempfile_dir(near: &Path) -> Result<LocalTempDir, crate::Error> {
    // Stage in the same directory as the output so the final atomic rename
    // doesn't cross filesystems. Falls back to /tmp if `near`'s parent
    // isn't writable yet (rare).
    let base = near
        .parent()
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| Path::new("/tmp"))
        .to_path_buf();
    let stamp = Utc::now().format("%Y%m%dT%H%M%S%f");
    let path = base.join(format!(".virtues-backup-stage-{stamp}"));
    fs::create_dir_all(&path)
        .map_err(|e| crate::Error::Other(format!("staging dir: {e}")))?;
    Ok(LocalTempDir { path })
}

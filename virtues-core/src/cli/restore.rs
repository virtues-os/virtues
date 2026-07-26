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
use std::io::{Read, Seek};
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

pub async fn run(
    path: Option<PathBuf>,
    force: bool,
    from_volume: Option<PathBuf>,
    key_file: Option<PathBuf>,
) -> Result<(), crate::Error> {
    let identity = key_file.as_deref().map(read_identity).transpose()?;

    if let Some(volume_path) = from_volume {
        if !force {
            check_service_inactive()?;
        }
        return run_from_volume(&volume_path, identity.as_ref()).await;
    }

    let Some(path) = path else {
        return Err(crate::Error::Other(
            "give an archive path, or --from-volume <id> to restore from a              registered drive (`virtues volumes ls`)"
                .to_string(),
        ));
    };
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
    extract_into(open_archive(&path, identity.as_ref())?, staging)?;

    let manifest = read_manifest(staging)?;
    println!(
        "→ manifest: binary {}, schema {}, created {}",
        manifest.binary_version, manifest.schema_version, manifest.created_at
    );

    check_schema_compatible(&manifest)?;
    verify_sha256(staging, &manifest.artifacts)?;
    let targets = Targets::from_env()?;
    preflight_database(&targets.database_url)?;

    // We've validated everything. Past this line is destructive.
    println!();
    println!("⚠  About to overwrite live box state. Press Ctrl-C in 5s to abort.");
    std::thread::sleep(std::time::Duration::from_secs(5));

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

/// Prove the database is reachable before destroying anything.
///
/// `apply` replaces the lake, the applet state and the env file, and only THEN
/// loads the database. A connection failure at that last step therefore leaves
/// a box with restored files and an untouched database — a half-restored state
/// that looks like a completed restore until someone looks closely. It happened
/// on real hardware.
///
/// So prove the connection first, using the same user and URL the restore will
/// actually use. This is the same preflight-then-mutate shape `upgrade` uses,
/// and for the same reason: a clean refusal beats a partial write.
pub(crate) fn preflight_database(database_url: &str) -> Result<(), crate::Error> {
    let out = Command::new("sudo")
        .args(["-u", "virtues", "psql", database_url, "-tAc", "SELECT 1"])
        .output()
        .map_err(|e| {
            crate::Error::Other(format!(
                "could not run psql to check the database ({e}); refusing to \
                 restore, box untouched"
            ))
        })?;
    if !out.status.success() {
        return Err(crate::Error::Other(format!(
            "cannot reach the database as the `virtues` user, so the restore \
             would replace your files and then fail to load the database. \
             Refusing; box untouched.\n\n{}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
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
        // Streamed, not slurped. Verification runs over every artifact in the
        // archive — the database dump included — and reading each one into
        // memory in full to hash it put the peak footprint of a *restore* at the
        // size of its largest member.
        let mut f = File::open(&abs)
            .map_err(|e| crate::Error::Other(format!("read artifact {}: {e}", art.path)))?;
        let mut h = Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];
        let mut len: u64 = 0;
        loop {
            let n = f
                .read(&mut buf)
                .map_err(|e| crate::Error::Other(format!("read artifact {}: {e}", art.path)))?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
            len += n as u64;
        }
        if len != art.size_bytes {
            return Err(crate::Error::Other(format!(
                "{} size mismatch (manifest {}, actual {len})",
                art.path, art.size_bytes,
            )));
        }
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
    // Runs as `virtues`, not root. Restore requires root — it stops the unit,
    // writes the env file, and hands directories back — and is deliberately
    // absent from main.rs's DB_COMMANDS re-exec list. So an inherited
    // pg_restore authenticates to a peer-auth cluster as `root` and is refused
    // outright. This is why restore had never once completed on a real box.
    //
    // The staged dump was written by root, so hand it over first or the
    // service user cannot read what it is being asked to load.
    for path in [
        dump.parent().and_then(|p| p.parent()),
        dump.parent(),
        Some(dump),
    ]
    .into_iter()
    .flatten()
    {
        let _ = Command::new("chown")
            .arg("virtues:virtues")
            .arg(path)
            .status();
    }
    // Drop + recreate the schema cleanly. `--clean --if-exists` makes the
    // pg_restore drop all existing objects before recreating. `--no-owner`
    // + `--no-acl` skip privilege replays that would fail in a peer-auth
    // setup.
    let status = Command::new("sudo")
        .args(["-u", "virtues", "pg_restore"])
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

/// Every age v1 file starts with this, armored or not.
const AGE_MAGIC: &[u8] = b"age-encryption.org/v1";

/// Open the archive, decrypting it when it is encrypted.
///
/// Sniffed rather than assumed from the filename, for two reasons: archives
/// written before encryption existed must still restore, and an operator who
/// renames a file should not thereby change how it is read.
fn open_archive(
    archive: &Path,
    identity: Option<&age::x25519::Identity>,
) -> Result<Box<dyn Read>, crate::Error> {
    let mut file = File::open(archive)
        .map_err(|e| crate::Error::Other(format!("open {}: {e}", archive.display())))?;
    let mut magic = [0u8; AGE_MAGIC.len()];
    let n = read_up_to(&mut file, &mut magic)?;
    file.rewind()
        .map_err(|e| crate::Error::Other(format!("rewind {}: {e}", archive.display())))?;

    if n < AGE_MAGIC.len() || magic != AGE_MAGIC {
        // Plaintext archive from before encryption landed.
        return Ok(Box::new(file));
    }
    let Some(identity) = identity else {
        return Err(crate::Error::Other(format!(
            "{} is encrypted and no key was given. Pass --key-file <path> with the \
             recovery key printed when this box took its first backup. The box does \
             not hold it — that is deliberate, and it is why a stolen box cannot \
             read this archive.",
            archive.display()
        )));
    };
    let decryptor = age::Decryptor::new(file)
        .map_err(|e| crate::Error::Other(format!("reading age header: {e}")))?;
    let reader = decryptor
        .decrypt(std::iter::once(identity as &dyn age::Identity))
        .map_err(|e| {
            crate::Error::Other(format!(
                "could not decrypt {} ({e}). This key does not match the archive — \
                 a box that re-minted its recipient cannot open archives written \
                 before that.",
                archive.display()
            ))
        })?;
    Ok(Box::new(reader))
}

/// `Read::read` may return short; fill as much of `buf` as the file has.
fn read_up_to(f: &mut File, buf: &mut [u8]) -> Result<usize, crate::Error> {
    let mut filled = 0;
    while filled < buf.len() {
        let n = f
            .read(&mut buf[filled..])
            .map_err(|e| crate::Error::Other(format!("read: {e}")))?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    Ok(filled)
}

/// Read the recovery key from a file. Whitespace-tolerant, because it will have
/// been copied out of a password manager or typed off paper.
fn read_identity(path: &Path) -> Result<age::x25519::Identity, crate::Error> {
    use std::str::FromStr;
    let text = fs::read_to_string(path)
        .map_err(|e| crate::Error::Other(format!("read key file {}: {e}", path.display())))?;
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .ok_or_else(|| {
            crate::Error::Other(format!("{} contains no key", path.display()))
        })?;
    age::x25519::Identity::from_str(line).map_err(|e| {
        crate::Error::Other(format!(
            "{} is not a valid age recovery key ({e}) — expected one starting \
             AGE-SECRET-KEY-1",
            path.display()
        ))
    })
}

fn extract_into(reader: Box<dyn Read>, dest: &Path) -> Result<(), crate::Error> {
    let gz = GzDecoder::new(reader);
    let mut t = tar::Archive::new(gz);
    t.unpack(dest)
        .map_err(|e| crate::Error::Other(format!("tar unpack: {e}")))?;
    Ok(())
}

/// `chown -R virtues:virtues <dir>`. Shells out to match how the installer
/// does the same job; a pure-Rust walk would need the uid/gid lookup anyway.
pub(crate) fn give_to_service_user(dir: &Path) {
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
        // Deep nesting matters: the archive no longer carries directory
        // entries, only files, so every intermediate directory has to be
        // reconstructed from the member path on extract.
        write(
            &lake.join("streams/ios/date=2026-07-25/nested/deep.jsonl"),
            "{\"deep\":true}\n",
        );
        // Binary, and containing bytes that a text-mode path would mangle.
        let blob: Vec<u8> = (0u8..=255).collect();
        fs::create_dir_all(lake.join("media/ios")).unwrap();
        fs::write(lake.join("media/ios/clip.bin"), &blob).unwrap();
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
        // A throwaway keypair standing in for the box's. The test holds the
        // secret half; production never does.
        let identity = age::x25519::Identity::generate();
        let archive = scratch_dir("out").join("backup.tar.gz.age");
        crate::cli::backup::write_archive(
            &pool,
            Some(archive.clone()),
            false,
            &sources,
            &identity.to_public(),
            true,
        )
        .await
        .expect("backup");
        assert!(archive.exists(), "archive was not written");

        // Encrypted at rest: the plaintext key we seeded must not be findable
        // by scanning the bytes on disk.
        let raw = fs::read(&archive).unwrap();
        assert!(
            raw.starts_with(b"age-encryption.org/v1"),
            "archive is not age-encrypted"
        );
        assert!(
            !raw.windows(9).any(|w| w == b"drill-key"),
            "the encryption key appears in cleartext inside the archive"
        );
        // And it must not open without the key.
        assert!(
            open_archive(&archive, None).is_err(),
            "an encrypted archive opened with no key"
        );

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
        extract_into(
            open_archive(&archive, Some(&identity)).expect("decrypt"),
            staging,
        )
        .expect("extract");
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
            fs::read_to_string(lake.join("streams/ios/date=2026-07-25/nested/deep.jsonl")).unwrap(),
            "{\"deep\":true}\n",
            "a deeply nested lake file did not survive — intermediate directories \
             are reconstructed from member paths, not from directory entries"
        );
        assert_eq!(
            fs::read(lake.join("media/ios/clip.bin")).unwrap(),
            blob,
            "binary lake content differs after restore"
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

    #[sqlx::test]
    async fn verify_accepts_a_good_archive_and_rejects_a_damaged_one(pool: sqlx::PgPool) {
        require_pg_tools();
        let url = scratch_url(&pool);
        let root = scratch_dir("verify");
        let lake = root.join("lake");
        let env_file = root.join("virtues.env");
        write(&env_file, "VIRTUES_ENCRYPTION_KEY=k\n");
        write(&lake.join("streams/x.jsonl"), "payload");

        let identity = age::x25519::Identity::generate();
        let key_path = root.join("key.txt");
        {
            use age::secrecy::ExposeSecret;
            fs::write(&key_path, identity.to_string().expose_secret()).unwrap();
        }
        let archive = scratch_dir("verify-out").join("a.tar.gz.age");
        crate::cli::backup::write_archive(
            &pool,
            Some(archive.clone()),
            false,
            &crate::cli::backup::Sources {
                database_url: url,
                lake,
                applets: root.join("applets"),
                env_file: Some(env_file),
            },
            &identity.to_public(),
            true,
        )
        .await
        .expect("backup");

        verify(archive.clone(), Some(key_path.clone()))
            .await
            .expect("a freshly written archive must verify");

        // Without the key it cannot even be opened, let alone verified.
        assert!(verify(archive.clone(), None).await.is_err());

        // Flip bytes in the middle of the ciphertext. age authenticates the
        // stream, so this must fail — if it passed, verification would be
        // theatre.
        let mut bytes = fs::read(&archive).unwrap();
        let mid = bytes.len() / 2;
        bytes[mid] ^= 0xFF;
        let damaged = archive.with_extension("damaged");
        fs::write(&damaged, &bytes).unwrap();
        assert!(
            verify(damaged, Some(key_path)).await.is_err(),
            "a corrupted archive verified clean"
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

/// `virtues restore --from-volume <path>`.
///
/// Takes a PATH, not a registered volume id, and that is the whole point. The
/// volume registry lives in `storage_volume` — inside the database being
/// restored — so on the scenario that matters most, a fresh box with an empty
/// database, there is no row to look up. The drive is physically present; the
/// operator can point at it.
///
/// It also has to work as root, which restore requires (it stops the unit,
/// writes the env file, drives pg_restore). `restore` is deliberately absent
/// from main.rs's DB_COMMANDS re-exec list, so on a peer-auth box a database
/// query from here authenticates as `root` and is refused outright.
async fn run_from_volume(
    path: &Path,
    identity: Option<&age::x25519::Identity>,
) -> Result<(), crate::Error> {
    let root = resolve_volume_root(path)?;
    let targets = Targets::from_env()?;
    preflight_database(&targets.database_url)?;

    println!();
    println!("⚠  About to overwrite live box state. Press Ctrl-C in 5s to abort.");
    std::thread::sleep(std::time::Duration::from_secs(5));

    apply_from_volume(&root, &targets, identity)?;
    for dir in [targets.lake.as_path(), targets.applets.as_path()] {
        if dir.is_dir() {
            give_to_service_user(dir);
        }
    }
    print_next_steps();
    Ok(())
}

/// Accept either the box directory itself or the mount point above it.
///
/// An operator restoring onto replacement hardware knows where they plugged the
/// drive in, not what the old box called itself, so pointing at `/mnt/backup`
/// has to work — or at minimum say exactly what to point at instead.
pub(crate) fn resolve_volume_root(path: &Path) -> Result<PathBuf, crate::Error> {
    if path.join("archives").is_dir() {
        return Ok(path.to_path_buf());
    }
    // One or two levels down: <mount>/virtues/<box>/archives.
    let mut found = Vec::new();
    for depth1 in read_dirs(path) {
        if depth1.join("archives").is_dir() {
            found.push(depth1.clone());
        }
        for depth2 in read_dirs(&depth1) {
            if depth2.join("archives").is_dir() {
                found.push(depth2);
            }
        }
    }
    match found.len() {
        1 => Ok(found.remove(0)),
        0 => Err(crate::Error::Other(format!(
            "no backups under {}. Expected an `archives/` directory, or a box \
             directory containing one. Is the drive mounted?",
            path.display()
        ))),
        _ => Err(crate::Error::Other(format!(
            "{} holds backups from more than one box; name the one to restore:\n{}",
            path.display(),
            found
                .iter()
                .map(|p| format!("    {}", p.display()))
                .collect::<Vec<_>>()
                .join("\n")
        ))),
    }
}

fn read_dirs(path: &Path) -> Vec<PathBuf> {
    fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default()
}

/// Restore from a backup volume: the newest full archive, then every increment.
///
/// A volume backup is not one file. The newest `full-*` carries the database,
/// applet state and env; the lake arrives as `lake-*` increments, each holding
/// the files that were new when it was written. Every increment is therefore
/// required — each file exists in exactly one — and they must be applied in
/// order so that a file is never shadowed by an older copy of itself.
///
/// A missing increment is a hole in the restored lake. It fails loudly and names
/// the window rather than restoring short and silent, because a restore that
/// quietly returns less than it should is the failure this whole path exists to
/// prevent.
pub(crate) fn apply_from_volume(
    root: &Path,
    targets: &Targets,
    identity: Option<&age::x25519::Identity>,
) -> Result<(), crate::Error> {
    let archives = root.join("archives");
    let (full, increments) = survey_volume(&archives)?;

    println!(
        "→ restoring from {} ({} increment(s))…",
        archives.display(),
        increments.len()
    );

    // Guard first, before anything destructive runs. A volume `full-*` carries
    // no `lake/` member (the lake arrives as increments), so `apply` never
    // reaches its own clearing branch and never runs this check — the volume
    // path has to do it itself.
    assert_replaceable_lake(&targets.lake)?;

    // The full archive first: it replaces the database and applet state
    // wholesale, so increments unpacked before it would survive into a restore
    // whose database does not describe them.
    let stage = mkstage(&full)?;
    let staging: &Path = stage.as_ref();
    extract_into(open_archive(&full, identity)?, staging)?;
    let manifest = read_manifest(staging)?;
    println!(
        "→ full archive: binary {}, schema {}, created {}",
        manifest.binary_version, manifest.schema_version, manifest.created_at
    );
    check_schema_compatible(&manifest)?;
    verify_sha256(staging, &manifest.artifacts)?;
    apply(staging, targets)?;

    // Clear the lake explicitly. `apply` only does this when the archive it was
    // given contains a `lake/` member, which a volume full never does — so
    // without this a restore would MERGE the archived lake into whatever was
    // already on the box. Files belonging to no archive would survive, be
    // indistinguishable from restored ones, and quietly make the result
    // something other than the state that was backed up.
    let _ = fs::remove_dir_all(&targets.lake);
    fs::create_dir_all(&targets.lake)
        .map_err(|e| crate::Error::Other(format!("create lake dir: {e}")))?;

    // Then each increment, oldest first, unpacked straight into the lake.
    for (n, inc) in increments.iter().enumerate() {
        let name = inc.file_name().unwrap_or_default().to_string_lossy();
        println!("→ increment {}/{}: {name}", n + 1, increments.len());
        let stage = mkstage(inc)?;
        let staging: &Path = stage.as_ref();
        extract_into(open_archive(inc, identity)?, staging)?;
        let manifest = read_manifest(staging)?;
        verify_sha256(staging, &manifest.artifacts)?;

        let staged_lake = staging.join("lake");
        if staged_lake.is_dir() {
            copy_tree(&staged_lake, &targets.lake)?;
        }
    }

    Ok(())
}

/// Newest `full-*` plus every `lake-*` in chronological order.
///
/// Increment filenames are UTC timestamps in a fixed-width format, so lexical
/// order is chronological order — no parsing, and no dependence on mtimes that a
/// copy between drives would not preserve.
fn survey_volume(archives: &Path) -> Result<(PathBuf, Vec<PathBuf>), crate::Error> {
    let mut fulls = Vec::new();
    let mut increments = Vec::new();
    let entries = fs::read_dir(archives).map_err(|e| {
        crate::Error::Other(format!(
            "{} is not a backup volume ({e}) — expected an archives/ directory",
            archives.display()
        ))
    })?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".partial") {
            // A run that died mid-write. Skipping is right: it is incomplete by
            // definition, and its contents are still recorded as unsent.
            eprintln!("warning: ignoring incomplete artifact {name}");
        } else if name.starts_with("full-") {
            fulls.push((name, entry.path()));
        } else if name.starts_with("lake-") {
            increments.push((name, entry.path()));
        }
    }
    fulls.sort();
    increments.sort();

    let full = fulls
        .pop()
        .map(|(_, p)| p)
        .ok_or_else(|| {
            crate::Error::Other(format!(
                "no full archive in {} — cannot restore a lake without the database \
                 that describes it",
                archives.display()
            ))
        })?;
    Ok((full, increments.into_iter().map(|(_, p)| p).collect()))
}

/// `virtues backup --verify <archive>` — prove an archive is readable without
/// restoring anything.
///
/// Decrypts, extracts to a scratch directory, and re-hashes every member against
/// the manifest. Deliberately shares `open_archive`, `read_manifest` and
/// `verify_sha256` with the restore path: a verifier that checked the archive its
/// own way could pass something restore would reject.
pub async fn verify(
    archive: PathBuf,
    key_file: Option<PathBuf>,
) -> Result<(), crate::Error> {
    if !archive.exists() {
        return Err(crate::Error::Other(format!("{} not found", archive.display())));
    }
    let identity = key_file.as_deref().map(read_identity).transpose()?;
    println!("→ verifying {}…", archive.display());

    let stage = mkstage(&archive)?;
    let staging: &Path = stage.as_ref();
    extract_into(open_archive(&archive, identity.as_ref())?, staging)?;
    let manifest = read_manifest(staging)?;
    println!(
        "→ manifest: binary {}, schema {}, created {}",
        manifest.binary_version, manifest.schema_version, manifest.created_at
    );
    verify_sha256(staging, &manifest.artifacts)?;

    let total: u64 = manifest.artifacts.iter().map(|a| a.size_bytes).sum();
    println!(
        "✓ {} member(s), {:.1} MB, all digests match",
        manifest.artifacts.len(),
        total as f64 / (1024.0 * 1024.0)
    );
    // Says nothing about whether the CONTENTS restore cleanly — that is what the
    // round-trip drill is for. This proves the bytes are intact and readable,
    // which is the failure mode an unattended backup actually has.
    Ok(())
}

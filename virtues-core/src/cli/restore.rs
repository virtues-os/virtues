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
//!
//! Archives are decrypted into a private folder on this box's own disk
//! (`backup::Stage`), never beside the archive, and after the database loads
//! restore checks that the restored key opens the restored secrets.

use std::fs::{self, File};
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::process::Command;

use flate2::read::GzDecoder;

use super::backup::{Artifact, Manifest, Stage};

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
            applets: crate::applet_templates::state_root(),
            // The file the server unit loads. A key restored anywhere else
            // leaves a box that boots fine (`EnvironmentFile=-` makes a
            // missing file non-fatal) and cannot decrypt anything.
            env_file: crate::box_env::path(),
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

    // Extract into a stage on this box's disk that we can inspect.
    sweep_stages_near(&path);
    ensure_room_to_unpack(std::slice::from_ref(&path))?;
    let stage = open_stage("restore")?;
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
    ensure_room_for_database(&targets, &manifest)?;

    // We've validated everything. Past `begin_changes` is destructive.
    println!();
    println!("⚠  About to overwrite live box state. Press Ctrl-C in 5s to abort.");
    std::thread::sleep(std::time::Duration::from_secs(5));

    super::backup::begin_changes()?;
    apply(staging, &targets)?;

    // Everything above was written by root (restore requires it — it stops the
    // unit, writes the env file, and drives pg_restore). Root extracted the
    // files that `move_tree` moved or copied into place, so the restored trees
    // land root-owned while the service runs as `virtues`. Left
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
    super::backup::end_changes();
    // The plaintext goes before the key check, which may fail the command.
    drop(stage);

    check_restored_key(&targets)?;
    print_next_steps();
    Ok(())
}

/// A stage for decrypting an archive, on this box's own disk.
///
/// Restore runs as root, but `pg_restore` runs as the service user and has
/// to pass through the staging folder to read the dump (`pg_restore` hands
/// the stage itself over). A root-owned staging folder would also stop the
/// next backup, which runs as that user. So the folder goes to the service
/// user; the stage inside it stays root's, 0700, until the dump is needed.
fn open_stage(purpose: &str) -> Result<Stage, crate::Error> {
    let stage = Stage::create(purpose)?;
    if let Some(root) = stage.path().parent() {
        // Best-effort and quiet, like the hand-over in `pg_restore`: off a
        // box there is no service user, and nothing needs one.
        let _ = Command::new("chown").arg("virtues:virtues").arg(root).output();
    }
    Ok(stage)
}

/// Clear stages earlier runs left: this box's own stale ones, and any an
/// earlier release made beside the archive.
fn sweep_stages_near(archive: &Path) {
    super::backup::sweep_stale_stages();
    if let Some(dir) = archive.parent() {
        super::backup::sweep_legacy_stages(dir);
    }
}

/// Refuse before unpacking when this box's disk cannot hold the archives and
/// still leave the floor free.
///
/// The stage is on the box, never on the backup drive, so a restore needs
/// room here for what it unpacks, while Postgres is still writing to the same
/// disk. The archives' compressed size is a lower bound on what they unpack
/// to. A box short of it would fail partway through unpacking.
fn ensure_room_to_unpack(archives: &[PathBuf]) -> Result<(), crate::Error> {
    let root = super::backup::staging_root();
    let Some(free) = super::backup::free_bytes_under(&root) else {
        return Ok(()); // cannot measure; the unpack itself reports a full disk
    };
    let need: u64 = archives
        .iter()
        .filter_map(|a| fs::metadata(a).ok())
        .map(|m| m.len())
        .sum();
    let floor = super::backup::STAGE_FLOOR_BYTES;
    if free < need.saturating_add(floor) {
        return Err(crate::Error::Other(format!(
            "not enough room on this server to unpack the backup. {} MB is free \
             where restore unpacks it ({}). The archives alone are {} MB, and \
             restore leaves {} MB free so the database can keep working. Restore \
             unpacks on this server, never on the backup drive. Free some space, \
             then run it again. Nothing has been changed.",
            free / (1024 * 1024),
            root.display(),
            need / (1024 * 1024),
            floor / (1024 * 1024),
        )));
    }
    Ok(())
}

/// Refuse before anything is replaced when the restored database will not
/// fit beside the unpacked archive.
///
/// `pg_restore` loads the database while the stage is still on disk, and a
/// load that runs out of room stops halfway, after the lake, applets and env
/// file have been replaced. `--clean` drops the current database first, so
/// the load has the free space plus what that database takes. The dump's own
/// size stands in for the restored database, which is larger once indexes are
/// rebuilt, so this catches only a clear shortfall. It measures the disk that
/// holds the stage, which on a box usually holds Postgres too.
fn ensure_room_for_database(targets: &Targets, manifest: &Manifest) -> Result<(), crate::Error> {
    // `apply` refuses an archive with no dump, and says why.
    let Some(dump) = manifest.artifacts.iter().find(|a| a.path == "db/virtues.dump") else {
        return Ok(());
    };
    let root = super::backup::staging_root();
    let Some(free) = super::backup::free_bytes_under(&root) else {
        return Ok(()); // cannot measure; pg_restore itself reports a full disk
    };
    let current: u64 = psql_lines(
        &targets.database_url,
        "SELECT pg_database_size(current_database())",
    )?
    .first()
    .and_then(|n| n.trim().parse().ok())
    .ok_or_else(|| crate::Error::Other("could not read the size of the current database".into()))?;
    let floor = super::backup::STAGE_FLOOR_BYTES;
    if free.saturating_add(current) < dump.size_bytes.saturating_add(floor) {
        return Err(crate::Error::Other(format!(
            "not enough room on this server to load the restored database. The \
             database dump is {} MB. With the backup unpacked, {} MB is free at {}, \
             and replacing the current database frees {} MB more. Restore leaves {} MB \
             free so the database can keep working. Free some space, then run it \
             again. Nothing has been changed.",
            dump.size_bytes / (1024 * 1024),
            free / (1024 * 1024),
            root.display(),
            current / (1024 * 1024),
            floor / (1024 * 1024),
        )));
    }
    Ok(())
}

/// How many sealed rows restore tries with the restored key. Every row on a
/// typical box; a random sample on one with more.
const KEY_CHECK_ROWS: usize = 2000;

/// Check that the key now in the env file opens the restored `credentials`.
///
/// The database and the key arrive separately: the key from the archive's
/// env file, or this box's own when the archive had none. A key that does not
/// match leaves a box that starts and cannot read a single saved password,
/// sign-in or key, and every secret saved after that is sealed with the wrong
/// key. So restore does not report success until the key opens the table.
///
/// Rows that do not open beside rows that do were sealed with another key, or
/// damaged, before the backup was made: the key is right, so they are a
/// warning, not a failure.
pub(crate) fn check_restored_key(targets: &Targets) -> Result<(), crate::Error> {
    println!("→ checking that the restored key opens the restored secrets…");
    let total: usize = psql_lines(
        &targets.database_url,
        "SELECT count(*) FROM credentials WHERE secrets_ciphertext <> ''",
    )?
    .first()
    .and_then(|n| n.trim().parse().ok())
    .ok_or_else(|| crate::Error::Other("could not count the restored secrets".into()))?;
    if total == 0 {
        println!("→ the backup holds no saved secrets, so there is no key to check");
        return Ok(());
    }
    let sealed = psql_lines(
        &targets.database_url,
        &format!(
            "SELECT secrets_ciphertext FROM credentials WHERE secrets_ciphertext <> '' \
             ORDER BY random() LIMIT {KEY_CHECK_ROWS}"
        ),
    )?;
    let env = targets.env_file.display();
    let (key, source) = key_after_restore(
        &targets.env_file,
        std::env::var(crate::box_env::ENCRYPTION_KEY).ok(),
    );
    let enc = match key {
        Some(key) => Some(
            virtues_helpers::crypto::TokenEncryptor::from_base64_key(&key).map_err(|e| {
                crate::Error::Other(format!(
                    "the {} in {source} is not a valid key ({e}), so none of the {total} \
                     restored secrets can be opened. Put the virtues.env from the server \
                     this backup came from at {env}, then start the service.",
                    crate::box_env::ENCRYPTION_KEY
                ))
            })?,
        ),
        None => None,
    };
    let check = crate::secrets::try_key(enc.as_ref(), sealed.iter().map(String::as_str));
    let tried = if check.tried < total {
        format!("{} of the {total} saved secrets, chosen at random", check.tried)
    } else if total == 1 {
        "the one saved secret".to_string()
    } else {
        format!("all {total} saved secrets")
    };

    if check.opens_nothing() {
        let why = if enc.is_none() {
            format!("{source} has no {}", crate::box_env::ENCRYPTION_KEY)
        } else {
            format!("the key in {source} is not the one they were saved with")
        };
        return Err(crate::Error::Other(format!(
            "the files and the database are restored, but {why}. Restore tried \
             {tried}, and none opened. Every saved password, sign-in and key \
             is unreadable until that is fixed.\n\n    \
             Put the virtues.env from the server this backup came from at {env}, \
             then start the service. Until then, do not enter passwords or keys \
             again: anything saved now is sealed with the wrong key."
        )));
    }
    if check.failed() > 0 {
        eprintln!(
            "warning: restore tried {tried}, and {} did not open with the \
             restored key. The rest do, so the key is right: those rows were sealed \
             with another key, or damaged, before the backup was made. Enter them \
             again once the server is running.",
            check.failed(),
        );
    } else {
        println!("→ the restored key opens {tried}");
    }
    Ok(())
}

/// The key the server will read after the restore, and where it is, for
/// messages.
///
/// The env file when it exists: on a box the unit loads it, and this
/// process's own environment still holds the key from before the restore.
/// With no env file, as on a dev machine that keeps its key in a repo `.env`,
/// the server takes its key from its environment, which this process loaded
/// the same way at start (`process_key`).
fn key_after_restore(env_file: &Path, process_key: Option<String>) -> (Option<String>, String) {
    if env_file.exists() {
        return (
            crate::box_env::encryption_key(env_file),
            env_file.display().to_string(),
        );
    }
    (
        process_key.filter(|k| !k.trim().is_empty()),
        format!("this process's environment (there is no {})", env_file.display()),
    )
}

/// One line per row of a single-column query, run through `psql` as the
/// user that can reach the database (`db_command`).
fn psql_lines(database_url: &str, sql: &str) -> Result<Vec<String>, crate::Error> {
    let out = db_command("psql")
        .args(["-X", "-tA", database_url, "-c", sql])
        .output()
        .map_err(|e| crate::Error::Other(format!("could not run psql ({e})")))?;
    if !out.status.success() {
        return Err(crate::Error::Other(format!(
            "the restored database could not be read to check the key: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
}

/// Build a command that talks to Postgres as whoever can actually authenticate.
///
/// On a box the cluster uses peer auth and restore runs as root, so the call has
/// to drop to `virtues` or be refused. Anywhere else — a dev machine, CI, a test
/// — there is no `virtues` user and the URL carries explicit credentials, so
/// dropping would break what already works. Decide from the environment rather
/// than assuming either one.
fn db_command(program: &str) -> Command {
    // Reuses upgrade's EUID probe rather than a second copy. It reads
    // /proc/self/status, so it is false on macOS — which is the right answer
    // there anyway, since a dev machine has no service user to drop to.
    let am_root = super::upgrade::running_as_root();
    let service_user_exists = Command::new("id")
        .args(["-u", "virtues"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if am_root && service_user_exists {
        let mut c = Command::new("sudo");
        c.args(["-u", "virtues", program]);
        c
    } else {
        Command::new(program)
    }
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
    let out = db_command("psql")
        .args([database_url, "-tAc", "SELECT 1"])
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
        move_tree(&staged_lake, &targets.lake)?;
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
        move_tree(&staged_applets, &targets.applets)?;
    }

    let staged_env = staging.join("env/virtues.env");
    // An archive env file without the key never replaces one with it: that
    // would destroy the only key this box has, for nothing.
    let keeps_own_key = staged_env.exists()
        && crate::box_env::encryption_key(&staged_env).is_none()
        && crate::box_env::encryption_key(&targets.env_file).is_some();
    if keeps_own_key {
        eprintln!(
            "warning: the backup's env file has no {}, so this box keeps its own \
             env file at {}. The key check after the database loads says whether \
             that key opens the restored secrets.",
            crate::box_env::ENCRYPTION_KEY,
            targets.env_file.display()
        );
    } else if staged_env.exists() {
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
        // The tarball carries no key, so whether the restored secrets open
        // depends on this box's own key, which `check_restored_key` tests.
        eprintln!(
            "warning: the backup has no env file, so {} stays as it is. The key \
             check after the database loads says whether its key opens the \
             restored secrets.",
            targets.env_file.display()
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
    parse_manifest(&raw)
}

fn parse_manifest(raw: &[u8]) -> Result<Manifest, crate::Error> {
    serde_json::from_slice::<Manifest>(raw)
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
        // Streamed, not slurped. Verification runs over every artifact in the
        // archive — the database dump included — and reading each one into
        // memory in full to hash it put the peak footprint of a *restore* at the
        // size of its largest member.
        let read = File::open(staging.join(&art.path)).and_then(super::backup::digest);
        let got = read
            .map_err(|e| crate::Error::Other(format!("read artifact {}: {e}", art.path)))?;
        check_artifact(art, &got)?;
    }
    Ok(())
}

/// Compare one artifact with the length and sha256 read for it
/// (`backup::digest`). Restore and `--verify` both decide here.
fn check_artifact(art: &Artifact, (len, sha256): &(u64, String)) -> Result<(), crate::Error> {
    if *len != art.size_bytes {
        return Err(crate::Error::Other(format!(
            "{} size mismatch (manifest {}, actual {len})",
            art.path, art.size_bytes,
        )));
    }
    if *sha256 != art.sha256 {
        return Err(crate::Error::Other(format!(
            "{} sha256 mismatch — backup is corrupt or modified",
            art.path
        )));
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
    let out = db_command("pg_restore")
        .arg("--clean")
        .arg("--if-exists")
        .arg("--no-owner")
        .arg("--no-acl")
        .arg("-d")
        .arg(database_url)
        .arg(dump)
        .output()
        .map_err(|e| crate::Error::Other(format!("invoke pg_restore: {e}")))?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !stderr.trim().is_empty() {
        eprint!("{stderr}");
    }
    if out.status.success() {
        return Ok(());
    }

    // `--clean` emits DROP EXTENSION and COMMENT ON EXTENSION for pgvector,
    // which the `virtues` role cannot execute: the extension was installed by a
    // superuser, and a role does not own an extension it did not create. Those
    // statements are cosmetic — the extension already exists and is left
    // untouched — but pg_restore exits non-zero for ANY skipped object. So a
    // restore that had in fact succeeded completely reported failure and told
    // the operator to leave the service stopped, which is a worse outcome than
    // the thing it was warning about.
    //
    // pg_restore continues past errors and prints the total at the end. If that
    // total is exactly the number of extension-ownership complaints, nothing
    // else was skipped and the data is whole. Any other error stays fatal.
    let reported: Option<usize> = stderr.lines().find_map(|l| {
        l.rsplit_once("errors ignored on restore: ")
            .and_then(|(_, n)| n.trim().parse().ok())
    });
    let extension_only = stderr.matches("must be owner of extension").count();
    if extension_only > 0 && reported == Some(extension_only) {
        eprintln!(
            "note: {extension_only} statement(s) about database extensions were \
             skipped — those are owned by the cluster superuser rather than by \
             `virtues`, and the extensions themselves are untouched. Everything \
             else restored."
        );
        return Ok(());
    }

    Err(crate::Error::Other(format!(
        "pg_restore exited {}; backup not fully restored. \
         Service should remain stopped until the cause is fixed.",
        out.status
    )))
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
    // Restored files are dated by the restore, as copies are. The archive's
    // headers carry no real time, and `move_tree` renames files into place,
    // which would otherwise keep that one.
    t.set_preserve_mtime(false);
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

/// Move a staged tree into place, file by file.
///
/// A rename when the stage and the target share a filesystem, which they
/// usually do now that the stage is on the box's own disk: the restore then
/// needs room for one copy of the lake, not two. A copy when they do not, as
/// for a lake on a disk of its own. Either way the stage is deleted after.
fn move_tree(src: &Path, dst: &Path) -> Result<(), crate::Error> {
    fs::create_dir_all(dst)
        .map_err(|e| crate::Error::Other(format!("mkdir {}: {e}", dst.display())))?;
    for entry in fs::read_dir(src)
        .map_err(|e| crate::Error::Other(format!("read {}: {e}", src.display())))?
    {
        let entry = entry.map_err(|e| crate::Error::Other(format!("dir entry: {e}")))?;
        let path = entry.path();
        let dest = dst.join(entry.file_name());
        if path.is_dir() {
            move_tree(&path, &dest)?;
            continue;
        }
        match fs::rename(&path, &dest) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
                fs::copy(&path, &dest)
                    .map_err(|e| crate::Error::Other(format!("copy {}: {e}", path.display())))?;
            }
            Err(e) => {
                return Err(crate::Error::Other(format!("move {}: {e}", path.display())));
            }
        }
    }
    Ok(())
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

    /// An invented box key, base64 of 32 equal bytes.
    fn key(byte: u8) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode([byte; 32])
    }

    fn seal(key_b64: &str, value: &str) -> String {
        virtues_helpers::crypto::TokenEncryptor::from_base64_key(key_b64)
            .unwrap()
            .encrypt(value)
            .unwrap()
    }

    /// Everything in `dir` but `keep`. A backup or a verify must leave nothing
    /// beside the archive: that folder is often the backup drive.
    fn beside(dir: &Path, keep: &Path) -> Vec<String> {
        fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter(|e| e.path() != keep)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect()
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
        let box_key = key(7);
        write(&env_file, &format!("VIRTUES_ENCRYPTION_KEY={box_key}\n"));
        sqlx::query(
            "INSERT INTO credentials (id, key, name, status, secrets_ciphertext) \
             VALUES ($1, 'ios', 'drill', 'active', $2)",
        )
        .bind("cred_drill")
        .bind(seal(&box_key, "{\"token\":\"invented\"}"))
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
        // A stage an earlier release left beside its output, with a dump in it.
        let legacy = archive.with_file_name(".virtues-backup-stage-20260101T000000");
        write(&legacy.join("virtues.dump"), "PGDMP");
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
        assert_eq!(
            beside(archive.parent().unwrap(), &archive),
            Vec::<String>::new(),
            "the backup left something beside its archive"
        );

        // Encrypted at rest: the plaintext key we seeded must not be findable
        // by scanning the bytes on disk.
        let raw = fs::read(&archive).unwrap();
        assert!(
            raw.starts_with(b"age-encryption.org/v1"),
            "archive is not age-encrypted"
        );
        assert!(
            !raw.windows(box_key.len()).any(|w| w == box_key.as_bytes()),
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
        let stage = open_stage("restore").expect("stage");
        let staging: &Path = stage.as_ref();
        assert!(
            !staging.starts_with(archive.parent().unwrap()),
            "restore staged beside the archive"
        );
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

        let targets = Targets {
            database_url: url,
            lake: lake.clone(),
            applets: applets.clone(),
            env_file: env_file.clone(),
        };
        apply(staging, &targets).expect("restore");
        assert!(
            !staging.join("lake/streams/ios/records_deadbeef.jsonl").exists(),
            "the lake was copied out of the stage rather than moved"
        );
        drop(stage);
        check_restored_key(&targets).expect("the restored key opens the restored table");

        // The same database under another key is the failure the check exists
        // for: it must fail, and say how many it tried.
        let real = fs::read_to_string(&env_file).unwrap();
        write(&env_file, &format!("VIRTUES_ENCRYPTION_KEY={}\n", key(8)));
        let err = check_restored_key(&targets).expect_err("a wrong key passed the check");
        assert!(format!("{err}").contains("the one saved secret"), "{err}");
        write(&env_file, "DATABASE_URL=postgres:///virtues\n");
        let err = check_restored_key(&targets).expect_err("a missing key passed the check");
        assert!(format!("{err}").contains("has no VIRTUES_ENCRYPTION_KEY"), "{err}");
        write(&env_file, &real);

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
            fs::read_to_string(&env_file).unwrap().contains(&box_key),
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

        // A restore stage an earlier release left beside the archive, with
        // the box key in it.
        let legacy = archive.with_file_name(".virtues-restore-stage-20260101T000000");
        write(&legacy.join("env/virtues.env"), "VIRTUES_ENCRYPTION_KEY=invented\n");

        verify(archive.clone(), Some(key_path.clone()))
            .await
            .expect("a freshly written archive must verify");
        assert_eq!(
            beside(archive.parent().unwrap(), &archive),
            Vec::<String>::new(),
            "verify left something beside the archive"
        );

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

    /// A gzipped tar in memory with the given members, in order.
    fn tarball(members: &[(&str, &[u8])]) -> Vec<u8> {
        let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut builder = tar::Builder::new(gz);
        for (path, body) in members {
            let mut header = tar::Header::new_gnu();
            header.set_size(body.len() as u64);
            header.set_mode(0o600);
            // `set_path` refuses `..`, which a hand-made archive can still hold.
            header.as_gnu_mut().unwrap().name[..path.len()].copy_from_slice(path.as_bytes());
            header.set_cksum();
            builder.append(&header, *body).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    fn artifact(path: &str, body: &[u8]) -> Artifact {
        let (size_bytes, sha256) = crate::cli::backup::digest(body).unwrap();
        Artifact { path: path.into(), size_bytes, sha256 }
    }

    fn manifest_json(artifacts: Vec<Artifact>) -> Vec<u8> {
        serde_json::to_vec(&Manifest {
            manifest_version: 1,
            binary_version: "0.0.0".into(),
            schema_version: "1".into(),
            created_at: "2026-10-27T04:00:00Z".into(),
            distro: None,
            artifacts,
        })
        .unwrap()
    }

    #[test]
    fn verify_reads_members_from_the_stream_as_restore_would_unpack_them() {
        let dump: &[u8] = b"PGDMP invented";
        let manifest = manifest_json(vec![artifact("db/virtues.dump", dump)]);
        // A leading `./` is dropped by restore's unpack, so it is here too.
        let archive = tarball(&[("./db/virtues.dump", dump), ("manifest.json", &manifest)]);
        let (read, members) = digest_members(Box::new(std::io::Cursor::new(archive))).unwrap();
        assert_eq!(read.artifacts.len(), 1);
        let got = members.get("db/virtues.dump").expect("the member was not read");
        check_artifact(&read.artifacts[0], got).expect("an intact member failed its check");

        // Restore's unpack skips a member with `..` in its path, so it cannot
        // satisfy the manifest here either.
        let manifest = manifest_json(vec![artifact("../db/virtues.dump", dump)]);
        let archive = tarball(&[("../db/virtues.dump", dump), ("manifest.json", &manifest)]);
        let (read, members) = digest_members(Box::new(std::io::Cursor::new(archive))).unwrap();
        assert!(members.is_empty(), "a member restore would skip was read: {members:?}");
        assert!(!read.artifacts.is_empty());

        // A changed member fails the same check restore uses.
        let manifest = manifest_json(vec![artifact("db/virtues.dump", dump)]);
        let archive = tarball(&[("db/virtues.dump", b"PGDMP changed"), ("manifest.json", &manifest)]);
        let (read, members) = digest_members(Box::new(std::io::Cursor::new(archive))).unwrap();
        assert!(check_artifact(&read.artifacts[0], &members["db/virtues.dump"]).is_err());

        // No manifest, nothing to check against.
        let archive = tarball(&[("db/virtues.dump", dump)]);
        let err = digest_members(Box::new(std::io::Cursor::new(archive))).unwrap_err();
        assert!(err.to_string().contains("no manifest"), "{err}");
    }

    #[test]
    fn the_key_check_reads_the_env_file_when_there_is_one_and_the_environment_otherwise() {
        let dir = scratch_dir("key-source");
        let env_file = dir.join("virtues.env");
        let shell = Some("from-the-environment=".to_string());

        // No file, as on a dev machine: the server's key is in its environment.
        let (key, source) = key_after_restore(&env_file, shell.clone());
        assert_eq!(key.as_deref(), Some("from-the-environment="));
        assert!(source.contains("there is no"), "{source}");
        assert_eq!(key_after_restore(&env_file, None).0, None);

        // A file: the server reads it, and the environment holds the key from
        // before the restore, so the environment is ignored.
        write(&env_file, "VIRTUES_ENCRYPTION_KEY=from-the-file=\n");
        let (key, source) = key_after_restore(&env_file, shell.clone());
        assert_eq!(key.as_deref(), Some("from-the-file="));
        assert_eq!(source, env_file.display().to_string());
        write(&env_file, "DATABASE_URL=postgres:///virtues\n");
        assert_eq!(key_after_restore(&env_file, shell).0, None);

        let _ = fs::remove_dir_all(&dir);
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
    super::backup::sweep_stale_stages();
    super::backup::sweep_legacy_stages(&root.join("archives"));

    println!();
    println!("⚠  About to overwrite live box state. Press Ctrl-C in 5s to abort.");
    std::thread::sleep(std::time::Duration::from_secs(5));

    apply_from_volume(&root, &targets, identity)?;
    for dir in [targets.lake.as_path(), targets.applets.as_path()] {
        if dir.is_dir() {
            give_to_service_user(dir);
        }
    }
    super::backup::end_changes();
    check_restored_key(&targets)?;
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
/// A missing increment is a hole in the restored lake, and this cannot see
/// one: the drive keeps no list of the increments it should hold, so a deleted
/// one restores short with no warning. The manual says so.
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
    let mut everything = vec![full.clone()];
    everything.extend(increments.iter().cloned());
    ensure_room_to_unpack(&everything)?;

    // OPEN AND CHECK EVERY ARCHIVE BEFORE DESTROYING ANYTHING.
    //
    // A bad sector, a wrong key or a full disk while unpacking increment 3 of
    // 7 must leave the box as it was. So the full archive and every increment
    // are unpacked into stages on this server and checksummed first, and kept
    // for the moves below so the work is not repeated. An interrupt in this
    // part removes the stages and changes nothing.
    let full_stage = open_stage("restore")?;
    extract_into(open_archive(&full, identity)?, full_stage.path())?;
    let manifest = read_manifest(full_stage.path())?;
    println!(
        "→ full archive: binary {}, schema {}, created {}",
        manifest.binary_version, manifest.schema_version, manifest.created_at
    );
    check_schema_compatible(&manifest)?;
    verify_sha256(full_stage.path(), &manifest.artifacts)?;

    println!("→ checking {} increment(s) before touching anything…", increments.len());
    let mut staged: Vec<(String, Stage)> = Vec::with_capacity(increments.len());
    for (n, inc) in increments.iter().enumerate() {
        let name = inc.file_name().unwrap_or_default().to_string_lossy().to_string();
        println!("   {}/{}: {name}", n + 1, increments.len());
        let stage = open_stage("restore")?;
        extract_into(open_archive(inc, identity)?, stage.path())?;
        let manifest = read_manifest(stage.path())?;
        verify_sha256(stage.path(), &manifest.artifacts)?;
        staged.push((name, stage));
    }
    ensure_room_for_database(targets, &manifest)?;

    // Past this line is destructive. The full archive goes first: it replaces
    // the database and applet state wholesale, so increments moved in before
    // it would survive into a restore whose database does not describe them.
    super::backup::begin_changes()?;
    apply(full_stage.path(), targets)?;
    drop(full_stage);

    // Clear the lake explicitly. `apply` only does this when the archive it was
    // given contains a `lake/` member, which a volume full never does — so
    // without this a restore would MERGE the archived lake into whatever was
    // already on the box. Files belonging to no archive would survive, be
    // indistinguishable from restored ones, and quietly make the result
    // something other than the state that was backed up.
    let _ = fs::remove_dir_all(&targets.lake);
    fs::create_dir_all(&targets.lake)
        .map_err(|e| crate::Error::Other(format!("create lake dir: {e}")))?;

    for (n, (name, stage)) in staged.iter().enumerate() {
        println!("→ increment {}/{}: {name}", n + 1, staged.len());
        let staged_lake = stage.path().join("lake");
        if staged_lake.is_dir() {
            move_tree(&staged_lake, &targets.lake)?;
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
/// Decrypts the archive and hashes every member as it streams past, then
/// checks each one against the manifest, which is the last member. Nothing
/// it decrypts is written anywhere, so it needs no free space and leaves
/// nothing behind, on the backup drive or on this machine. It shares
/// `open_archive`, the manifest parse, `backup::digest` and `check_artifact`
/// with the restore path: a verifier that checked the archive its own way
/// could pass something restore would reject. Member paths are read the way
/// tar's unpack reads them for restore ([`unpacked_path`]).
pub async fn verify(
    archive: PathBuf,
    key_file: Option<PathBuf>,
) -> Result<(), crate::Error> {
    if !archive.exists() {
        return Err(crate::Error::Other(format!("{} not found", archive.display())));
    }
    let identity = key_file.as_deref().map(read_identity).transpose()?;
    println!("→ verifying {}…", archive.display());
    sweep_stages_near(&archive);

    let (manifest, members) = digest_members(open_archive(&archive, identity.as_ref())?)?;
    println!(
        "→ manifest: binary {}, schema {}, created {}",
        manifest.binary_version, manifest.schema_version, manifest.created_at
    );
    println!("→ verifying sha256 of {} artifact(s)…", manifest.artifacts.len());
    for art in &manifest.artifacts {
        let got = members.get(&art.path).ok_or_else(|| {
            crate::Error::Other(format!("read artifact {}: not in the archive", art.path))
        })?;
        check_artifact(art, got)?;
    }

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

/// The length and sha256 of each member, by the path it unpacks to.
type MemberDigests = std::collections::HashMap<String, (u64, String)>;

/// The manifest and the length and sha256 of every regular file in an
/// archive, read in one pass without writing anything.
fn digest_members(reader: Box<dyn Read>) -> Result<(Manifest, MemberDigests), crate::Error> {
    let mut tar = tar::Archive::new(GzDecoder::new(reader));
    let read_err = |e: std::io::Error| crate::Error::Other(format!("read archive: {e}"));
    let mut members = MemberDigests::new();
    let mut manifest = None;
    for entry in tar.entries().map_err(read_err)? {
        let mut entry = entry.map_err(read_err)?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let Some(path) = unpacked_path(&entry.path().map_err(read_err)?) else {
            continue; // restore's unpack skips it too
        };
        if path == "manifest.json" {
            let mut raw = Vec::new();
            entry.read_to_end(&mut raw).map_err(read_err)?;
            manifest = Some(parse_manifest(&raw)?);
        } else {
            let got = super::backup::digest(&mut entry)
                .map_err(|e| crate::Error::Other(format!("read artifact {path}: {e}")))?;
            members.insert(path, got);
        }
    }
    let manifest = manifest
        .ok_or_else(|| crate::Error::Other("read manifest: the archive has no manifest.json".into()))?;
    Ok((manifest, members))
}

/// The path a tar member lands at under the stage when restore unpacks it,
/// as the manifest names it: leading `/` and `.` parts dropped. `None` for a
/// member with a `..` part, which tar's unpack skips.
fn unpacked_path(path: &Path) -> Option<String> {
    use std::path::Component;
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::Normal(p) => out.push(p),
            Component::ParentDir => return None,
            Component::RootDir | Component::CurDir | Component::Prefix(_) => {}
        }
    }
    let out = out.to_string_lossy().into_owned();
    (!out.is_empty()).then_some(out)
}

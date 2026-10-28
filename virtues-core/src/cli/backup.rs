//! `virtues backup` — produce a single encrypted archive of the box state.
//!
//! The archive is `tar → gzip → age`, written straight to its destination:
//!
//!   db/virtues.dump          (pg_dump --format=custom output)
//!   env/virtues.env          (holds VIRTUES_ENCRYPTION_KEY)
//!   lake/<file...>           (the data lake)
//!   applets/<file...>        (chat-authored applets + imported packs)
//!   manifest.json            (LAST — see `stream_archive`)
//!
//! The manifest records the binary version, the schema migration version,
//! distro, a UTC timestamp, and the sha256 of every member. `virtues restore`
//! verifies all of it before touching anything live.
//!
//! Two properties worth knowing before changing anything here:
//!
//! **Only the database is staged, and only on the box.** Members stream from
//! where they already live, digested in flight. The pg_dump has no original on
//! disk, so it is written first, into a private folder on the box's own disk
//! ([`Stage`]), never beside the archive. The manifest goes last because it
//! names digests that are not known until the bytes have been read.
//!
//! **The box cannot read what it writes.** Archives are encrypted to an age
//! recipient whose secret half was printed once and never stored. A stolen box
//! yields an encryption key with nothing to decrypt; a stolen drive yields
//! ciphertext with no key, as long as nothing in plaintext is ever written to
//! the drive. Restore therefore needs `--key-file`, by design.
//!
//! The archive holds every sealed row in `credentials` and the env file with
//! the key that opens them, so anyone with the archive and the age identity
//! can read every saved password and key. The age identity is the only
//! protection.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU8, Ordering};

use chrono::Utc;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

const DEFAULT_BACKUP_DIR: &str = "/var/lib/virtues/backups";
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

/// Everything a backup reads off the box, resolved once and passed down.
///
/// Exists so the archive writer takes its inputs as arguments rather than
/// re-deriving them from the environment at each use. That is what lets the
/// round-trip drill in `tests/backup_restore.rs` run against scratch paths and
/// a scratch database — and a backup path that cannot be exercised in a test is
/// the thing this whole area was missing.
pub(crate) struct Sources {
    pub database_url: String,
    pub lake: PathBuf,
    pub applets: PathBuf,
    /// `None` only when `--allow-missing-key` was passed.
    pub env_file: Option<PathBuf>,
}

impl Sources {
    pub(crate) fn from_env(allow_missing_key: bool) -> Result<Self, crate::Error> {
        Ok(Self {
            database_url: crate::database::normalize_database_url()
                .map_err(|e| crate::Error::Other(format!("DATABASE_URL: {e}")))?,
            lake: crate::storage::lake::lake_root(),
            applets: crate::applet_templates::state_root(),
            env_file: env_file_to_archive(&crate::box_env::path(), allow_missing_key)?,
        })
    }
}

/// The env file a backup carries: the one the server reads
/// (`box_env::path`), and only when it holds the key.
///
/// Without the encryption key every sealed row in the dump is permanently
/// unreadable. A keyless backup looks complete, and you only find out
/// otherwise at restore time. So a missing file, or a file without the key, is a hard
/// failure. `--allow-missing-key` is the explicit out for dev machines that
/// keep their key in a repo `.env`, and then the archive carries no env file
/// at all, so a restore never replaces a box's key with nothing.
fn env_file_to_archive(path: &Path, allow_missing_key: bool) -> Result<Option<PathBuf>, crate::Error> {
    if crate::box_env::encryption_key(path).is_some() {
        return Ok(Some(path.to_path_buf()));
    }
    if allow_missing_key {
        eprintln!(
            "warning: this backup CANNOT decrypt the database it contains \
             (--allow-missing-key was passed)"
        );
        return Ok(None);
    }
    let problem = match fs::read_to_string(path) {
        Ok(_) => format!("{} has no {}", path.display(), crate::box_env::ENCRYPTION_KEY),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            format!("no env file at {}", path.display())
        }
        Err(e) => format!("cannot read {} ({e})", path.display()),
    };
    Err(crate::Error::Other(format!(
        "{problem}, so the backup would hold a database nobody can decrypt. \
         That file is the one the server unit loads. Check that it holds the \
         key, or pass --allow-missing-key if you want a backup without it."
    )))
}

pub async fn run(
    pool: &PgPool,
    output: Option<PathBuf>,
    force: bool,
    allow_missing_key: bool,
) -> Result<PathBuf, crate::Error> {
    let sources = Sources::from_env(allow_missing_key)?;
    let recipient = load_or_create_recipient()?;
    // A standalone `virtues backup` is one self-contained file, so it carries
    // the lake. Volume backups do not — see `include_lake`.
    write_archive(pool, output, force, &sources, &recipient, true).await
}

/// Write a full archive.
///
/// `include_lake` is the difference between the two callers, and getting it
/// wrong is silent and expensive. A standalone backup is a single complete file
/// and must contain the lake. A volume `full-*` must NOT: the lake reaches the
/// volume through `lake-*` increments, and bundling it here would re-archive
/// hundreds of gigabytes on every run — precisely the cost increments exist to
/// avoid, and invisible except as a backup that takes hours.
pub(crate) async fn write_archive(
    pool: &PgPool,
    output: Option<PathBuf>,
    force: bool,
    sources: &Sources,
    recipient: &age::x25519::Recipient,
    include_lake: bool,
) -> Result<PathBuf, crate::Error> {
    let now = Utc::now();
    let out_path = match output {
        Some(p) => p,
        None => {
            let stamp = now.format("%Y%m%dT%H%M%SZ");
            let dir = Path::new(DEFAULT_BACKUP_DIR);
            fs::create_dir_all(dir)
                .map_err(|e| crate::Error::Other(format!("creating backup dir: {e}")))?;
            dir.join(format!("virtues-{stamp}.tar.gz.age"))
        }
    };
    if out_path.exists() && !force {
        return Err(crate::Error::Other(format!(
            "{} already exists; pass --force to overwrite",
            out_path.display()
        )));
    }

    // Only the database dump is materialized, because it is the one member with
    // no original on disk. Everything else streams from where it already lives,
    // so the stage is bounded by the database, not by the archive. It is on the
    // box's own disk (`Stage`): `out_path` is often on a backup drive, which
    // must only ever hold ciphertext. `pg_dump_into` keeps the box's disk from
    // filling while it writes there.
    sweep_stale_stages();
    if let Some(dir) = out_path.parent() {
        sweep_legacy_stages(dir);
    }
    let staging = Stage::create("backup")?;
    let dump = staging.path().join("virtues.dump");

    println!("→ pg_dump (full database)…");
    pg_dump_into(&dump, &sources.database_url)?;

    let mut members: Vec<(PathBuf, String)> = vec![(dump, "db/virtues.dump".to_string())];
    match &sources.env_file {
        Some(env_file) => members.push((env_file.clone(), "env/virtues.env".to_string())),
        None => println!("→ no env file found — continuing without the encryption key"),
    }
    // Resolved, never hardcoded. A backup that read a fixed path while the box
    // wrote somewhere else would succeed, report success, and contain no lake at
    // all — the failure only surfacing at restore, when it is far too late.
    if include_lake {
        println!("→ scanning data lake at {}…", sources.lake.display());
        collect_files(&sources.lake, "lake", &mut members)?;
    }

    // Authored applets are user data with no other copy: the manifest, the
    // schema DDL, and the face HTML the model wrote. The DB row and the
    // applet's Postgres schema survive on their own, but these files don't —
    // losing them leaves exactly the half-state of a row with no folder.
    println!(
        "→ scanning authored applets at {}…",
        sources.applets.display()
    );
    collect_files(&sources.applets, "applets", &mut members)?;

    let schema_version = current_schema_version(pool).await?;
    println!(
        "→ writing {} member(s) to {}…",
        members.len(),
        out_path.display()
    );
    stream_archive(
        &members,
        ManifestMeta {
            schema_version,
            created_at: now.to_rfc3339(),
        },
        &out_path,
        recipient,
    )?;

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

/// How often a running pg_dump's disk is checked against [`STAGE_FLOOR_BYTES`].
const DUMP_WATCH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);

fn pg_dump_into(dest: &Path, database_url: &str) -> Result<(), crate::Error> {
    // `pg_dump --format=custom` produces a binary, compressed, parallel-
    // restorable archive. The URL is passed in rather than read here: it must be
    // the database the caller already holds a pool to, not whatever the ambient
    // environment happens to name.
    //
    // The dump is written to the box's own disk, which Postgres writes to at
    // the same time. So the disk is watched while the dump runs, and the dump
    // is stopped and deleted if free space falls to the floor, before Postgres
    // runs short. It is not budgeted up front: the database's size on disk
    // overstates a compressed dump several times over, and refusing on that
    // figure would stop backups on boxes that have room for them.
    let watched = dest.parent().unwrap_or(dest);
    let too_full = |free: u64| {
        crate::Error::Other(format!(
            "stopped the backup: the server's disk has {} MB free at {}, where the \
             database dump is written before it is encrypted, and a backup always \
             leaves {} MB free so the database can keep working. Free some space on \
             the server, then run the backup again.",
            free / (1024 * 1024),
            watched.display(),
            STAGE_FLOOR_BYTES / (1024 * 1024),
        ))
    };
    if let Some(free) = free_bytes_under(watched).filter(|f| *f < STAGE_FLOOR_BYTES) {
        return Err(too_full(free));
    }
    let mut child = Command::new("pg_dump")
        .arg("--format=custom")
        .arg("--no-owner")
        .arg("--no-acl")
        .arg("-f")
        .arg(dest)
        .arg(database_url)
        .spawn()
        .map_err(|e| crate::Error::Other(format!("invoking pg_dump: {e}")))?;
    loop {
        let finished = child
            .try_wait()
            .map_err(|e| crate::Error::Other(format!("waiting for pg_dump: {e}")))?;
        if let Some(status) = finished {
            if !status.success() {
                return Err(crate::Error::Other(format!(
                    "pg_dump exited with status {status}"
                )));
            }
            return Ok(());
        }
        if let Some(free) = free_bytes_under(watched).filter(|f| *f < STAGE_FLOOR_BYTES) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = fs::remove_file(dest);
            return Err(too_full(free));
        }
        std::thread::sleep(DUMP_WATCH_INTERVAL);
    }
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

/// The box's age recipient — a **public** key. The matching secret is printed
/// once, when it is minted, and never written here.
const WELL_KNOWN_RECIPIENT_PATH: &str = "/var/lib/virtues/backup-recipient";
/// Dev-only, relative to virtues-core. Gitignored alongside `.applet-state`.
const DEV_RECIPIENT_PATH_FROM_CORE: &str = "../.backup-recipient";

/// Where this box keeps its backup recipient.
///
/// Same precedence as `storage::lake::lake_root` and
/// `applet_templates::state_root`, and for the same reason: a hardcoded box path
/// makes the command unusable anywhere else. `/var/lib` is root-owned, so a dev
/// machine cannot even create the directory — `virtues backup --output /tmp/x`
/// failed outright until this resolved properly.
pub(crate) fn recipient_path() -> PathBuf {
    if let Ok(explicit) = std::env::var("VIRTUES_BACKUP_RECIPIENT") {
        if !explicit.is_empty() {
            return PathBuf::from(explicit);
        }
    }
    let installed = PathBuf::from(WELL_KNOWN_RECIPIENT_PATH);
    if installed.parent().is_some_and(|p| p.is_dir()) {
        return installed;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DEV_RECIPIENT_PATH_FROM_CORE)
}

/// Load the recipient every backup encrypts to. Never mints — see `init_key`.
///
/// The box stores only the public half, so **it cannot read its own backups.**
/// That is the point, and it is what makes "virtues never holds the key" a
/// property rather than a promise: a stolen box yields an encryption key and
/// nothing to decrypt, a stolen drive yields ciphertext and no key.
///
/// It also means the recovery secret is printed exactly once. There is no way to
/// recover it afterwards — by construction, since anywhere we could stash it for
/// later would be somewhere an attacker with the box could read it too.
///
/// The file lives beside the lake rather than inside it, so it is never swept
/// into the archive. A recovery key sealed inside the thing it unseals would be
/// no recovery key at all.
pub(crate) fn load_or_create_recipient() -> Result<age::x25519::Recipient, crate::Error> {
    load_recipient(false)
}

/// `virtues backup --init-key` — the one place a backup key is created.
///
/// Refuses when one already exists rather than replacing it: a new key does not
/// open old archives, so silently rotating would strand every backup the box has
/// ever written.
pub fn init_key() -> Result<(), crate::Error> {
    let path = recipient_path();
    if path.is_file() {
        return Err(crate::Error::Other(format!(
            "{} already exists. A new key would not open the archives this box \
             has already written, so this refuses rather than strand them. To \
             deliberately start over, remove that file first — and know that \
             every existing archive becomes unreadable.",
            path.display()
        )));
    }
    load_recipient(true)?;
    Ok(())
}

/// `mint` is the caller asserting a human is watching.
///
/// Minting prints a secret exactly once and can never reproduce it, so doing it
/// anywhere the output is not being read produces archives nobody can ever open
/// — and says nothing is wrong. That is not hypothetical: the nightly applet
/// runs with its stdout captured as a JSON contract, and a first backup from
/// cron would have minted a key straight into a void. It also happened to an
/// operator, whose terminal pipeline truncated the banner and silently cost
/// 4 GB of archives.
///
/// So auto-minting is gone. `virtues backup --init-key` is the only way to
/// create one, it refuses to overwrite, and everything else fails loudly when
/// no recipient exists.
pub(crate) fn load_recipient(mint: bool) -> Result<age::x25519::Recipient, crate::Error> {
    use std::str::FromStr;

    let path = recipient_path();
    if let Ok(existing) = fs::read_to_string(&path) {
        let existing = existing.trim();
        if !existing.is_empty() {
            return age::x25519::Recipient::from_str(existing).map_err(|e| {
                crate::Error::Other(format!(
                    "{} does not contain a valid age recipient ({e}). \
                     Refusing to back up rather than write an archive nobody can \
                     open. Fix or remove the file — removing it mints a NEW key, \
                     which will not open existing archives.",
                    path.display()
                ))
            });
        }
    }

    if !mint {
        return Err(crate::Error::Other(format!(
            "no backup key on this box ({} is missing).\n\n    \
             Run `virtues backup --init-key` from a terminal you are watching. \
             It prints a recovery key once and cannot ever print it again — so \
             it is never created automatically, and never by a scheduled run.",
            path.display()
        )));
    }

    let identity = age::x25519::Identity::generate();
    let recipient = identity.to_public();
    fs::write(&path, format!("{recipient}\n"))
        .map_err(|e| crate::Error::Other(format!("writing {}: {e}", path.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }

    use age::secrecy::ExposeSecret;
    println!();
    println!("┌─ SAVE THIS — it is shown once and cannot be recovered ─────────────");
    println!("│");
    println!("│  Backup recovery key:");
    println!("│");
    println!("│    {}", identity.to_string().expose_secret());
    println!("│");
    println!("│  This box keeps only the public half, so it CANNOT decrypt its own");
    println!("│  backups. If the box is lost, this key is the only way to read them.");
    println!("│");
    println!("│  Put it somewhere that is not this box — a password manager, or");
    println!("│  printed and filed. Restore with:");
    println!("│    virtues restore --key-file <file> <archive>");
    println!("└────────────────────────────────────────────────────────────────────");
    println!();

    Ok(recipient)
}

/// Walk `root`, recording every file as `(absolute, archive-relative)`.
///
/// Records paths only — nothing is read here. An absent root is not an error:
/// a box that has never ingested has no lake, and that is a legitimate backup.
fn collect_files(
    root: &Path,
    rel_prefix: &str,
    out: &mut Vec<(PathBuf, String)>,
) -> Result<(), crate::Error> {
    if !root.is_dir() {
        return Ok(());
    }
    let read =
        fs::read_dir(root).map_err(|e| crate::Error::Other(format!("read {}: {e}", root.display())))?;
    for entry in read {
        let entry = entry.map_err(|e| crate::Error::Other(format!("dir entry: {e}")))?;
        let path = entry.path();
        let name = entry.file_name();
        let rel = format!("{rel_prefix}/{}", name.to_string_lossy());
        if path.is_dir() {
            collect_files(&path, &rel, out)?;
        } else {
            out.push((path, rel));
        }
    }
    Ok(())
}

/// The manifest fields that are not derived from the members themselves.
struct ManifestMeta {
    schema_version: String,
    created_at: String,
}

/// A reader that digests and counts what passes through it.
///
/// The point is that a member's sha256 costs no extra read: the bytes are
/// already streaming into the tar, so they are hashed on the way. Previously
/// each artifact was `fs::read` into memory in full purely to hash it — the
/// database dump included.
struct HashingReader<R> {
    inner: R,
    hasher: Sha256,
    len: u64,
}

impl<R: Read> HashingReader<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            hasher: Sha256::new(),
            len: 0,
        }
    }
}

impl<R: Read> Read for HashingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.hasher.update(&buf[..n]);
        self.len += n as u64;
        Ok(n)
    }
}

/// The length and sha256 of everything `r` yields, in the form the manifest
/// records them. Restore and `--verify` check members with this, so both
/// compute a digest the way the writer did.
pub(crate) fn digest(r: impl Read) -> std::io::Result<(u64, String)> {
    let mut reader = HashingReader::new(r);
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok((reader.len, format!("{:x}", reader.hasher.finalize())))
}

/// Stream every member into a gzipped tar, digesting as we go, and close with
/// the manifest.
///
/// **The manifest is written last, and that is load-bearing.** It names the
/// digest of every other member, which is only known once those bytes have been
/// read — so writing it first is what forced the old staging copy to exist. Tar
/// is a sequential format with no central directory, so member order is free;
/// restore extracts the whole archive before reading the manifest and does not
/// care where in the stream it appeared. Archives written by the previous
/// implementation, with the manifest first, still restore unchanged.
fn stream_archive(
    members: &[(PathBuf, String)],
    meta: ManifestMeta,
    out: &Path,
    recipient: &age::x25519::Recipient,
) -> Result<Vec<String>, crate::Error> {
    match stream_archive_inner(members, meta, out, recipient) {
        Ok(v) => Ok(v),
        Err(e) => {
            // Otherwise every failed run leaves a `.partial` of whatever it had
            // written — on a backup volume, which is exactly where free space is
            // the binding constraint, and which nothing else reclaims.
            let _ = fs::remove_file(out.with_file_name(format!(
                "{}.partial",
                out.file_name().unwrap_or_default().to_string_lossy()
            )));
            Err(e)
        }
    }
}

fn stream_archive_inner(
    members: &[(PathBuf, String)],
    meta: ManifestMeta,
    out: &Path,
    recipient: &age::x25519::Recipient,
) -> Result<Vec<String>, crate::Error> {
    // Appended, not `with_extension`, which would replace `.age` and turn
    // `virtues-….tar.gz.age` into `virtues-….tar.gz.partial`.
    let tmp = out.with_file_name(format!(
        "{}.partial",
        out.file_name().unwrap_or_default().to_string_lossy()
    ));
    // Paths the writer ACTUALLY archived. Not the same as the paths it was
    // asked to: members that vanish mid-run are skipped, and recording those as
    // shipped would mark them backed up forever.
    let archived: Vec<String>;
    {
        let file = File::create(&tmp)
            .map_err(|e| crate::Error::Other(format!("create {}: {e}", tmp.display())))?;
        // tar → gzip → age → disk. Compress before encrypting: ciphertext does
        // not compress, so the other order would produce a much larger archive.
        let encryptor =
            age::Encryptor::with_recipients(std::iter::once(recipient as &dyn age::Recipient))
                .map_err(|e| crate::Error::Other(format!("age encryptor: {e}")))?;
        let sealed = encryptor
            .wrap_output(file)
            .map_err(|e| crate::Error::Other(format!("age wrap: {e}")))?;
        let gz = GzEncoder::new(sealed, Compression::default());
        let mut builder = tar::Builder::new(gz);
        let mut artifacts = Vec::with_capacity(members.len());

        for (src, rel) in members {
            // Open first, then size the OPEN handle. Sizing the path and then
            // opening it leaves a window in which the file changes, and tar
            // writes a header declaring a length it then cannot fill.
            let f = match File::open(src) {
                Ok(f) => f,
                // The lake is live. A file that vanished between the walk and
                // here was never part of this backup; recording it in the
                // manifest would make restore fail a digest check for a file
                // that legitimately does not exist.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    eprintln!("warning: {} disappeared during backup — skipped", src.display());
                    continue;
                }
                Err(e) => {
                    return Err(crate::Error::Other(format!("open {}: {e}", src.display())))
                }
            };
            let size = f
                .metadata()
                .map_err(|e| crate::Error::Other(format!("stat {}: {e}", src.display())))?
                .len();

            let mut header = tar::Header::new_gnu();
            header.set_size(size);
            header.set_mode(0o600);
            header.set_cksum();
            let mut reader = HashingReader::new(f);
            builder
                .append_data(&mut header, rel, &mut reader)
                .map_err(|e| crate::Error::Other(format!("archiving {rel}: {e}")))?;

            if reader.len != size {
                return Err(crate::Error::Other(format!(
                    "{} changed size while being archived ({size} → {}); backup aborted",
                    src.display(),
                    reader.len
                )));
            }
            artifacts.push(Artifact {
                path: rel.clone(),
                size_bytes: reader.len,
                sha256: format!("{:x}", reader.hasher.finalize()),
            });
        }

        archived = artifacts.iter().map(|a| a.path.clone()).collect();
        let manifest = Manifest {
            manifest_version: MANIFEST_VERSION,
            binary_version: env!("CARGO_PKG_VERSION").to_string(),
            schema_version: meta.schema_version,
            created_at: meta.created_at,
            distro: read_distro(),
            artifacts,
        };
        let json = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| crate::Error::Other(format!("encode manifest: {e}")))?;
        let mut header = tar::Header::new_gnu();
        header.set_size(json.len() as u64);
        header.set_mode(0o600);
        header.set_cksum();
        builder
            .append_data(&mut header, "manifest.json", json.as_slice())
            .map_err(|e| crate::Error::Other(format!("archiving manifest: {e}")))?;

        let mut gz = builder
            .into_inner()
            .map_err(|e| crate::Error::Other(format!("tar finalize: {e}")))?;
        gz.flush()
            .map_err(|e| crate::Error::Other(format!("flush gz: {e}")))?;
        let sealed = gz
            .finish()
            .map_err(|e| crate::Error::Other(format!("finish gz: {e}")))?;
        // Without this the age stream's final chunk is never written and the
        // archive decrypts to a truncated tar — a corruption that only surfaces
        // at restore.
        sealed
            .finish()
            .map_err(|e| crate::Error::Other(format!("finish age stream: {e}")))?;
    }
    // Atomic rename so a crashed backup never leaves a half-written file
    // at the final path.
    fs::rename(&tmp, out)
        .map_err(|e| crate::Error::Other(format!("rename to {}: {e}", out.display())))?;
    Ok(archived)
}

// ─── Staging on the box's own disk ─────────────────────────────────────────

/// Where backup and restore keep plaintext while they work: the pg_dump a
/// backup is about to encrypt, and everything a restore decrypts, the env file
/// with the box key included.
///
/// On the box's own disk, in a 0700 folder under its data dir, and never
/// beside the archive. The archive is often on a backup drive, and a drive is
/// the thing the encryption protects: it can be lost, lent, or shared with
/// another box. Deleting files does not reliably erase them from flash media,
/// so plaintext is never written to the drive at all.
///
/// Away from a box (a dev checkout, a test) there is no data dir, so the
/// folder goes in the temporary directory. macOS gives each user a temporary
/// directory of their own. On Linux it is the shared `/tmp`, so the name
/// carries the uid, and [`Stage::create_in`] refuses a folder there that
/// another user made.
pub(crate) fn staging_root() -> PathBuf {
    let data_dir = crate::data_disk::data_dir();
    if data_dir.is_dir() {
        return data_dir.join("staging");
    }
    #[cfg(target_os = "linux")]
    // SAFETY: getuid has no preconditions and cannot fail.
    let name = format!("virtues-staging-{}", unsafe { libc::getuid() });
    #[cfg(not(target_os = "linux"))]
    let name = "virtues-staging".to_string();
    std::env::temp_dir().join(name)
}

/// Free space a stage always leaves on the disk it is on. It is the same floor
/// below which the lake stops archiving (`storage::lake`): on a box the stage
/// shares its disk with Postgres and its WAL while the server runs, and both
/// need room to keep working.
pub(crate) const STAGE_FLOOR_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Free bytes on the disk that holds `path`, or its nearest existing parent.
/// Resolved through symlinks, so a data dir linked onto another disk is
/// measured on that disk. `None` when it cannot be measured.
pub(crate) fn free_bytes_under(path: &Path) -> Option<u64> {
    let existing = path.ancestors().find(|p| p.exists())?;
    crate::storage::lake::free_bytes_at(&fs::canonicalize(existing).ok()?)
}

/// One run's private folder under [`staging_root`].
///
/// Removed when dropped, when the run is interrupted before it changes
/// anything ([`clean_up_on_interrupt`]), or by the next run when this process
/// ended without doing either ([`sweep_stale_stages`]).
pub(crate) struct Stage {
    path: PathBuf,
}

impl Stage {
    /// A fresh 0700 folder named `<purpose>-<pid>-<boot>-<time>`. The pid and
    /// the boot are how a later run tells a folder whose process is gone from
    /// one in use.
    pub(crate) fn create(purpose: &str) -> Result<Self, crate::Error> {
        Self::create_in(&staging_root(), purpose)
    }

    pub(crate) fn create_in(root: &Path, purpose: &str) -> Result<Self, crate::Error> {
        use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(root)
            .map_err(|e| crate::Error::Other(format!("create {}: {e}", root.display())))?;
        // A link would put the stage wherever it points.
        let meta = fs::symlink_metadata(root)
            .map_err(|e| crate::Error::Other(format!("stat {}: {e}", root.display())))?;
        if !meta.is_dir() {
            return Err(crate::Error::Other(format!(
                "{} is not a folder. Remove it, then run this again.",
                root.display()
            )));
        }
        // An existing folder keeps the mode it was made with.
        if meta.permissions().mode() & 0o077 != 0 {
            fs::set_permissions(root, fs::Permissions::from_mode(0o700)).map_err(|e| {
                crate::Error::Other(format!(
                    "{} is readable by other users and could not be made private ({e})",
                    root.display()
                ))
            })?;
        }
        let stamp = Utc::now().format("%Y%m%dT%H%M%S%f");
        let path = root.join(format!(
            "{purpose}-{}-{}-{stamp}",
            std::process::id(),
            this_boot()
        ));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .map_err(|e| crate::Error::Other(format!("create {}: {e}", path.display())))?;
        // In a folder every user can write to, such as the shared `/tmp`,
        // another user could have made `root` first, to read what is unpacked
        // into it. The stage just made belongs to this process's user, so
        // there the two owners must match.
        let own_uid = fs::metadata(&path)
            .map_err(|e| crate::Error::Other(format!("stat {}: {e}", path.display())))?
            .uid();
        if in_shared_folder(root) && meta.uid() != own_uid {
            let _ = fs::remove_dir(&path);
            return Err(crate::Error::Other(format!(
                "{} belongs to another user. Remove it, or set TMPDIR to a folder \
                 only you can write to, then run this again.",
                root.display()
            )));
        }
        active_stages().push(path.clone());
        clean_up_on_interrupt();
        Ok(Self { path })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for Stage {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl Drop for Stage {
    fn drop(&mut self) {
        active_stages().retain(|p| p != &self.path);
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Whether every user can create entries in `path`'s parent, as in `/tmp`.
fn in_shared_folder(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.parent()
        .and_then(|p| fs::metadata(p).ok())
        .is_some_and(|m| m.permissions().mode() & 0o002 != 0)
}

/// This boot of the machine: the first part of Linux's boot id.
///
/// A restart gives old pids to new processes, so a stage made before it can
/// carry the pid of an unrelated process that is running now. Comparing boots
/// tells the two apart without trusting the clock, which a box without a
/// battery-backed clock sets late. Other systems have no boot id to read, and
/// there the pid alone decides.
fn this_boot() -> String {
    #[cfg(target_os = "linux")]
    if let Ok(id) = fs::read_to_string("/proc/sys/kernel/random/boot_id") {
        let first = id.trim().split('-').next().unwrap_or_default();
        if !first.is_empty() {
            return first.to_string();
        }
    }
    "noboot".to_string()
}

/// Remove stages left by runs that ended without removing their own: a
/// power cut, a kill, an OOM. Called at the start of every backup, restore
/// and verify. A folder whose process is still running belongs to a run in
/// progress and is left alone.
pub(crate) fn sweep_stale_stages() {
    sweep_stale_stages_in(&staging_root());
}

fn sweep_stale_stages_in(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return; // no staging folder yet
    };
    let boot = this_boot();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if stage_is_stale(&name, &boot, process_alive) == Some(true) {
            remove_left_stage(&entry.path());
        }
    }
}

/// The pid and boot in a stage's name, `<purpose>-<pid>-<boot>-<time>`.
fn stage_owner(name: &str) -> Option<(u32, &str)> {
    let mut parts = name.splitn(4, '-');
    let (_purpose, pid, boot, _time) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    Some((pid.parse().ok()?, boot))
}

/// Whether the run that made the stage `name` has ended: it ran in an earlier
/// boot than `boot`, or its process is gone. `None` when `name` is not a stage.
fn stage_is_stale(name: &str, boot: &str, alive: impl Fn(u32) -> bool) -> Option<bool> {
    let (pid, made_in) = stage_owner(name)?;
    Some(made_in != boot || !alive(pid))
}

/// Whether `pid` is a running process, whoever owns it: a restore runs as
/// root and a backup as the service user, and each must leave the other's
/// stage alone.
fn process_alive(pid: u32) -> bool {
    if pid == std::process::id() {
        return true;
    }
    if pid == 0 {
        return false;
    }
    #[cfg(target_os = "linux")]
    {
        Path::new("/proc").join(pid.to_string()).exists()
    }
    #[cfg(not(target_os = "linux"))]
    {
        // Off Linux this only runs on a dev machine. When `ps` cannot be run,
        // the folder is kept, so a run in progress never loses its stage. The
        // next sweep that can run `ps` removes it.
        Command::new("ps")
            .args(["-p", &pid.to_string()])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(true)
    }
}

/// Names earlier releases gave the stage they made beside an archive.
const LEGACY_STAGE_PREFIXES: [&str; 2] = [".virtues-backup-stage-", ".virtues-restore-stage-"];

/// Remove stages an earlier release left beside an archive: on a backup
/// drive, in the backup folder, or wherever `--output` pointed. A backup's
/// holds a plaintext dump of the database, and a restore's also holds the
/// box key. No current run makes one, so any that exists is stale.
pub(crate) fn sweep_legacy_stages(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if LEGACY_STAGE_PREFIXES.iter().any(|p| name.starts_with(p)) {
            remove_left_stage(&entry.path());
        }
    }
}

fn remove_left_stage(path: &Path) {
    match fs::remove_dir_all(path) {
        Ok(()) => println!("→ removed {}, left by a run that did not finish", path.display()),
        Err(e) => eprintln!(
            "warning: could not remove {} ({e}). A run that did not finish left it, \
             and it can hold a plaintext copy of the database and the box key. \
             Remove it by hand.",
            path.display()
        ),
    }
}

// ─── Interrupts ────────────────────────────────────────────────────────────

/// Stages this process has open, for the interrupt handler.
static ACTIVE_STAGES: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());

fn active_stages() -> std::sync::MutexGuard<'static, Vec<PathBuf>> {
    ACTIVE_STAGES.lock().unwrap_or_else(|e| e.into_inner())
}

/// What an interrupt may still do, settled between the interrupt handler and
/// the main thread, which carries on while the handler runs.
///
/// Until restore starts replacing this server's data, an interrupt removes
/// every open stage and exits, and [`begin_changes`] then refuses, so nothing
/// is changed. Once restore has started, the stages hold files it is moving
/// into place that are no longer anywhere else on the server. An interrupt
/// then exits at once, as the signal's default action would, and leaves them
/// for the next run's sweep.
static RUN_STATE: AtomicU8 = AtomicU8::new(RUNNING);
const RUNNING: u8 = 0;
const INTERRUPTED: u8 = 1;
const CHANGING: u8 = 2;

/// Called by restore just before it replaces anything. Fails when an
/// interrupt came first, so a restore that was told to stop never starts.
pub(crate) fn begin_changes() -> Result<(), crate::Error> {
    begin_changes_in(&RUN_STATE)
}

/// Called by restore once it has replaced everything, so a later interrupt
/// is not reported as cutting a restore short.
pub(crate) fn end_changes() {
    let _ = RUN_STATE.compare_exchange(CHANGING, RUNNING, Ordering::SeqCst, Ordering::SeqCst);
}

fn begin_changes_in(state: &AtomicU8) -> Result<(), crate::Error> {
    match state.compare_exchange(RUNNING, CHANGING, Ordering::SeqCst, Ordering::SeqCst) {
        Ok(_) | Err(CHANGING) => Ok(()),
        Err(_) => Err(crate::Error::Other(
            "interrupted before anything was replaced, so this server is unchanged.".into(),
        )),
    }
}

/// Whether an interrupt that arrives now may remove the open stages.
fn may_remove_stages(state: &AtomicU8) -> bool {
    match state.compare_exchange(RUNNING, INTERRUPTED, Ordering::SeqCst, Ordering::SeqCst) {
        Ok(_) | Err(INTERRUPTED) => true,
        Err(_) => false,
    }
}

/// On Ctrl-C, SIGTERM or SIGHUP, remove every open stage, then exit.
///
/// A signal's default action ends the process before any `Drop` runs, which
/// would leave the plaintext in place. Ctrl-C is how the manual says to stop
/// a restore in its five-second window, a dropped SSH session sends SIGHUP,
/// and systemd stops the nightly job with SIGTERM. Installed the first time a
/// stage is created, so a command that never stages keeps the default
/// handling. Once restore has begun replacing data, the handler leaves the
/// stages alone ([`RUN_STATE`]).
///
/// It needs the tokio runtime the CLI runs in, and runs on a worker thread
/// while the main one is busy extracting or dumping. Tests skip it: once
/// tokio takes SIGINT it never gives it back, so a handler from one test
/// would swallow Ctrl-C for the rest of the run.
fn clean_up_on_interrupt() {
    static INSTALLED: std::sync::Once = std::sync::Once::new();
    if cfg!(test) {
        return;
    }
    INSTALLED.call_once(|| {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        runtime.spawn(async {
            use tokio::signal::unix::{signal, SignalKind};
            let mut term = signal(SignalKind::terminate()).ok();
            let mut hup = signal(SignalKind::hangup()).ok();
            let code = tokio::select! {
                Ok(()) = tokio::signal::ctrl_c() => 130,
                Some(()) = next_signal(&mut term) => 143,
                Some(()) = next_signal(&mut hup) => 129,
                else => return,
            };
            if may_remove_stages(&RUN_STATE) {
                remove_active_stages();
            } else {
                eprintln!(
                    "\ninterrupted while replacing this server's data, so the restore \
                     is not complete. Run it again. The next backup or restore removes \
                     what this one left in {}.",
                    staging_root().display()
                );
            }
            std::process::exit(code);
        });
    });
}

async fn next_signal(signal: &mut Option<tokio::signal::unix::Signal>) -> Option<()> {
    match signal {
        Some(s) => s.recv().await,
        None => std::future::pending().await,
    }
}

fn remove_active_stages() {
    let stages = std::mem::take(&mut *active_stages());
    for path in stages {
        eprintln!("\ninterrupted. Removing {}…", path.display());
        // The main thread may still be writing into it, so a first pass can
        // race a new file. A few passes outlast that; anything left after
        // the exit is swept by the next run.
        for _ in 0..3 {
            if fs::remove_dir_all(&path).is_ok() || !path.exists() {
                break;
            }
        }
        if path.exists() {
            eprintln!(
                "could not remove {}; the next backup or restore removes it.",
                path.display()
            );
        }
    }
}

// ─── Entry points for volume-targeted backups (see cli/backup_volume.rs) ────

/// Walk a tree into `(absolute, archive-relative)` pairs.
///
/// Same walker the archive writer uses, exposed so the volume path can diff a
/// lake against what a drive already holds without a second implementation.
pub(crate) fn collect_files_pub(
    root: &Path,
    rel_prefix: &str,
    out: &mut Vec<(PathBuf, String)>,
) -> Result<(), crate::Error> {
    collect_files(root, rel_prefix, out)
}

/// Write a full archive to an explicit path.
pub(crate) async fn write_archive_to(
    pool: &PgPool,
    dest: &Path,
    sources: &Sources,
    recipient: &age::x25519::Recipient,
) -> Result<PathBuf, crate::Error> {
    write_archive(pool, Some(dest.to_path_buf()), true, sources, recipient, false).await
}

/// Archive an arbitrary member list — used for lake increments.
///
/// `schema_version` is "unknown" rather than the live one on purpose: an
/// increment carries no database, so claiming a schema would invite restore's
/// compatibility gate to reason about one that is not there.
pub(crate) fn write_members(
    members: &[(PathBuf, String)],
    dest: &Path,
    recipient: &age::x25519::Recipient,
    label: &str,
) -> Result<Vec<String>, crate::Error> {
    println!("→ writing {} ({} member(s))…", label, members.len());
    stream_archive(
        members,
        ManifestMeta {
            schema_version: "unknown".to_string(),
            created_at: Utc::now().to_rfc3339(),
        },
        dest,
        recipient,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn a_backup_carries_the_env_file_only_when_it_holds_the_key() {
        let dir = tempfile::tempdir().unwrap();
        let keyed = dir.path().join("keyed.env");
        fs::write(&keyed, "DATABASE_URL=postgres:///virtues\nVIRTUES_ENCRYPTION_KEY=abc=\n").unwrap();
        let keyless = dir.path().join("keyless.env");
        fs::write(&keyless, "VIRTUES_PUBLIC_URL=https://box.example.com\n").unwrap();
        let absent = dir.path().join("absent.env");

        assert_eq!(env_file_to_archive(&keyed, false).unwrap(), Some(keyed.clone()));

        let err = env_file_to_archive(&keyless, false).unwrap_err().to_string();
        assert!(err.contains("has no VIRTUES_ENCRYPTION_KEY"), "{err}");
        let err = env_file_to_archive(&absent, false).unwrap_err().to_string();
        assert!(err.contains("no env file at"), "{err}");

        // The explicit out archives no env file, never a keyless one.
        assert_eq!(env_file_to_archive(&keyless, true).unwrap(), None);
        assert_eq!(env_file_to_archive(&absent, true).unwrap(), None);
    }

    #[test]
    fn a_stage_is_private_and_goes_when_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("staging");
        let stage = Stage::create_in(&root, "backup").unwrap();
        let path = stage.path().to_path_buf();
        assert!(path.starts_with(&root));
        assert_eq!(mode(&root), 0o700, "the staging folder is readable by others");
        assert_eq!(mode(&path), 0o700, "a stage is readable by others");
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let boot = this_boot();
        assert_eq!(stage_owner(&name), Some((std::process::id(), boot.as_str())));
        fs::write(path.join("virtues.dump"), "plaintext").unwrap();
        drop(stage);
        assert!(!path.exists(), "a dropped stage was left behind");

        // A folder someone made with a wider mode is tightened.
        let loose = dir.path().join("loose");
        fs::create_dir(&loose).unwrap();
        fs::set_permissions(&loose, fs::Permissions::from_mode(0o755)).unwrap();
        let _stage = Stage::create_in(&loose, "restore").unwrap();
        assert_eq!(mode(&loose), 0o700);
    }

    #[test]
    fn a_sweep_removes_stages_whose_run_is_gone_and_keeps_live_ones() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let live = Stage::create_in(&root, "backup").unwrap();

        // A pid that existed and has exited: a child we already waited on.
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let dead = child.id();
        child.wait().unwrap();
        let stale = root.join(format!("restore-{dead}-{}-20260101T000000000000", this_boot()));
        fs::create_dir_all(stale.join("env")).unwrap();
        fs::write(stale.join("env/virtues.env"), "VIRTUES_ENCRYPTION_KEY=invented\n").unwrap();
        let unrelated = root.join("notes");
        fs::create_dir(&unrelated).unwrap();

        sweep_stale_stages_in(&root);
        assert!(!stale.exists(), "a stage from a finished process survived the sweep");
        assert!(live.path().exists(), "the sweep removed a stage still in use");
        assert!(unrelated.exists(), "the sweep removed something that is not a stage");
    }

    #[test]
    fn a_stage_from_an_earlier_boot_is_stale_even_when_its_pid_is_running() {
        let me = std::process::id();
        let alive = |_: u32| true;
        let gone = |_: u32| false;
        let now = format!("restore-{me}-1a2b3c4d-20261027T040000000000000");
        assert_eq!(stage_is_stale(&now, "1a2b3c4d", alive), Some(false));
        assert_eq!(stage_is_stale(&now, "1a2b3c4d", gone), Some(true));
        // After a restart the same pid can belong to an unrelated process.
        assert_eq!(stage_is_stale(&now, "9f8e7d6c", alive), Some(true));
        assert_eq!(stage_is_stale("notes", "1a2b3c4d", alive), None);
        assert_eq!(stage_is_stale("restore-notapid-1a2b3c4d-x", "1a2b3c4d", gone), None);
    }

    #[test]
    fn a_staging_folder_that_is_a_link_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let elsewhere = dir.path().join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        let root = dir.path().join("staging");
        std::os::unix::fs::symlink(&elsewhere, &root).unwrap();
        let err = Stage::create_in(&root, "restore").err().expect("a linked staging folder was used");
        assert!(err.to_string().contains("is not a folder"), "{err}");
        assert_eq!(fs::read_dir(&elsewhere).unwrap().count(), 0, "a stage was made through the link");

        // Only a folder in a directory every user can write to is checked for
        // its owner.
        let shared = dir.path().join("shared");
        fs::create_dir(&shared).unwrap();
        fs::set_permissions(&shared, fs::Permissions::from_mode(0o1777)).unwrap();
        assert!(in_shared_folder(&shared.join("virtues-staging")));
        assert!(!in_shared_folder(&dir.path().join("virtues-staging")));
        // A folder this user made there is accepted.
        let _stage = Stage::create_in(&shared.join("virtues-staging"), "restore").unwrap();
    }

    #[test]
    fn an_interrupt_before_restore_starts_changing_stops_it_and_one_after_does_not() {
        // Interrupted first: the stages go, and the restore never starts.
        let state = AtomicU8::new(RUNNING);
        assert!(may_remove_stages(&state));
        assert!(may_remove_stages(&state), "a second signal changed the answer");
        let err = begin_changes_in(&state).expect_err("a restore started after an interrupt");
        assert!(err.to_string().contains("unchanged"), "{err}");

        // Started first: the stages hold files being moved into place, so an
        // interrupt leaves them.
        let state = AtomicU8::new(RUNNING);
        begin_changes_in(&state).unwrap();
        begin_changes_in(&state).expect("a second step of the same restore was refused");
        assert!(!may_remove_stages(&state));
    }

    #[test]
    fn stages_an_earlier_release_left_beside_an_archive_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        let archives = dir.path();
        for name in [".virtues-backup-stage-20260101T0000", ".virtues-restore-stage-20260101T0000"] {
            fs::create_dir_all(archives.join(name).join("db")).unwrap();
            fs::write(archives.join(name).join("db/virtues.dump"), "PGDMP").unwrap();
        }
        fs::write(archives.join("full-20260101T000000Z.tar.gz.age"), "ciphertext").unwrap();

        sweep_legacy_stages(archives);
        let left: Vec<String> = fs::read_dir(archives)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, ["full-20260101T000000Z.tar.gz.age"]);
    }
}

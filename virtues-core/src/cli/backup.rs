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
//! **Nothing is staged.** Members stream from where they already live, digested
//! in flight. Only the pg_dump is materialized, because it has no original on
//! disk. The manifest goes last precisely because it names digests that are not
//! known until the bytes have been read — writing it first is what used to force
//! a full copy of the lake onto the disk beside the archive.
//!
//! **The box cannot read what it writes.** Archives are encrypted to an age
//! recipient whose secret half was printed once and never stored. A stolen box
//! yields an encryption key with nothing to decrypt; a stolen drive yields
//! ciphertext with no key. Restore therefore needs `--key-file`, and that is
//! working as intended rather than an oversight.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::Utc;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

const DEFAULT_BACKUP_DIR: &str = "/var/lib/virtues/backups";
const MANIFEST_VERSION: u32 = 1;

/// Candidate locations for the env file that holds `VIRTUES_ENCRYPTION_KEY`,
/// in preference order.
///
/// FHS says config lives in `/etc`, and the rest of the codebase (restore,
/// diag, auth docs) assumes `/etc/virtues/env`. The installer disagreed: it
/// writes `<data_dir>/virtues.env` and points the unit's `EnvironmentFile=`
/// there. Every box built by that installer therefore has NO `/etc/virtues/env`
/// — so a backup that only looked there captured no key at all, and said so
/// with a `tracing::warn!` nobody sees. The resulting tarball has a perfect
/// manifest, a full pg_dump, and no way to decrypt a single credential.
///
/// Read both until the installer is fixed; prefer the FHS path so boxes that
/// have migrated win.
pub const ENV_CANDIDATES: [&str; 2] = ["/etc/virtues/env", "/var/lib/virtues/virtues.env"];

/// First existing env file, or `None` when the box has none.
///
/// `VIRTUES_ENV_FILE` overrides both candidates — the install prefix is
/// configurable (`DATA_DIR`), and a box that moved its data dir would
/// otherwise match neither path. It now fails loudly rather than backing up
/// without a key, so an operator in that position gets told which paths were
/// tried; this is the knob that lets them answer.
pub fn find_env_file() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("VIRTUES_ENV_FILE") {
        let p = PathBuf::from(explicit);
        return p.is_file().then_some(p);
    }
    ENV_CANDIDATES
        .iter()
        .map(PathBuf::from)
        .find(|p| p.is_file())
}

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
        // The encryption key is not optional baggage — without it every encrypted
        // column in the dump is permanently unreadable. A keyless backup is worse
        // than no backup, because it looks complete and you only discover
        // otherwise at restore time, which is the worst possible moment. So this
        // is a hard failure, not a warning; `--allow-missing-key` is the explicit
        // out for dev boxes that keep their key in a repo `.env`.
        let env_file = match find_env_file() {
            Some(f) => Some(f),
            None if allow_missing_key => {
                eprintln!(
                    "warning: this backup CANNOT decrypt the database it contains \
                     (--allow-missing-key was passed)"
                );
                None
            }
            None => {
                return Err(crate::Error::Other(format!(
                    "no env file at any of {} — the backup would contain an \
                     undecryptable database. Locate the file holding \
                     VIRTUES_ENCRYPTION_KEY, or pass --allow-missing-key if you \
                     genuinely want a keyless dump.",
                    ENV_CANDIDATES.join(" or ")
                )));
            }
        };
        Ok(Self {
            database_url: crate::database::normalize_database_url()
                .map_err(|e| crate::Error::Other(format!("DATABASE_URL: {e}")))?,
            lake: crate::storage::lake::lake_root(),
            applets: crate::action_templates::state_root(),
            env_file,
        })
    }
}

pub async fn run(
    pool: &PgPool,
    output: Option<PathBuf>,
    force: bool,
    allow_missing_key: bool,
) -> Result<PathBuf, crate::Error> {
    let sources = Sources::from_env(allow_missing_key)?;
    let recipient = load_or_create_recipient()?;
    write_archive(pool, output, force, &sources, &recipient).await
}

pub(crate) async fn write_archive(
    pool: &PgPool,
    output: Option<PathBuf>,
    force: bool,
    sources: &Sources,
    recipient: &age::x25519::Recipient,
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
    // no original on disk. Everything else streams from where it already lives.
    //
    // The old path staged a full COPY of the lake beside the output before
    // tarring it, so a backup needed twice the lake's size in free space and
    // wrote every byte twice — on the box least able to afford either. The
    // staging dir is now bounded by the database, not by the archive.
    let staging = tempfile_dir(&out_path)?;
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
    println!("→ scanning data lake at {}…", sources.lake.display());
    collect_files(&sources.lake, "lake", &mut members)?;

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

fn pg_dump_into(dest: &Path, database_url: &str) -> Result<(), crate::Error> {
    // `pg_dump --format=custom` produces a binary, compressed, parallel-
    // restorable archive. The URL is passed in rather than read here: it must be
    // the database the caller already holds a pool to, not whatever the ambient
    // environment happens to name.
    let status = Command::new("pg_dump")
        .arg("--format=custom")
        .arg("--no-owner")
        .arg("--no-acl")
        .arg("-f")
        .arg(dest)
        .arg(database_url)
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

/// The box's age recipient — a **public** key. The matching secret is printed
/// once, when it is minted, and never written here.
pub(crate) const RECIPIENT_PATH: &str = "/var/lib/virtues/backup-recipient";

/// Load the recipient every backup encrypts to, minting one on first use.
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
fn load_or_create_recipient() -> Result<age::x25519::Recipient, crate::Error> {
    use std::str::FromStr;

    if let Ok(existing) = fs::read_to_string(RECIPIENT_PATH) {
        let existing = existing.trim();
        if !existing.is_empty() {
            return age::x25519::Recipient::from_str(existing).map_err(|e| {
                crate::Error::Other(format!(
                    "{RECIPIENT_PATH} does not contain a valid age recipient ({e}). \
                     Refusing to back up rather than write an archive nobody can \
                     open. Fix or remove the file — removing it mints a NEW key, \
                     which will not open existing archives."
                ))
            });
        }
    }

    let identity = age::x25519::Identity::generate();
    let recipient = identity.to_public();
    fs::write(RECIPIENT_PATH, format!("{recipient}\n"))
        .map_err(|e| crate::Error::Other(format!("writing {RECIPIENT_PATH}: {e}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(RECIPIENT_PATH, fs::Permissions::from_mode(0o600));
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
) -> Result<(), crate::Error> {
    // Appended, not `with_extension`, which would replace `.age` and turn
    // `virtues-….tar.gz.age` into `virtues-….tar.gz.partial`.
    let tmp = out.with_file_name(format!(
        "{}.partial",
        out.file_name().unwrap_or_default().to_string_lossy()
    ));
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
    write_archive(pool, Some(dest.to_path_buf()), true, sources, recipient).await
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
) -> Result<(), crate::Error> {
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

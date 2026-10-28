//! The box env file: `KEY=value` lines the installer writes and systemd hands
//! the server as its `EnvironmentFile`.
//!
//! The server reads its environment once, at start. Some settings change while
//! it runs: when a search model change finishes, the indexer promotes the new
//! endpoint in this file, and every process has to follow it without a restart.
//! So settings that can change underneath a running server are read through
//! [`var`], which prefers this file to the process environment. Everything
//! else keeps reading `std::env`.
//!
//! Values are written the way the installer writes them: bare, or double-quoted
//! with `\` and `"` escaped (a prompt prefix has spaces and a `|`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// The data dir when the install manifest names none.
const DEFAULT_DATA_DIR: &str = "/var/lib/virtues";

/// The box env file: `<data dir>/virtues.env`, where the installer wrote it
/// and where the server unit's `EnvironmentFile=` points. The data dir comes
/// from the install manifest, since an install can put it elsewhere
/// (`DATA_DIR`); `VIRTUES_ENV_FILE` overrides both, for tests.
///
/// This answers "where is the file that holds `VIRTUES_ENCRYPTION_KEY`".
/// Backup archives this file, restore writes it, the CLI loads it at start
/// ([`path_for_startup`]), and the CLI's switch to the service user looks for
/// it ([`installed_path`]). A reader with its own list of paths can disagree
/// with the server about which file holds the key: a backup then archives a
/// file without the key, or a restore writes the key where systemd never
/// reads it.
///
/// TODO(2026-10-27): migrate the applet runtime (`applets/src/lib.rs`) and
/// `relay.rs` onto this. Both still read `/var/lib/virtues/virtues.env`
/// directly, which is wrong on an install that moved its data dir.
pub fn path() -> PathBuf {
    resolve(crate::install_manifest::get().as_ref())
}

/// [`path`] without the `VIRTUES_ENV_FILE` override: where the installer put
/// the file on this machine. For questions about the install itself, such as
/// whether this machine is a box, which a test's override must not answer.
pub fn installed_path() -> PathBuf {
    resolve_from(None, crate::install_manifest::get().as_ref())
}

/// [`path`], for the CLI's own env load, which runs before anything else.
///
/// The manifest's location can depend on `INSTALL_PREFIX`, and a command run
/// from a shell only learns that from this file. `install_manifest::get`
/// caches its first answer for the life of the process, so asking it this
/// early would fix the manifest as absent on a prefixed install. This reads
/// the manifest without caching it.
pub fn path_for_startup() -> PathBuf {
    let manifest = crate::install_manifest::load_from(&crate::install_manifest::manifest_path());
    resolve(manifest.as_ref())
}

fn resolve(manifest: Option<&crate::install_manifest::InstallManifest>) -> PathBuf {
    resolve_from(std::env::var_os("VIRTUES_ENV_FILE"), manifest)
}

/// The rule itself, with the override passed in so a test need not set a
/// process-wide variable.
fn resolve_from(
    explicit: Option<std::ffi::OsString>,
    manifest: Option<&crate::install_manifest::InstallManifest>,
) -> PathBuf {
    if let Some(p) = explicit.filter(|p| !p.is_empty()) {
        return PathBuf::from(p);
    }
    manifest
        .and_then(|m| m.data_dir.clone())
        .unwrap_or_else(|| PathBuf::from(DEFAULT_DATA_DIR))
        .join("virtues.env")
}

/// Every `KEY=value` in the file, values unquoted. `None` when the file is
/// missing or unreadable, which on a dev machine is the normal case.
pub fn read(path: &Path) -> Option<HashMap<String, String>> {
    let contents = std::fs::read_to_string(path).ok()?;
    let mut out = HashMap::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            out.insert(k.trim().to_string(), unquote(v.trim()));
        }
    }
    Some(out)
}

/// The variable that holds the box key, the one key that seals every row in
/// `credentials`.
pub const ENCRYPTION_KEY: &str = "VIRTUES_ENCRYPTION_KEY";

/// The box key in an env file at `path`, if the file has a non-empty one.
/// Takes a path because backup and restore also read env files that are not
/// this box's: the copy inside an archive.
pub fn encryption_key(path: &Path) -> Option<String> {
    read(path)?.remove(ENCRYPTION_KEY).filter(|v| !v.trim().is_empty())
}

/// One key from the file, if the file has it and it's non-empty.
pub fn get(key: &str) -> Option<String> {
    read(&path())?.remove(key).filter(|v| !v.is_empty())
}

/// A setting that can change while the server runs. The box env file wins when
/// it exists, because it is newer than this process's environment; without one
/// (a dev checkout), the process environment is all there is.
///
/// A key the file does NOT have is absent, even if the process environment
/// still carries it: the file dropping a key is how a change removes one.
pub fn var(key: &str) -> Option<String> {
    match read(&path()) {
        Some(mut file) => file.remove(key),
        None => std::env::var(key).ok(),
    }
    .filter(|v| !v.trim().is_empty())
}

fn unquote(v: &str) -> String {
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        let inner = &v[1..v.len() - 1];
        let mut out = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                if let Some(n) = chars.next() {
                    out.push(n);
                }
            } else {
                out.push(c);
            }
        }
        out
    } else {
        v.to_string()
    }
}

/// Quote a value for the file when it needs it, the way the installer does.
pub fn quote(v: &str) -> String {
    if v.is_empty() || v.chars().any(|c| c.is_whitespace() || "\"'\\|#$`".contains(c)) {
        format!("\"{}\"", v.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        v.to_string()
    }
}

/// Rewrite the file with `set` upserted and `unset` removed, keeping every
/// other line (comments, order, the encryption key) exactly as it was.
/// Values in `set` are raw; they are quoted here when they need it. Written to
/// a sibling and renamed, so a crash mid-write never leaves half a file.
///
/// A missing file is not created: on a dev machine there is no box env, and the
/// caller has already applied the change to its own process.
pub fn edit(path: &Path, set: &[(&str, String)], unset: &[&str]) -> Result<bool> {
    let Ok(file) = std::fs::File::open(path) else {
        return Ok(false);
    };
    // Root (`virtues upgrade`) and the service user (the indexer promoting a
    // model change) both rewrite this file. Without a lock, two read-modify-
    // writes interleave and one loses its change. Held until return; the
    // rename replaces the inode, so a waiter re-reads the new file below.
    let _lock = Flock::exclusive(&file)?;
    let contents = std::fs::read_to_string(path)
        .map_err(|e| Error::Other(format!("read {}: {e}", path.display())))?;
    let key_of = |l: &str| l.trim_start().split_once('=').map(|(k, _)| k.trim().to_string());
    let mut written: Vec<&str> = Vec::new();
    let mut out: Vec<String> = Vec::new();
    for line in contents.lines() {
        match key_of(line) {
            Some(k) if unset.contains(&k.as_str()) => {}
            Some(k) => match set.iter().find(|(sk, _)| *sk == k) {
                Some((sk, v)) => {
                    out.push(format!("{sk}={}", quote(v)));
                    written.push(sk);
                }
                None => out.push(line.to_string()),
            },
            None => out.push(line.to_string()),
        }
    }
    for (k, v) in set {
        if !written.contains(k) {
            out.push(format!("{k}={}", quote(v)));
        }
    }
    let mut body = out.join("\n");
    body.push('\n');

    // The file holds the encryption key: the copy is created 0600 (never
    // readable by anyone else, not even for a moment), takes the original's
    // owner, and has a name of its own so a writer that died leaves nothing in
    // another's way.
    use std::io::Write;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    let meta = file.metadata().map_err(|e| Error::Other(format!("stat {}: {e}", path.display())))?;
    let tmp = path.with_extension(format!("env.{}.tmp", std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    let written = (|| -> std::io::Result<()> {
        let mut out = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
        out.write_all(body.as_bytes())?;
        out.sync_all()?;
        std::os::unix::fs::chown(&tmp, Some(meta.uid()), Some(meta.gid()))?;
        std::fs::rename(&tmp, path)
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(Error::Other(format!("rewrite {}: {e}", path.display())));
    }
    Ok(true)
}

/// An exclusive lock on an open file (`flock` on Unix), released on drop.
struct Flock<'a>(&'a std::fs::File);

impl<'a> Flock<'a> {
    fn exclusive(file: &'a std::fs::File) -> Result<Self> {
        file.lock().map_err(|e| Error::Other(format!("lock the box env file: {e}")))?;
        Ok(Self(file))
    }
}

impl Drop for Flock<'_> {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_keeps_other_lines_and_round_trips_quotes() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("virtues.env");
        std::fs::write(
            &p,
            "# header\nVIRTUES_ENCRYPTION_KEY=abc=\nVIRTUES_EMBED_URL=http://127.0.0.1:18181\nVIRTUES_EMBED_DIMS=256\n",
        )
        .unwrap();
        edit(
            &p,
            &[
                ("VIRTUES_EMBED_URL", "http://127.0.0.1:18183".into()),
                ("VIRTUES_EMBED_QUERY_PROMPT", "task: search result | query: ".into()),
            ],
            &["VIRTUES_EMBED_DIMS"],
        )
        .unwrap();
        let body = std::fs::read_to_string(&p).unwrap();
        assert!(body.starts_with("# header\nVIRTUES_ENCRYPTION_KEY=abc=\n"), "{body}");
        assert!(!body.contains("DIMS"), "{body}");
        let vars = read(&p).unwrap();
        assert_eq!(vars["VIRTUES_EMBED_URL"], "http://127.0.0.1:18183");
        assert_eq!(vars["VIRTUES_EMBED_QUERY_PROMPT"], "task: search result | query: ");
        assert_eq!(vars["VIRTUES_ENCRYPTION_KEY"], "abc=");
    }

    #[test]
    fn the_env_file_follows_the_manifest_data_dir_unless_overridden() {
        use crate::install_manifest::InstallManifest;
        assert_eq!(resolve_from(None, None), PathBuf::from("/var/lib/virtues/virtues.env"));
        let moved = InstallManifest { data_dir: Some(PathBuf::from("/srv/virtues")), ..Default::default() };
        assert_eq!(resolve_from(None, Some(&moved)), PathBuf::from("/srv/virtues/virtues.env"));
        let no_dir = InstallManifest::default();
        assert_eq!(resolve_from(None, Some(&no_dir)), PathBuf::from("/var/lib/virtues/virtues.env"));
        assert_eq!(
            resolve_from(Some("/tmp/drill.env".into()), Some(&moved)),
            PathBuf::from("/tmp/drill.env")
        );
        assert_eq!(resolve_from(Some("".into()), Some(&moved)), PathBuf::from("/srv/virtues/virtues.env"));
    }

    #[test]
    fn a_missing_file_is_left_missing() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("absent.env");
        assert!(!edit(&p, &[("K", "v".into())], &[]).unwrap());
        assert!(!p.exists());
    }
}

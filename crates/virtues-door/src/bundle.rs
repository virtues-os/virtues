//! The bundle directory: the only thing the door reads.
//!
//! ```text
//! <root>/<token>/index.html   the frozen page
//! <root>/<token>/meta.json    expiry
//! ```
//!
//! The core writes it ([`write`], [`remove`]) and the door reads it
//! ([`Store::load`]). Revoking is removing the directory; there is no other
//! copy. Writes land in a hidden temporary directory and are renamed into
//! place, so the door never serves half a page.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::token;

/// The largest page the door will serve. A page past this is carrying video
/// or unoptimized images, and every byte of it crosses the relay.
pub const MAX_PAGE_BYTES: u64 = 8 * 1024 * 1024;

const PAGE_FILE: &str = "index.html";
const META_FILE: &str = "meta.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Meta {
    /// Seconds since the epoch after which the page is gone. `None` never
    /// expires.
    #[serde(default)]
    pub expires_at_unix: Option<i64>,
}

impl Meta {
    fn expired(&self, now_unix: i64) -> bool {
        self.expires_at_unix.is_some_and(|t| now_unix >= t)
    }
}

fn now_unix() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// A read-only view of a bundle directory.
#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The page for `token`, or `None` if it is malformed, absent, expired,
    /// oversized, or reached through a symlink. Every one of those reads the
    /// same to the caller, so a visitor learns nothing about which it was.
    pub fn load(&self, token: &str) -> Option<Vec<u8>> {
        self.load_at(token, now_unix())
    }

    fn load_at(&self, token: &str, now: i64) -> Option<Vec<u8>> {
        if !token::is_valid(token) {
            return None;
        }
        let dir = self.root.join(token);
        // `symlink_metadata` does not follow links: a bundle, or the page in
        // it, that is a symlink is refused rather than followed somewhere the
        // owner never published.
        if !std::fs::symlink_metadata(&dir).ok()?.file_type().is_dir() {
            return None;
        }
        let meta: Meta = match std::fs::symlink_metadata(dir.join(META_FILE)) {
            Ok(m) if m.file_type().is_file() => {
                serde_json::from_slice(&std::fs::read(dir.join(META_FILE)).ok()?).ok()?
            }
            _ => return None,
        };
        if meta.expired(now) {
            return None;
        }
        let page = std::fs::symlink_metadata(dir.join(PAGE_FILE)).ok()?;
        if !page.file_type().is_file() || page.len() > MAX_PAGE_BYTES {
            return None;
        }
        std::fs::read(dir.join(PAGE_FILE)).ok()
    }
}

fn scratch_name(kind: &str, token: &str) -> String {
    format!(".{kind}-{token}-{}", token::generate())
}

/// Publish `page` under `token`, replacing what was there. Atomic per bundle:
/// a reader sees the old page or the new one, never a mix.
pub fn write(root: &Path, token: &str, page: &[u8], meta: &Meta) -> Result<()> {
    if !token::is_valid(token) {
        bail!("not a publication token");
    }
    if page.len() as u64 > MAX_PAGE_BYTES {
        bail!("the page is {} KB; the limit is {} KB", page.len() / 1024, MAX_PAGE_BYTES / 1024);
    }
    std::fs::create_dir_all(root).with_context(|| format!("create {}", root.display()))?;
    let tmp = root.join(scratch_name("tmp", token));
    std::fs::create_dir(&tmp).context("create a scratch bundle")?;
    let filled = (|| -> Result<()> {
        std::fs::write(tmp.join(PAGE_FILE), page)?;
        std::fs::write(tmp.join(META_FILE), serde_json::to_vec(meta)?)?;
        Ok(())
    })();
    if let Err(e) = filled {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e.context("write the bundle"));
    }

    let dest = root.join(token);
    // A directory cannot be renamed over a non-empty one, so an update moves
    // the old bundle aside first and removes it after the new one is live.
    let old = if dest.exists() {
        let old = root.join(scratch_name("old", token));
        std::fs::rename(&dest, &old).context("move the previous bundle aside")?;
        Some(old)
    } else {
        None
    };
    if let Err(e) = std::fs::rename(&tmp, &dest) {
        if let Some(old) = &old {
            let _ = std::fs::rename(old, &dest);
        }
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(anyhow::Error::new(e).context("put the bundle in place"));
    }
    if let Some(old) = old {
        let _ = std::fs::remove_dir_all(old);
    }
    Ok(())
}

/// Unpublish `token`. Removing an absent bundle is not an error.
pub fn remove(root: &Path, token: &str) -> Result<()> {
    if !token::is_valid(token) {
        bail!("not a publication token");
    }
    match std::fs::remove_dir_all(root.join(token)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(anyhow::Error::new(e).context("remove the bundle")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_then_load_then_remove() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let t = token::generate();
        write(dir.path(), &t, b"v1", &Meta::default()).unwrap();
        assert_eq!(store.load(&t).as_deref(), Some(&b"v1"[..]));
        write(dir.path(), &t, b"v2", &Meta::default()).unwrap();
        assert_eq!(store.load(&t).as_deref(), Some(&b"v2"[..]));
        remove(dir.path(), &t).unwrap();
        assert_eq!(store.load(&t), None);
        remove(dir.path(), &t).unwrap();
        // Nothing left behind but the root.
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn an_expired_page_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let t = token::generate();
        write(dir.path(), &t, b"x", &Meta { expires_at_unix: Some(1_000) }).unwrap();
        assert_eq!(store.load_at(&t, 999).as_deref(), Some(&b"x"[..]));
        assert_eq!(store.load_at(&t, 1_000), None);
    }

    #[test]
    fn a_token_cannot_reach_outside_the_root() {
        let outer = tempfile::tempdir().unwrap();
        let root = outer.path().join("pub");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(outer.path().join("secret"), b"private").unwrap();
        let store = Store::new(&root);
        for bad in ["..", "../secret", "../../etc/passwd", "", "."] {
            assert_eq!(store.load(bad), None, "{bad:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_bundle_or_page_is_refused() {
        let outer = tempfile::tempdir().unwrap();
        let root = outer.path().join("pub");
        std::fs::create_dir(&root).unwrap();
        let elsewhere = outer.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        std::fs::write(elsewhere.join("index.html"), b"private").unwrap();
        std::fs::write(elsewhere.join("meta.json"), b"{}").unwrap();
        let store = Store::new(&root);

        // The whole bundle is a link to a directory nobody published.
        let t1 = token::generate();
        std::os::unix::fs::symlink(&elsewhere, root.join(&t1)).unwrap();
        assert_eq!(store.load(&t1), None);

        // A real bundle whose page is a link out.
        let t2 = token::generate();
        std::fs::create_dir(root.join(&t2)).unwrap();
        std::fs::write(root.join(&t2).join("meta.json"), b"{}").unwrap();
        std::os::unix::fs::symlink(elsewhere.join("index.html"), root.join(&t2).join("index.html"))
            .unwrap();
        assert_eq!(store.load(&t2), None);
    }

    #[test]
    fn a_bundle_without_meta_or_with_bad_meta_is_not_served() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let t = token::generate();
        std::fs::create_dir(dir.path().join(&t)).unwrap();
        std::fs::write(dir.path().join(&t).join("index.html"), b"x").unwrap();
        assert_eq!(store.load(&t), None);
        std::fs::write(dir.path().join(&t).join("meta.json"), b"not json").unwrap();
        assert_eq!(store.load(&t), None);
    }

    #[test]
    fn writes_refuse_bad_tokens_and_oversized_pages() {
        let dir = tempfile::tempdir().unwrap();
        assert!(write(dir.path(), "../x", b"x", &Meta::default()).is_err());
        let big = vec![b'x'; MAX_PAGE_BYTES as usize + 1];
        assert!(write(dir.path(), &token::generate(), &big, &Meta::default()).is_err());
    }
}

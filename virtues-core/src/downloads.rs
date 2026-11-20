//! Downloading a file whose size and sha256 are known in advance, and
//! unpacking a tar archive that may hold no links. Map files (maps/sync.rs)
//! and home background styles (api/backgrounds.rs) both come down this way.
//!
//! No `{e}` of a reqwest error is formatted with its URL: a map file's URL
//! names a square of the map, which is location data.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{bail, Result};
use futures::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

/// What a download should be: its size and sha256, and a word for it in
/// errors ("map", "background").
pub struct Expected<'a> {
    pub bytes: u64,
    pub sha256: &'a str,
    pub what: &'a str,
}

/// Download `url` into `tmp`, resuming a partial `tmp`, check its size and
/// sha256, and rename it to `dest`. A rename swaps the inode, so a reader
/// holding the old file keeps working. `progress`, when given, holds the
/// bytes on disk so far.
pub async fn verified(
    http: &reqwest::Client,
    url: &str,
    bearer: Option<&str>,
    tmp: &Path,
    dest: &Path,
    expected: &Expected<'_>,
    progress: Option<&AtomicU64>,
) -> Result<()> {
    let what = expected.what;
    let have = tokio::fs::metadata(tmp).await.map(|m| m.len()).unwrap_or(0);
    let mut req = http.get(url);
    if let Some(b) = bearer {
        req = req.bearer_auth(b);
    }
    if have > 0 && have < expected.bytes {
        req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
    }
    let resp = req.send().await.map_err(|e| anyhow::anyhow!("{what} download request: {}", e.without_url()))?;
    let status = resp.status();
    let mut resuming = status == reqwest::StatusCode::PARTIAL_CONTENT;
    if !status.is_success() {
        bail!("{what} download answered {status}");
    }
    if resuming {
        // Append only where we asked to start; anything else starts over.
        let starts_at_have = resp
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with(&format!("bytes {have}-")));
        if !starts_at_have {
            let _ = tokio::fs::remove_file(tmp).await;
            bail!("{what} download resumed at the wrong offset; starting over next time");
        }
    } else {
        resuming = false;
    }
    let mut out = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(resuming)
        .truncate(!resuming)
        .open(tmp)
        .await?;
    let mut written = if resuming { have } else { 0 };
    if let Some(p) = progress {
        p.store(written, Ordering::Relaxed);
    }
    let mut body = resp.bytes_stream();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|e| anyhow::anyhow!("{what} download interrupted: {}", e.without_url()))?;
        written += chunk.len() as u64;
        if written > expected.bytes {
            // The index says how big the file is; more than that is not it.
            drop(out);
            let _ = tokio::fs::remove_file(tmp).await;
            bail!("{what} download ran past its expected size");
        }
        out.write_all(&chunk).await?;
        if let Some(p) = progress {
            p.store(written, Ordering::Relaxed);
        }
    }
    out.flush().await?;
    drop(out);

    let sha = sha256_file(tmp).await?;
    if sha != expected.sha256 {
        // Start over next time rather than resuming onto bad bytes.
        let _ = tokio::fs::remove_file(tmp).await;
        bail!("{what} file failed its checksum");
    }
    tokio::fs::rename(tmp, dest).await?;
    Ok(())
}

/// The sha256 of a file, as lowercase hex.
pub async fn sha256_file(path: &Path) -> Result<String> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<String> {
        let mut h = Sha256::new();
        let mut file = std::fs::File::open(path)?;
        std::io::copy(&mut file, &mut h)?;
        Ok(hex::encode(h.finalize()))
    })
    .await?
}

/// Unpack a tar archive into `staging`, entry by entry, refusing any link:
/// the routes that serve these files read under the folder, and a symlink
/// there would let them read any file. `unpack_in` also refuses a path that
/// climbs out of `staging`.
pub fn unpack_without_links(archive: &Path, staging: &Path, what: &str) -> Result<()> {
    let _ = std::fs::remove_dir_all(staging);
    std::fs::create_dir_all(staging)?;
    let mut ar = tar::Archive::new(std::fs::File::open(archive)?);
    for entry in ar.entries()? {
        let mut entry = entry?;
        let kind = entry.header().entry_type();
        if kind.is_symlink() || kind.is_hard_link() {
            bail!("the {what} archive holds a link; refusing it");
        }
        if !entry.unpack_in(staging)? {
            bail!("the {what} archive holds a path outside its folder; refusing it");
        }
    }
    Ok(())
}

/// Put `staging` in place of `live`, keeping the old copy at `old` only until
/// the new one is in.
pub fn swap_in(staging: &Path, live: &Path, old: &Path) -> Result<()> {
    let _ = std::fs::remove_dir_all(old);
    if live.exists() {
        std::fs::rename(live, old)?;
    }
    std::fs::rename(staging, live)?;
    let _ = std::fs::remove_dir_all(old);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archive_with(dir: &Path, build: impl FnOnce(&mut tar::Builder<std::fs::File>)) -> std::path::PathBuf {
        let path = dir.join("a.tar");
        let mut b = tar::Builder::new(std::fs::File::create(&path).unwrap());
        build(&mut b);
        b.finish().unwrap();
        path
    }

    #[test]
    fn unpacks_plain_files_and_swaps_them_in() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = archive_with(tmp.path(), |b| {
            let mut h = tar::Header::new_gnu();
            h.set_size(2);
            h.set_cksum();
            b.append_data(&mut h, "c/one.txt", &b"hi"[..]).unwrap();
        });
        let staging = tmp.path().join("new");
        unpack_without_links(&archive, &staging, "test").unwrap();
        let live = tmp.path().join("live");
        std::fs::create_dir_all(&live).unwrap();
        std::fs::write(live.join("stale.txt"), "old").unwrap();
        swap_in(&staging, &live, &tmp.path().join("old")).unwrap();
        assert_eq!(std::fs::read_to_string(live.join("c/one.txt")).unwrap(), "hi");
        assert!(!live.join("stale.txt").exists());
        assert!(!tmp.path().join("old").exists());
    }

    #[test]
    fn refuses_a_link() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = archive_with(tmp.path(), |b| {
            let mut h = tar::Header::new_gnu();
            h.set_entry_type(tar::EntryType::Symlink);
            h.set_size(0);
            h.set_cksum();
            b.append_link(&mut h, "escape", "/etc").unwrap();
        });
        let err = unpack_without_links(&archive, &tmp.path().join("new"), "test").unwrap_err();
        assert!(err.to_string().contains("link"), "{err}");
    }
}

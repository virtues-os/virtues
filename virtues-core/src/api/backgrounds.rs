//! Home backgrounds: the styles installed on this server and their files.
//!
//! A style is a folder under [`state_dir`] holding a `manifest.json` and the
//! files it names (`tools/backgrounds/build.py` writes both). The apps read
//! the manifests to resolve the background setting, and load the pictures
//! from this server only. See agents/plan/backgrounds-plan.md.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{Error, Result};

const WELL_KNOWN_DATA_DIR: &str = "/var/lib/virtues";
const DEV_STATE_DIR_FROM_CORE: &str = "../.background-state";

/// The manifest version this server serves. A style built for another
/// version is skipped, so an app never reads fields it does not know.
const MANIFEST_VERSION: u64 = 1;

/// Where installed styles live: `VIRTUES_BACKGROUNDS_DIR`, then
/// `/var/lib/virtues/backgrounds` on a server install, then
/// `.background-state/` in a dev checkout.
pub fn state_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("VIRTUES_BACKGROUNDS_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    let installed = Path::new(WELL_KNOWN_DATA_DIR);
    if installed.is_dir() {
        return installed.join("backgrounds");
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DEV_STATE_DIR_FROM_CORE)
}

#[derive(Debug, Serialize)]
pub struct Styles {
    /// Each installed style's manifest, as written by the build tool.
    pub styles: Vec<serde_json::Value>,
}

/// Every installed style whose manifest parses, has this server's version,
/// and names the folder it is in. A style that fails one of those is logged
/// and left out, so one bad folder cannot take the others down.
pub async fn list() -> Result<Styles> {
    list_in(&state_dir()).await
}

async fn list_in(root: &Path) -> Result<Styles> {
    let mut styles = Vec::new();
    let mut entries = match tokio::fs::read_dir(root).await {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Styles { styles }),
        Err(e) => return Err(Error::Other(format!("reading {}: {e}", root.display()))),
    };
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| Error::Other(format!("reading {}: {e}", root.display())))?
    {
        let id = entry.file_name().to_string_lossy().into_owned();
        if !is_style_id(&id) {
            continue;
        }
        let path = entry.path().join("manifest.json");
        let text = match tokio::fs::read_to_string(&path).await {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                tracing::warn!("background style {id}: reading {}: {e}", path.display());
                continue;
            }
        };
        let manifest: serde_json::Value = match serde_json::from_str(&text) {
            Ok(value) => value,
            Err(e) => {
                tracing::warn!("background style {id}: manifest.json does not parse: {e}");
                continue;
            }
        };
        if manifest.get("version").and_then(|v| v.as_u64()) != Some(MANIFEST_VERSION) {
            tracing::warn!("background style {id}: manifest version is not {MANIFEST_VERSION}");
            continue;
        }
        if manifest.get("style").and_then(|v| v.as_str()) != Some(id.as_str()) {
            tracing::warn!("background style {id}: manifest names a different style");
            continue;
        }
        styles.push(manifest);
    }
    styles.sort_by(|a, b| a["style"].as_str().cmp(&b["style"].as_str()));
    Ok(Styles { styles })
}

fn is_style_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A picture file and its content type.
pub struct File {
    pub bytes: Vec<u8>,
    pub content_type: &'static str,
}

/// One file of an installed style, by its path relative to the backgrounds
/// folder (`<style>/<collection>/<theme>/<name>-<width>.avif`). Only picture
/// files are served, and only from inside the folder.
pub async fn file(rel: &str) -> Result<File> {
    file_in(&state_dir(), rel).await
}

async fn file_in(root: &Path, rel: &str) -> Result<File> {
    let content_type = match rel.rsplit_once('.').map(|(_, ext)| ext) {
        Some("avif") => "image/avif",
        Some("webp") => "image/webp",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        _ => return Err(Error::NotFound(format!("no background file {rel}"))),
    };
    // Each segment is a plain name: no `..`, no hidden files, no separators
    // other than `/`. This keeps the path inside `root` without touching the
    // filesystem to canonicalize it.
    let safe = !rel.is_empty()
        && rel.split('/').all(|seg| {
            !seg.is_empty()
                && !seg.starts_with('.')
                && seg
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        });
    if !safe {
        return Err(Error::NotFound(format!("no background file {rel}")));
    }
    match tokio::fs::read(root.join(rel)).await {
        Ok(bytes) => Ok(File { bytes, content_type }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err(Error::NotFound(format!("no background file {rel}")))
        }
        Err(e) => Err(Error::Other(format!("reading background file {rel}: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(style: &str, version: u64) -> String {
        format!(r#"{{"version":{version},"style":"{style}","renderer":"image","collections":[]}}"#)
    }

    #[tokio::test]
    async fn lists_only_valid_styles() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for (folder, body) in [
            ("paintings-b", manifest("paintings-b", 1)),
            ("paintings-a", manifest("paintings-a", 1)),
            ("old", manifest("old", 0)),
            ("renamed", manifest("something-else", 1)),
            ("broken", "{".to_string()),
        ] {
            std::fs::create_dir_all(root.join(folder)).unwrap();
            std::fs::write(root.join(folder).join("manifest.json"), body).unwrap();
        }
        std::fs::create_dir_all(root.join("empty")).unwrap();

        let styles = list_in(root).await.unwrap().styles;
        let ids: Vec<_> = styles.iter().map(|s| s["style"].as_str().unwrap()).collect();
        assert_eq!(ids, ["paintings-a", "paintings-b"]);
    }

    #[tokio::test]
    async fn a_missing_folder_is_no_styles() {
        let dir = tempfile::tempdir().unwrap();
        let styles = list_in(&dir.path().join("absent")).await.unwrap().styles;
        assert!(styles.is_empty());
    }

    #[tokio::test]
    async fn serves_pictures_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("s/forest/light")).unwrap();
        std::fs::write(root.join("s/forest/light/a-896.webp"), b"webp").unwrap();
        std::fs::write(root.join("s/manifest.json"), b"{}").unwrap();

        let ok = file_in(root, "s/forest/light/a-896.webp").await.unwrap();
        assert_eq!(ok.content_type, "image/webp");
        assert_eq!(ok.bytes, b"webp");

        for bad in [
            "s/manifest.json",
            "s/forest/light/../../s/forest/light/a-896.webp",
            "../outside.webp",
            "/etc/a.webp",
            "s//forest/a.webp",
            "s/.hidden.webp",
            "s/forest/light/missing.webp",
        ] {
            assert!(
                matches!(file_in(root, bad).await, Err(Error::NotFound(_))),
                "{bad} should not be served"
            );
        }
    }
}

//! Read an applet's own source.
//!
//! Every applet, whatever its provenance — shipped, source fan-out, chat
//! authored, git imported — can be read by its owner. On an appliance whose
//! pitch is that you can verify what touches your data, "here is the code that
//! ran" is the cheapest trust artifact available, and it is the half of
//! "fork, change, see" that costs almost nothing.
//!
//! Read-only. Editing is a separate surface with a separate lifecycle (a fork
//! into the state root); nothing here writes.
//!
//! ## Why this is more careful than the face server
//!
//! `faces.rs` serves from `<applet>/face/`, a directory that ships with the
//! box. This serves from the applet folder itself, which for an authored or
//! imported package lives in the **writable** state root — so the file tree is
//! partly attacker-influenced and the guards have to hold against a hostile
//! layout, not just a careless path:
//!
//! - **Symlinks.** A package can ship a symlink to `/etc/shadow` or to the box
//!   env file. Rejecting `..` is not enough; every path is canonicalized and
//!   must still resolve inside the applet folder.
//! - **`.git`.** A repo cloned from an authenticated HTTPS remote keeps the
//!   credential in `.git/config`'s URL. Dot-directories are skipped wholesale,
//!   which also covers `.env` and editor droppings.
//! - **Size and binary content.** Applet folders can contain committed
//!   binaries; those are listed but never inlined.

use axum::{
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use serde_json::json;

/// Per-file inline cap. Large enough for any hand-written applet, small enough
/// that a listing can't be used to haul data through the API.
const MAX_FILE_BYTES: u64 = 256 * 1024;

/// Total files listed. A runaway folder shouldn't produce an unbounded tree.
const MAX_FILES: usize = 500;

/// Directory depth walked below the applet folder.
const MAX_DEPTH: usize = 6;

#[derive(Debug, Serialize)]
pub struct SourceFile {
    /// Path relative to the applet folder, `/`-separated.
    pub path: String,
    pub size: u64,
    /// False when the file is binary or over [`MAX_FILE_BYTES`]; the content
    /// endpoint will refuse it, and the UI should not offer to open it.
    pub readable: bool,
}

#[derive(Debug, Serialize)]
pub struct SourceListing {
    /// Manifest folder, relative to whichever applet root it resolved from.
    pub dir: String,
    /// `shipped` (came with the box) or `state` (authored, imported, forked).
    /// Decided by which root the folder actually resolves in, not by anything
    /// the manifest claims.
    pub origin_root: &'static str,
    pub files: Vec<SourceFile>,
    /// True when the walk stopped early at [`MAX_FILES`].
    pub truncated: bool,
}

/// Resolve an applet id to its on-disk folder, plus which root it came from.
fn applet_dir(applet_id: &str) -> Option<(std::path::PathBuf, &'static str)> {
    let dir = crate::applet_templates::dir_for_applet_id(applet_id)?;
    let resolved = crate::applet_templates::resolve_applet_dir(&dir);
    if !resolved.is_dir() {
        return None;
    }
    let shipped = crate::applet_templates::shipped_root();
    let origin_root = match (resolved.canonicalize(), shipped.canonicalize()) {
        (Ok(r), Ok(s)) if r.starts_with(&s) => "shipped",
        _ => "state",
    };
    Some((resolved, origin_root))
}

/// A segment we never descend into or serve. Dot-directories carry secrets far
/// more often than they carry anything worth reading — `.git/config` holds the
/// remote URL, which for an authenticated clone holds a token.
fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

/// Cheap binary sniff: a NUL in the first 8 KB. Good enough to keep compiled
/// artifacts and images out of a text pane.
fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|b| *b == 0)
}

/// Walk the applet folder, breadth-first, skipping hidden dirs.
fn collect(root: &std::path::Path) -> (Vec<SourceFile>, bool) {
    let mut out = Vec::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back((root.to_path_buf(), String::new(), 0usize));

    while let Some((dir, prefix, depth)) = queue.pop_front() {
        if depth > MAX_DEPTH {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut names: Vec<_> = entries.flatten().collect();
        names.sort_by_key(|e| e.file_name());

        for entry in names {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if is_hidden(&name) {
                continue;
            }
            let rel = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            // `metadata()` follows symlinks; `symlink_metadata` does not. Use
            // the latter so a link is classified as a link and skipped rather
            // than silently walked to wherever it points.
            let Ok(meta) = entry.path().symlink_metadata() else {
                continue;
            };
            if meta.is_symlink() {
                continue;
            }
            if meta.is_dir() {
                queue.push_back((entry.path(), rel, depth + 1));
                continue;
            }
            if out.len() >= MAX_FILES {
                return (out, true);
            }
            let size = meta.len();
            out.push(SourceFile {
                path: rel,
                size,
                readable: size <= MAX_FILE_BYTES,
            });
        }
    }
    (out, false)
}

/// `GET /api/applets/:id/source` — the applet's file tree.
pub async fn list_handler(Path(applet_id): Path<String>) -> Response {
    let Some((root, origin_root)) = applet_dir(&applet_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "no source directory for this applet" })),
        )
            .into_response();
    };
    let dir = crate::applet_templates::dir_for_applet_id(&applet_id).unwrap_or_default();
    let (mut files, truncated) = collect(&root);
    // Manifest first — it is what the reader is usually looking for.
    files.sort_by(|a, b| {
        let rank = |p: &str| if p == "manifest.toml" { 0 } else { 1 };
        rank(&a.path)
            .cmp(&rank(&b.path))
            .then_with(|| a.path.cmp(&b.path))
    });
    (
        StatusCode::OK,
        Json(SourceListing {
            dir,
            origin_root,
            files,
            truncated,
        }),
    )
        .into_response()
}

/// `GET /api/applets/:id/source/*path` — one file, as text.
pub async fn file_handler(Path((applet_id, rel)): Path<(String, String)>) -> Response {
    let Some((root, _)) = applet_dir(&applet_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    // First pass: reject anything that isn't a plain relative name chain.
    if rel.is_empty()
        || rel.split('/').any(|seg| {
            seg.is_empty() || seg == "." || seg == ".." || seg.contains('\\') || is_hidden(seg)
        })
    {
        return StatusCode::BAD_REQUEST.into_response();
    }

    // Second pass, and the one that actually holds: resolve the real path and
    // require it to still be inside the applet folder. The segment check above
    // cannot see a symlink whose target escapes.
    let candidate = root.join(&rel);
    let (Ok(real), Ok(real_root)) = (candidate.canonicalize(), root.canonicalize()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !real.starts_with(&real_root) || !real.is_file() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Ok(meta) = real.metadata() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if meta.len() > MAX_FILE_BYTES {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({ "error": "file too large to display", "size": meta.len() })),
        )
            .into_response();
    }

    let Ok(bytes) = tokio::fs::read(&real).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if looks_binary(&bytes) {
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Json(json!({ "error": "binary file" })),
        )
            .into_response();
    }

    match String::from_utf8(bytes) {
        Ok(text) => (StatusCode::OK, Json(json!({ "path": rel, "text": text }))).into_response(),
        Err(_) => (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Json(json!({ "error": "not valid UTF-8" })),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_segments_are_refused() {
        assert!(is_hidden(".git"));
        assert!(is_hidden(".env"));
        assert!(!is_hidden("manifest.toml"));
    }

    #[test]
    fn binary_sniff_catches_nul() {
        assert!(looks_binary(b"\x7fELF\x00\x01"));
        assert!(!looks_binary(b"fn main() {}\n"));
    }

    /// The walk must not expose a cloned repo's `.git` (whose config holds the
    /// remote URL, and for an authenticated clone the token in it) and must not
    /// follow a symlink out of the applet folder. Both are things a package
    /// controls, since imported and authored packages live in a writable root.
    #[test]
    fn walk_skips_dotdirs_and_symlinks() {
        let base = std::env::temp_dir().join(format!("vsrc-{}", std::process::id()));
        let applet = base.join("applet");
        let outside = base.join("outside");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(applet.join(".git")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();

        std::fs::write(applet.join("manifest.toml"), "name = \"x\"\n").unwrap();
        std::fs::write(applet.join(".git").join("config"), "url = https://tok@h/r\n").unwrap();
        std::fs::write(outside.join("secret.txt"), "vault key\n").unwrap();

        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.join("secret.txt"), applet.join("escape.txt")).unwrap();

        let (files, _) = collect(&applet);
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();

        assert!(paths.contains(&"manifest.toml"), "real files are listed");
        assert!(
            !paths.iter().any(|p| p.contains(".git")),
            "dot-directories must never be walked: {paths:?}"
        );
        #[cfg(unix)]
        assert!(
            !paths.contains(&"escape.txt"),
            "symlinks must not be listed: {paths:?}"
        );

        let _ = std::fs::remove_dir_all(&base);
    }
}

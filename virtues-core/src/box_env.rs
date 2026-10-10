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

/// The box env file. `VIRTUES_ENV_FILE` overrides it so tests (and a box with a
/// different layout) don't have to touch `/var/lib`.
pub fn path() -> PathBuf {
    std::env::var("VIRTUES_ENV_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/var/lib/virtues/virtues.env"))
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
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Ok(false);
    };
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

    let tmp = path.with_extension("env.tmp");
    std::fs::write(&tmp, body).map_err(|e| Error::Other(format!("write {}: {e}", tmp.display())))?;
    // Keep the original's mode and owner: the file holds the encryption key.
    if let Ok(meta) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(&tmp, meta.permissions());
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let _ = std::os::unix::fs::chown(&tmp, Some(meta.uid()), Some(meta.gid()));
        }
    }
    std::fs::rename(&tmp, path)
        .map_err(|e| Error::Other(format!("replace {}: {e}", path.display())))?;
    Ok(true)
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
    fn a_missing_file_is_left_missing() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("absent.env");
        assert!(!edit(&p, &[("K", "v".into())], &[]).unwrap());
        assert!(!p.exists());
    }
}

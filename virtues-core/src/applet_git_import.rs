//! Git-import on-ramp for actions.
//!
//! `POST /api/admin/actions/import-git { url, ref }` clones a repo into
//! `actions/<slug>/` and re-runs the standard catalog scanner + reconcile
//! flow. Once the folder lands under `actions/`, the system makes no further
//! distinction between built-ins and imports — the dir is the spec.
//!
//! Layout supported by the scanner (see `applet_templates::load_catalog`):
//!   - `actions/<slug>/manifest.toml` — single-action repo
//!   - `actions/<slug>/actions/<name>/manifest.toml` — pack
//!
//! Updates are manual: re-running this endpoint with the same URL fetches and
//! resets the working tree to the requested ref, then reconciles. Stale rows
//! (manifests removed upstream) are deleted by diffing the row set under the
//! slug prefix before/after reconcile.
//!
//! Trust note: cloned manifests run with the same privileges as built-ins.
//! There is no sandbox in v1; users should only import repos they trust.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tokio::process::Command;

use crate::applet_templates;
use crate::error::{Error, Result};

#[derive(Debug, Deserialize)]
pub struct ImportRequest {
    /// HTTPS URL or `git@host:owner/repo.git` form. Validated before use.
    pub url: String,
    /// Branch, tag, or commit SHA. Defaults to `main` when absent or empty.
    #[serde(default)]
    pub r#ref: Option<String>,
}

#[derive(Debug, Serialize, Default)]
pub struct ImportOutcome {
    /// Folder name under `actions/` that the repo landed in.
    pub slug: String,
    /// Resolved commit SHA after fetch.
    pub commit: Option<String>,
    /// Applet ids newly inserted by this import.
    pub added: Vec<String>,
    /// Applet ids that already existed under this slug and were re-upserted.
    pub updated: Vec<String>,
    /// Applet ids that disappeared from the repo since the last import; their
    /// rows are deleted (run history preserved via FK nullification).
    pub removed: Vec<String>,
}

/// Top-level entry point. Validates the request, clones-or-fetches into
/// `actions/<slug>/`, reloads the on-disk catalog, runs reconcile, and returns
/// a per-row diff scoped to the slug prefix.
pub async fn import(db: &PgPool, req: ImportRequest) -> Result<ImportOutcome> {
    let url = req.url.trim();
    validate_url(url)?;

    let git_ref = req
        .r#ref
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("main");
    validate_ref(git_ref)?;

    let slug = slug_for_url(url)?;
    let actions_root = applet_templates::state_root();
    let target = actions_root.join(&slug);

    // Snapshot the existing row set under this slug so we can diff after
    // reconcile. Matches both `dir = '<slug>'` (single-action repo) and
    // `dir LIKE '<slug>/%'` (pack member).
    let before: HashSet<String> = ids_under_slug(db, &slug).await?;

    // Clone or fast-forward in place. We hard-reset to the requested ref so
    // the working tree is deterministic — pulled code wins over local edits
    // in this directory.
    clone_or_update(&target, url, git_ref).await?;

    let commit = resolve_head(&target).await.ok();

    // Re-read manifests from disk and reconcile. This is the same flow the
    // admin Reconcile button runs; we just scoped the diff to our slug.
    applet_templates::reload_catalog();
    if let Err(e) = applet_templates::reconcile_templates(db).await {
        return Err(Error::Other(format!(
            "reconcile after import failed: {e}"
        )));
    }

    let after: HashSet<String> = ids_under_slug(db, &slug).await?;

    let added: Vec<String> = after.difference(&before).cloned().collect();
    let removed: Vec<String> = before.difference(&after).cloned().collect();
    let updated: Vec<String> = before.intersection(&after).cloned().collect();

    // Reconcile upserts but never deletes. Manifests removed upstream leave
    // zombie rows pointing to nothing — clean those up here.
    for id in &removed {
        // Preserve run history: nullify FK first.
        sqlx::query("UPDATE app_applet_runs SET applet_id = NULL WHERE applet_id = $1")
            .bind(id)
            .execute(db)
            .await?;
        sqlx::query("DELETE FROM app_applets WHERE id = $1")
            .bind(id)
            .execute(db)
            .await?;
    }

    Ok(ImportOutcome {
        slug,
        commit,
        added,
        updated,
        removed,
    })
}

/// Reject anything that isn't a plausible git URL. We pass the URL as a
/// distinct argv to `git`, so shell-injection isn't the threat model — but a
/// nonsense URL hangs the clone for the network timeout. Better to fail fast.
fn validate_url(url: &str) -> Result<()> {
    if url.is_empty() {
        return Err(Error::InvalidInput("git url is empty".into()));
    }
    let ok_https = url.starts_with("https://") || url.starts_with("http://");
    let ok_ssh = url.starts_with("git@") && url.contains(':');
    let ok_git = url.starts_with("git://");
    if !(ok_https || ok_ssh || ok_git) {
        return Err(Error::InvalidInput(format!(
            "unsupported git url scheme: {url}"
        )));
    }
    // Cheap sanity check — no embedded newlines or NUL.
    if url.bytes().any(|b| b == 0 || b == b'\n' || b == b'\r') {
        return Err(Error::InvalidInput("git url contains control bytes".into()));
    }
    Ok(())
}

/// Refs are passed as argv to `git fetch` / `git checkout`, but reject the
/// obvious garbage so a typo doesn't end up shelling out at all.
fn validate_ref(r: &str) -> Result<()> {
    if r.is_empty() {
        return Err(Error::InvalidInput("git ref is empty".into()));
    }
    if r.starts_with('-') {
        return Err(Error::InvalidInput("git ref must not start with '-'".into()));
    }
    if r.bytes()
        .any(|b| b == 0 || b == b' ' || b == b'\n' || b == b'\r')
    {
        return Err(Error::InvalidInput("git ref contains whitespace".into()));
    }
    Ok(())
}

/// Derive a folder name under `actions/` from a Git URL.
///
/// Examples:
///   `https://github.com/alice/my-actions.git`     → `my-actions`
///   `git@github.com:alice/my-actions.git`         → `my-actions`
///   `https://example.com/foo/bar/baz`             → `baz`
fn slug_for_url(url: &str) -> Result<String> {
    // Strip trailing slash and `.git`.
    let trimmed = url.trim_end_matches('/').trim_end_matches(".git");
    // Last path-or-colon segment is the repo name.
    let raw = trimmed
        .rsplit(|c: char| c == '/' || c == ':')
        .next()
        .unwrap_or("");
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches('-').to_string();
    if cleaned.is_empty() {
        return Err(Error::InvalidInput(format!(
            "could not derive a folder name from url: {url}"
        )));
    }
    Ok(cleaned)
}

/// `git clone --depth 1 --branch <ref>` if the dir is empty; otherwise
/// `fetch` + hard `reset` to `FETCH_HEAD`. Either way the working tree
/// matches the requested ref when this returns.
async fn clone_or_update(target: &Path, url: &str, git_ref: &str) -> Result<()> {
    let exists = target.is_dir() && target.join(".git").is_dir();

    if !exists {
        // Make sure parent exists.
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Other(format!("mkdir {} failed: {e}", parent.display())))?;
        }
        // If the target exists but isn't a git checkout, refuse — don't
        // clobber whatever the user has there. This usually means the slug
        // collides with a built-in or hand-authored action folder. Tell the
        // user concretely so they can rename or remove the conflict.
        if target.exists() {
            return Err(Error::InvalidInput(format!(
                "actions/{} already exists and isn't a git checkout — \
                 the import slug collides with a built-in or hand-authored action. \
                 Rename or remove that folder before importing this URL.",
                target.file_name().and_then(|s| s.to_str()).unwrap_or("?")
            )));
        }
        run_git(
            &applets_root_buf(),
            &[
                "clone",
                "--depth",
                "1",
                "--branch",
                git_ref,
                "--single-branch",
                url,
                target.to_str().ok_or_else(|| {
                    Error::Other("non-utf8 import target path".into())
                })?,
            ],
        )
        .await?;
        return Ok(());
    }

    // Existing checkout: fetch the ref shallowly, then hard-reset.
    run_git(target, &["fetch", "--depth", "1", "origin", git_ref]).await?;
    run_git(target, &["reset", "--hard", "FETCH_HEAD"]).await?;
    Ok(())
}

async fn resolve_head(dir: &Path) -> Result<String> {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| Error::Other(format!("git rev-parse failed to spawn: {e}")))?;
    if !out.status.success() {
        return Err(Error::Other(format!(
            "git rev-parse HEAD failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

async fn run_git(cwd: &Path, args: &[&str]) -> Result<()> {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        // Refuse to prompt on stdin. Without this, a private HTTPS URL with
        // no credentials hangs the request indefinitely waiting for input
        // that's never going to come. Fail fast with the underlying auth
        // error instead.
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| Error::Other(format!("git {} failed to spawn: {e}", args.join(" "))))?;
    if !out.status.success() {
        return Err(Error::Other(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(())
}

fn applets_root_buf() -> PathBuf {
    applet_templates::state_root()
}

async fn ids_under_slug(db: &PgPool, slug: &str) -> Result<HashSet<String>> {
    let prefix = format!("{slug}/");
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT id FROM app_applets WHERE dir = $1 OR dir LIKE $2 || '%'",
    )
    .bind(slug)
    .bind(&prefix)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|(s,)| s).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_extraction() {
        assert_eq!(
            slug_for_url("https://github.com/alice/my-actions.git").unwrap(),
            "my-actions"
        );
        assert_eq!(
            slug_for_url("git@github.com:alice/my-actions.git").unwrap(),
            "my-actions"
        );
        assert_eq!(
            slug_for_url("https://example.com/foo/bar/baz/").unwrap(),
            "baz"
        );
        // Special chars get folded.
        assert_eq!(
            slug_for_url("https://example.com/foo/Bar.Baz").unwrap(),
            "bar-baz"
        );
    }

    #[test]
    fn url_validation() {
        assert!(validate_url("https://github.com/x/y.git").is_ok());
        assert!(validate_url("git@github.com:x/y.git").is_ok());
        assert!(validate_url("file:///etc/passwd").is_err());
        assert!(validate_url("ssh://x@y/z").is_err());
        assert!(validate_url("").is_err());
        assert!(validate_url("https://x\nhost").is_err());
    }

    #[test]
    fn ref_validation() {
        assert!(validate_ref("main").is_ok());
        assert!(validate_ref("v1.2.3").is_ok());
        assert!(validate_ref("a7f3c2d").is_ok());
        assert!(validate_ref("").is_err());
        assert!(validate_ref("--upload-pack=evil").is_err());
        assert!(validate_ref("main branch").is_err());
    }
}

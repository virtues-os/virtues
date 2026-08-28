//! Git-import on-ramp for applets.
//!
//! `POST /api/admin/applets/import-git { url, ref }` clones a repo into the
//! **state root** (`/var/lib/virtues/applets/<slug>/`) and re-runs the standard
//! catalog scanner + reconcile flow. Once the folder lands there the system
//! makes no further distinction between built-ins and imports — the dir is the
//! spec. The state root, not the shipped root: the shipped tree is package data
//! the installer replaces wholesale on every release, and imports have to
//! survive that.
//!
//! Layout supported by the scanner (see `applet_templates::load_catalog`):
//!   - `<slug>/manifest.toml` — single-applet repo
//!   - `<slug>/<name>/manifest.toml` — pack
//!   - `<slug>/sources.toml` — the package's own `[[source]]` rows
//!
//! Updates are manual: re-running this endpoint with the same URL fetches and
//! resets the working tree to the requested ref, then reconciles. Stale rows
//! (manifests removed upstream) are deleted by diffing the row set under the
//! slug's id prefix before/after reconcile.
//!
//! **Trust note, and it is the whole story right now:** cloned manifests run
//! with the same privileges as built-ins. `command` is argv and the authoring
//! docs teach `["python3", "main.py"]`, so an import is arbitrary code
//! execution as the box user — which has passwordless sudo. There is no
//! sandbox yet. Until P4 of `agents/plan/sources-packages-plan.md` lands (argv policy
//! by provenance, sudo-gating, and the `systemd-run` jail that
//! `code_interpreter` already proves out), this endpoint should not be put in
//! front of anyone who would not audit the repo themselves.

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
    /// Sudo request id from `/api/sudo/request`. Required — importing runs a
    /// third party's code on this box.
    #[serde(default)]
    pub sudo_request_id: Option<String>,
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

    // Provenance (repo/ref/SHA) used to be persisted to app_applet_package
    // here — written once, read by nothing, table dropped 2026-08-28. If an
    // update check for git-imported applets is ever built, the table returns
    // WITH its reader.

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
    // Cheap sanity check — no embedded newlines or NUL.
    if url.bytes().any(|b| b == 0 || b == b'\n' || b == b'\r') {
        return Err(Error::InvalidInput("git url contains control bytes".into()));
    }

    // `http://` and `git://` are gone. Both are unauthenticated cleartext, so
    // the code the box is about to run could be swapped in flight by anything
    // on the path — which is not a trade worth making for a convenience nobody
    // asked for.
    let ok_https = url.starts_with("https://");
    let ok_ssh = url.starts_with("git@") && url.contains(':');
    if !(ok_https || ok_ssh) {
        return Err(Error::InvalidInput(format!(
            "git url must be https:// or git@host:owner/repo — got: {url}"
        )));
    }

    deny_internal_host(url)
}

/// Refuse hosts that only the box itself can reach.
///
/// The importer runs server-side, so an unrestricted URL turns it into a
/// request forger: `169.254.169.254` is the cloud metadata service, and a box
/// on a home LAN can see every other machine on it. This is a literal-host
/// check, not full SSRF protection — a hostname that *resolves* to a private
/// address still passes, and DNS rebinding is not addressed. It stops the
/// obvious and the accidental; the real containment for what an import can do
/// once fetched is the jail (P4 in agents/plan/sources-packages-plan.md).
fn deny_internal_host(url: &str) -> Result<()> {
    let no_scheme = url
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(url);
    let no_user = no_scheme
        .split_once('@')
        .map(|(_, rest)| rest)
        .unwrap_or(no_scheme);
    let host = no_user
        .split(['/', ':'])
        .next()
        .unwrap_or("")
        .trim_matches(['[', ']'])
        .to_ascii_lowercase();

    let blocked = host == "localhost"
        || host == "::1"
        || host.ends_with(".localhost")
        || host.ends_with(".internal")
        || host.starts_with("127.")
        || host.starts_with("10.")
        || host.starts_with("192.168.")
        || host.starts_with("169.254.")
        || host.starts_with("fd")
        || host.starts_with("fe80:")
        // 172.16.0.0/12 — 172.16 through 172.31.
        || host
            .strip_prefix("172.")
            .and_then(|r| r.split('.').next())
            .and_then(|o| o.parse::<u8>().ok())
            .is_some_and(|o| (16..=31).contains(&o));

    if blocked {
        return Err(Error::InvalidInput(format!(
            "refusing to fetch from an internal address: {host}"
        )));
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

/// Derive a folder name under the state root from a Git URL.
///
/// **Host and owner are part of the identity.** The slug used to be the repo
/// basename alone, so `github.com/alice/tools` and `evil.com/mallory/tools`
/// both became `tools` — and because the URL is only consulted on first clone,
/// importing the second while the first existed would silently `git fetch` the
/// *original* remote and report success. A name collision between two strangers
/// is not a naming inconvenience, it is a supply-chain hole.
///
/// Examples:
///   `https://github.com/alice/my-applets.git` → `github-com-alice-my-applets`
///   `git@github.com:alice/my-applets.git`     → `github-com-alice-my-applets`
fn slug_for_url(url: &str) -> Result<String> {
    let trimmed = url.trim_end_matches('/').trim_end_matches(".git");

    // Strip scheme and any userinfo, then split host from path. `git@host:path`
    // and `https://host/path` normalize to the same `host/path` shape.
    let no_scheme = trimmed
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(trimmed);
    let no_user = no_scheme
        .split_once('@')
        .map(|(_, rest)| rest)
        .unwrap_or(no_scheme);
    // scp-form uses ':' between host and path; URL form uses '/'.
    let host_and_path = no_user.replacen(':', "/", 1);

    let parts: Vec<&str> = host_and_path
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    // Keep host + the last two path segments (owner/repo) — enough to be
    // unambiguous without turning a deep path into a filename.
    let keep: Vec<&str> = if parts.len() > 3 {
        let mut v = vec![parts[0]];
        v.extend_from_slice(&parts[parts.len() - 2..]);
        v
    } else {
        parts
    };

    let cleaned: String = keep
        .join("-")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    // Collapse runs of '-' so `github.com` doesn't become `github--com`.
    let mut collapsed = String::with_capacity(cleaned.len());
    for c in cleaned.chars() {
        if c == '-' && collapsed.ends_with('-') {
            continue;
        }
        collapsed.push(c);
    }
    let collapsed = collapsed.trim_matches('-').to_string();
    if collapsed.is_empty() {
        return Err(Error::InvalidInput(format!(
            "could not derive a folder name from url: {url}"
        )));
    }
    Ok(collapsed)
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
                "applets/{} already exists and isn't a git checkout — \
                 the import slug collides with a built-in or hand-authored applet. \
                 Rename or remove that folder before importing this URL.",
                target.file_name().and_then(|s| s.to_str()).unwrap_or("?")
            )));
        }
        let dest = target
            .to_str()
            .ok_or_else(|| Error::Other("non-utf8 import target path".into()))?;

        // `git clone --branch` takes a branch or a tag, never a commit — so
        // pinning to a SHA, which the request has always claimed to support,
        // silently failed at the one moment it matters most. init + fetch +
        // checkout takes all three.
        // `git init` runs *inside* the state root, so that directory has to
        // exist before we spawn — otherwise `current_dir` fails with
        // `No such file or directory (os error 2)`, which reads exactly like
        // git being missing and is not. It fooled CI (and me) into installing
        // git on a runner that already had 2.52 before anyone read the log
        // closely enough to notice.
        //
        // The root is absent on any box that has never authored or imported an
        // applet — a fresh install, and every CI run — so this is the first
        // import on a new box failing, not only a test artifact.
        let root = applets_root_buf();
        std::fs::create_dir_all(&root).map_err(|e| {
            Error::Other(format!("cannot create applet state root {}: {e}", root.display()))
        })?;
        run_git(&root, &["init", "--quiet", dest]).await?;
        if let Err(e) = run_git(target, &["remote", "add", "origin", url]).await {
            // Without this the slug is wedged permanently: `.git` exists so the
            // next attempt takes the update branch, whose first call is
            // `remote set-url origin` — which fails with "No such remote".
            let _ = std::fs::remove_dir_all(target);
            return Err(e);
        }
        if let Err(e) = run_git(target, &["fetch", "--depth", "1", "origin", git_ref]).await {
            // Leave nothing half-created behind for the next attempt to trip on.
            let _ = std::fs::remove_dir_all(target);
            return Err(e);
        }
        if let Err(e) = run_git(target, &["checkout", "--quiet", "FETCH_HEAD"]).await {
            let _ = std::fs::remove_dir_all(target);
            return Err(e);
        }
        return Ok(());
    }

    // Existing checkout: make the requested URL authoritative before fetching.
    // The slug now carries host and owner so a different repo lands in a
    // different folder, but the same repo reached over https and then ssh
    // shares one — and silently fetching whichever remote was configured first
    // is the shape of the bug this whole path just had.
    run_git(target, &["remote", "set-url", "origin", url]).await?;
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

/// Ceiling on any one git invocation. Without it an unresponsive host holds an
/// axum request handler open indefinitely — `GIT_TERMINAL_PROMPT=0` only covers
/// the credential-prompt case, not a server that accepts the connection and
/// then says nothing.
const GIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

async fn run_git(cwd: &Path, args: &[&str]) -> Result<()> {
    let fut = Command::new("git")
        .args(args)
        .current_dir(cwd)
        // Refuse to prompt on stdin. Without this, a private HTTPS URL with
        // no credentials hangs the request indefinitely waiting for input
        // that's never going to come. Fail fast with the underlying auth
        // error instead.
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output();

    let out = tokio::time::timeout(GIT_TIMEOUT, fut)
        .await
        .map_err(|_| {
            Error::Other(format!(
                "git {} timed out after {}s",
                args.join(" "),
                GIT_TIMEOUT.as_secs()
            ))
        })?
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

/// Every applet id that belongs to an imported slug.
///
/// Keys on the **id prefix**, not on a `dir` column. `app_applets.dir` was
/// dropped by migration 0051 as derivable-from-the-id, and this query was not
/// updated — so it failed at runtime with `column "dir" does not exist`. Since
/// it runs before the clone, the whole endpoint has been dead ever since: every
/// import returned 400 without fetching anything. `sqlx::query_as` is unchecked,
/// so nothing caught it at compile time and there was no integration test.
///
/// Ids derive as `applet_<dir with / → __>` (`applet_templates::parse_template`),
/// so a slug owns `applet_<slug>` itself, `applet_<slug>__<member>` for a pack,
/// and `<either>_<anchor>` for per-credential and per-device fan-out. One
/// prefix match covers all three. The underscore is escaped because it is a
/// single-character wildcard in LIKE — without that, `applet_foo_%` would also
/// claim rows belonging to `applet_fooX`.
async fn ids_under_slug(db: &PgPool, slug: &str) -> Result<HashSet<String>> {
    let prefix = format!("applet_{}", slug.replace('/', "__"));
    // Escape the prefix itself, not just the trailing separator. `applet_` alone
    // contributes an underscore, and a slug may carry more — each one is a
    // single-character wildcard in LIKE, so an unescaped pattern could claim ids
    // belonging to a neighbouring package.
    let escaped = prefix.replace('\\', "\\\\").replace('_', "\\_").replace('%', "\\%");
    let like = format!("{escaped}\\_%");
    let rows: Vec<(String,)> = sqlx::query_as(
        r#"SELECT id FROM app_applets
            WHERE id = $1 OR id LIKE $2 ESCAPE '\'"#,
    )
    .bind(&prefix)
    .bind(&like)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|(s,)| s).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_extraction() {
        // Host and owner are part of the slug — both URL and scp forms
        // normalize to the same identity.
        assert_eq!(
            slug_for_url("https://github.com/alice/my-actions.git").unwrap(),
            "github-com-alice-my-actions"
        );
        assert_eq!(
            slug_for_url("git@github.com:alice/my-actions.git").unwrap(),
            "github-com-alice-my-actions"
        );
        // Deep paths keep host + the last two segments, not the whole path.
        assert_eq!(
            slug_for_url("https://example.com/a/b/foo/bar/").unwrap(),
            "example-com-foo-bar"
        );
        // Special chars fold, and runs of '-' collapse.
        assert_eq!(
            slug_for_url("https://example.com/foo/Bar.Baz").unwrap(),
            "example-com-foo-bar-baz"
        );
    }

    /// The bug this replaced: the slug was the repo basename alone, so two
    /// different remotes owned the same folder — and since the URL is only
    /// consulted on first clone, the second import silently re-fetched the
    /// first's remote and reported success.
    #[test]
    fn different_remotes_never_share_a_slug() {
        let alice = slug_for_url("https://github.com/alice/tools").unwrap();
        let mallory = slug_for_url("https://evil.example/mallory/tools").unwrap();
        assert_ne!(alice, mallory, "same basename must not collide");
        assert!(alice.contains("alice") && alice.contains("github"));
    }

    #[test]
    fn url_validation() {
        assert!(validate_url("https://github.com/x/y.git").is_ok());
        assert!(validate_url("git@github.com:x/y.git").is_ok());
        assert!(validate_url("file:///etc/passwd").is_err());
        assert!(validate_url("ssh://x@y/z").is_err());
        assert!(validate_url("").is_err());
        assert!(validate_url("https://x\nhost").is_err());
        // Cleartext transports are refused: the box runs what it fetches.
        assert!(validate_url("http://github.com/x/y.git").is_err());
        assert!(validate_url("git://github.com/x/y.git").is_err());
    }

    /// The importer fetches server-side, so an unrestricted URL makes it a
    /// request forger — the cloud metadata endpoint being the sharpest case.
    #[test]
    fn internal_hosts_are_refused() {
        for url in [
            "https://169.254.169.254/latest/meta-data/",
            "https://localhost/x/y",
            "https://127.0.0.1/x/y",
            "https://10.1.2.3/x/y",
            "https://192.168.1.9/x/y",
            "https://172.20.0.5/x/y",
            "git@192.168.1.9:x/y.git",
        ] {
            assert!(validate_url(url).is_err(), "should refuse {url}");
        }
        // 172.32 is public; only 172.16-31 is private.
        assert!(validate_url("https://172.32.0.1/x/y").is_ok());
    }

    /// Exercises the fetch path against a real repo on disk. Everything here
    /// was untested — which is how a query against a column dropped two dozen
    /// migrations ago survived in the one place that runs before the clone.
    ///
    /// Goes through `clone_or_update` rather than `import`, because `import`
    /// needs a database and because `validate_url` now (correctly) refuses the
    /// local path this uses as an origin.
    #[tokio::test]
    async fn clone_then_update_checks_out_the_requested_ref() {
        // A `git` we can run, and an identity so `commit` doesn't fail on a
        // machine with no global config.
        let origin = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let o = origin.path();

        let git = |dir: &std::path::Path, args: &[&str]| {
            let ok = std::process::Command::new("git")
                .args(args)
                .current_dir(dir)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@t")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@t")
                .output()
                .expect("git runs")
                .status
                .success();
            assert!(ok, "git {args:?} failed");
        };

        git(o, &["init", "--quiet", "-b", "main", "."]);
        std::fs::write(o.join("manifest.toml"), "name = \"One\"\nowner = \"user\"\n").unwrap();
        git(o, &["add", "."]);
        git(o, &["commit", "--quiet", "-m", "one"]);

        let first = String::from_utf8(
            std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(o)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .to_string();

        // Fresh clone of a branch.
        let target = work.path().join("pkg");
        clone_or_update(&target, o.to_str().unwrap(), "main")
            .await
            .expect("first clone");
        assert!(target.join("manifest.toml").is_file());
        assert_eq!(resolve_head(&target).await.unwrap(), first);

        // Upstream moves; re-running takes the new commit.
        std::fs::write(o.join("manifest.toml"), "name = \"Two\"\nowner = \"user\"\n").unwrap();
        git(o, &["add", "."]);
        git(o, &["commit", "--quiet", "-m", "two"]);

        clone_or_update(&target, o.to_str().unwrap(), "main")
            .await
            .expect("update");
        let body = std::fs::read_to_string(target.join("manifest.toml")).unwrap();
        assert!(body.contains("Two"), "update must take upstream's content");
        assert_ne!(resolve_head(&target).await.unwrap(), first);

        // And a bare SHA is a valid ref to pin to — `clone --branch` could not
        // do this, which is why the first clone is init + fetch + checkout.
        let pinned = work.path().join("pinned");
        clone_or_update(&pinned, o.to_str().unwrap(), &first)
            .await
            .expect("clone at a commit");
        assert_eq!(resolve_head(&pinned).await.unwrap(), first);
        let body = std::fs::read_to_string(pinned.join("manifest.toml")).unwrap();
        assert!(body.contains("One"), "pinned to the older commit");
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

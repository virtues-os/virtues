//! `publish_to_github` — put an applet's face on the web through a repo the
//! owner names.
//!
//! The box opens no port for this. It writes one file into a GitHub repo,
//! and whatever deploys that repo (Vercel, Pages, Netlify) serves it.
//!
//! Three things keep it a publish rather than an exfiltration channel:
//!
//! - **The token is bound, never handed out.** It is the `github_publish`
//!   credential, decrypted here and sent only to `api.github.com`. The model
//!   names a repo and a path; it never sees the secret, and no shell gets it.
//!   GitHub's own fine-grained scope (one repo, contents) bounds the rest.
//! - **A grant is for exact bytes.** The Allow card names the repo, branch
//!   and path, and the grant id hashes those with the content, so a face
//!   edited after it was allowed asks again.
//! - **A face that only works on the box is refused.** One that still calls
//!   `virtues.query`, loads `virtues.js`, or names `/api/` or a localhost URL
//!   would publish a dead page that also names the box's API. The check is
//!   `api::publications::standalone_face`, the same one the Share sheet runs.

use base64::Engine;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use super::executor::{ToolError, ToolResult};

/// The credential source this publishes with (`applets/sources.toml`).
pub const SOURCE_ID: &str = "github_publish";

/// `owner/name`, GitHub's own character set.
fn valid_repo(repo: &str) -> bool {
    let mut parts = repo.split('/');
    let ok = |s: Option<&str>| {
        s.is_some_and(|s| {
            !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        })
    };
    ok(parts.next()) && ok(parts.next()) && parts.next().is_none()
}

/// A relative path inside the repo: no leading slash, no empty segment, and
/// nothing hidden, which also rules out `..` and `.github/`, the one place a
/// file in a repo runs as code.
fn valid_segments(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 512
        && !path.contains('\\')
        && path.split('/').all(|seg| !seg.is_empty() && !seg.starts_with('.'))
}

/// Where a page may go: a valid path ending in `.html`.
fn valid_path(path: &str) -> bool {
    valid_segments(path) && path.to_lowercase().ends_with(".html")
}

/// The permission id for these exact bytes at this exact place.
pub fn grant_id(repo: &str, branch: &str, path: &str, content: &str) -> String {
    let digest = Sha256::digest(format!("{repo}\0{branch}\0{path}\0{content}").as_bytes());
    format!("publish:{}", &hex::encode(digest)[..24])
}

/// One validated publish request, read from the tool's arguments and the
/// applet's face on disk.
pub struct Request {
    pub applet_id: String,
    pub applet_name: String,
    pub repo: String,
    pub branch: String,
    pub path: String,
    pub html: String,
}

impl Request {
    pub fn grant_id(&self) -> String {
        grant_id(&self.repo, &self.branch, &self.path, &self.html)
    }
}

/// Read the arguments and the face. `Err` is a refusal the model can act on.
pub async fn prepare(pool: &PgPool, arguments: &serde_json::Value) -> Result<Request, ToolError> {
    let arg = |k: &str| arguments.get(k).and_then(|v| v.as_str()).map(str::trim).unwrap_or("");
    let bad = |m: String| ToolError::InvalidParameters(m);

    let applet_id = arg("applet_id").to_string();
    let repo = arg("repo").to_string();
    let path = arg("path").trim_start_matches('/').to_string();
    let branch = match arg("branch") {
        "" => "main".to_string(),
        b => b.to_string(),
    };
    if applet_id.is_empty() {
        return Err(bad("applet_id is required (list_applets gives it)".into()));
    }
    if !valid_repo(&repo) {
        return Err(bad(format!("repo must be owner/name, got {repo:?}")));
    }
    if !valid_path(&path) {
        return Err(bad(format!(
            "path must be a relative .html file with no hidden segments, e.g. static/rome/index.html; got {path:?}"
        )));
    }
    if !valid_segments(&branch) {
        return Err(bad(format!("not a branch name: {branch:?}")));
    }

    // The same check the Share sheet runs: one file, nothing that needs the
    // box at view time.
    let (applet_name, html) = crate::api::publications::standalone_face(pool, &applet_id)
        .await
        .map_err(|e| match e {
            crate::error::Error::InvalidInput(m) | crate::error::Error::NotFound(m) => bad(m),
            other => ToolError::ExecutionFailed(other.to_string()),
        })?;

    Ok(Request { applet_id, applet_name, repo, branch, path, html })
}

/// The result that stops the turn and puts an Allow card in the chat, naming
/// where the page goes.
pub fn ask(req: &Request) -> ToolResult {
    ToolResult::success(serde_json::json!({
        "permission_needed": true,
        "awaiting_owner": true,
        "entity_id": req.grant_id(),
        "entity_type": "publish",
        "entity_title": format!(
            "{}  ·  {}  ·  {}  ({} KB)",
            req.repo, req.branch, req.path, req.html.len().div_ceil(1024)
        ),
        "message": format!("Publish \"{}\" to GitHub? Anyone who can see that repo or its site will see it.", req.applet_name),
        "note": "Not published. The owner has to allow it. Stop here. Once they allow it, \
                 call publish_to_github again with exactly the same arguments.",
    }))
}

/// Write the file. Creates it, or replaces what is there.
pub async fn publish(pool: &PgPool, req: &Request) -> Result<ToolResult, ToolError> {
    let token = token(pool).await?;
    let http = crate::http_client::base_builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| ToolError::ExecutionFailed(format!("http client: {e}")))?;
    let url = format!("https://api.github.com/repos/{}/contents/{}", req.repo, req.path);
    let github = |rb: reqwest::RequestBuilder| {
        rb.bearer_auth(&token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "virtues")
    };

    // Replacing a file needs the sha of the one there now.
    let existing = github(http.get(&url).query(&[("ref", &req.branch)]))
        .send()
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("reaching GitHub: {e}")))?;
    let sha = match existing.status().as_u16() {
        200 => existing
            .json::<serde_json::Value>()
            .await
            .ok()
            .and_then(|v| v.get("sha").and_then(|s| s.as_str()).map(str::to_string)),
        404 => None,
        _ => return Ok(github_failure(existing).await),
    };

    let mut body = serde_json::json!({
        "message": format!("publish: {} (from virtues)", req.applet_name),
        "content": base64::engine::general_purpose::STANDARD.encode(&req.html),
        "branch": req.branch,
    });
    if let Some(sha) = sha.as_deref() {
        body["sha"] = sha.into();
    }
    let resp = github(http.put(&url).json(&body))
        .send()
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("reaching GitHub: {e}")))?;
    if !resp.status().is_success() {
        return Ok(github_failure(resp).await);
    }
    let out: serde_json::Value = resp.json().await.unwrap_or_default();
    Ok(ToolResult::success(serde_json::json!({
        "published": true,
        "replaced": sha.is_some(),
        "applet_id": req.applet_id,
        "repo": req.repo,
        "branch": req.branch,
        "path": req.path,
        "commit": out.pointer("/commit/html_url"),
        "file": out.pointer("/content/html_url"),
        "note": "Committed. Whatever deploys this repo serves it from its next deploy; \
                 the public URL depends on that host, so say where it will appear only if the owner told you.",
    })))
}

/// The active `github_publish` token. The newest wins if several exist.
async fn token(pool: &PgPool) -> Result<String, ToolError> {
    let id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM credentials WHERE source_id = $1 AND status = 'active' \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(SOURCE_ID)
    .fetch_optional(pool)
    .await
    .map_err(|e| ToolError::ExecutionFailed(format!("credential lookup: {e}")))?;
    let Some(id) = id else {
        return Err(ToolError::ExecutionFailed(
            "no GitHub publishing connection. The owner adds one under Sources → \
             GitHub publishing, with a fine-grained token that can write contents of the repo."
                .into(),
        ));
    };
    let secrets = virtues_helpers::auth::read_credential_secrets(pool, &id)
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("reading the GitHub token: {e}")))?;
    secrets
        .get("token")
        .and_then(|v| v.as_str())
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .ok_or_else(|| ToolError::ExecutionFailed("the GitHub publishing connection has no token".into()))
}

/// GitHub's refusal, as something the model can relay. 401/403/404 on a
/// write are nearly always the token's scope, so that is what it says.
async fn github_failure(resp: reqwest::Response) -> ToolResult {
    let status = resp.status().as_u16();
    let detail = resp
        .json::<serde_json::Value>()
        .await
        .ok()
        .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(str::to_string))
        .unwrap_or_default();
    let hint = match status {
        401 => " The token is invalid or expired.",
        403 | 404 => " The token likely cannot write this repo: it needs Contents read and write on it.",
        409 | 422 => " The branch may not exist.",
        _ => "",
    };
    ToolResult::error(format!("GitHub refused ({status}): {detail}.{hint}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repos_and_paths_stay_inside_their_lines() {
        assert!(valid_repo("someone/site"));
        assert!(!valid_repo("someone"));
        assert!(!valid_repo("someone/site/extra"));
        assert!(!valid_repo("some one/site"));
        assert!(valid_path("static/rome/index.html"));
        assert!(!valid_path("static/../index.html"));
        assert!(!valid_path(".github/workflows/x.html"));
        assert!(!valid_path("static/rome/notes.md"));
        assert!(!valid_path("static//x.html"));
        assert!(!valid_path(""));
    }

    #[test]
    fn a_grant_is_for_exact_bytes_at_an_exact_place() {
        let a = grant_id("o/r", "main", "static/rome/index.html", "<h1>Rome</h1>");
        assert_eq!(a, grant_id("o/r", "main", "static/rome/index.html", "<h1>Rome</h1>"));
        assert_ne!(a, grant_id("o/r", "main", "static/rome/index.html", "<h1>Roma</h1>"));
        assert_ne!(a, grant_id("o/r", "main", "static/paris/index.html", "<h1>Rome</h1>"));
        assert_ne!(a, grant_id("o/r", "gh-pages", "static/rome/index.html", "<h1>Rome</h1>"));
    }
}

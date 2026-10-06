//! Publications: what the owner has shared (migration 0043).
//!
//! Publishing freezes an applet's face into a bundle the door serves
//! (`virtues-door`), records it in `app_publications`, and hands back a link.
//! The door has no database access, so the bundle directory is what is live
//! and this table is the owner's record of it: updating rewrites the bundle
//! under the same token, revoking deletes it.
//!
//! Only a face that stands alone publishes. Off the box nothing answers
//! `virtues.query` or serves `virtues.js`, so a live face would publish as an
//! empty page that also names the box's API; [`standalone_face`] refuses it.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::error::{Error, Result};
use virtues_door::{bundle, token};

/// How long a link lives unless the owner says otherwise.
pub const DEFAULT_EXPIRY_DAYS: u32 = 30;

/// A face larger than this does not publish. The door's own ceiling is higher;
/// this one is about what a phone should be asked to download.
pub const MAX_FACE_BYTES: usize = 5 * 1024 * 1024;

/// What a face that still depends on the box contains. Matched
/// case-insensitively; a mention in prose refuses too, which costs a rewrite.
const BOX_ONLY: &[(&str, &str)] = &[
    ("virtues.query", "asks your server for data each time someone opens it"),
    ("virtues.js", "loads a script only your server has"),
    ("virtues.css", "loads a stylesheet only your server has"),
    ("/api/", "names your server's API"),
    ("?vt=", "carries a face token"),
    ("localhost", "points at a local address"),
    ("127.0.0.1", "points at a local address"),
];

/// Every reason `html` would not stand on its own off the box.
pub fn box_only_findings(html: &str) -> Vec<String> {
    let lower = html.to_lowercase();
    BOX_ONLY
        .iter()
        .filter(|(needle, _)| lower.contains(needle))
        .map(|(needle, why)| format!("`{needle}` {why}"))
        .collect()
}

const WELL_KNOWN_DATA_DIR: &str = "/var/lib/virtues";
const DEV_STATE_DIR_FROM_CORE: &str = "../.publication-state";

/// Where publication state lives: `VIRTUES_PUBLICATIONS_DIR`, then the box's
/// data directory, then a gitignored folder in a dev checkout.
fn state_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("VIRTUES_PUBLICATIONS_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    let installed = Path::new(WELL_KNOWN_DATA_DIR);
    if installed.is_dir() {
        return installed.join("publications");
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DEV_STATE_DIR_FROM_CORE)
}

/// The bundle directory the door serves.
pub fn bundles_dir() -> PathBuf {
    state_dir().join("bundles")
}

/// The door's own directory: its key and the `endpoint-id` it writes.
pub fn door_dir() -> PathBuf {
    state_dir().join("door")
}

/// The door's EndpointId, once it has run at least once.
fn door_id() -> Option<String> {
    std::fs::read_to_string(door_dir().join("endpoint-id"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// A door's EndpointId (64 hex characters, as iroh prints it) in the 43
/// base64url characters a link carries.
fn compact_key(hex_id: &str) -> Option<String> {
    use base64::Engine;
    let bytes = hex::decode(hex_id).ok().filter(|b| b.len() == 32)?;
    Some(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

/// `<loader>#<door-key>.<token>`, when both are known. The part after `#`
/// never leaves the visitor's browser.
fn link_for(loader: Option<&str>, door: Option<&str>, token: &str) -> Option<String> {
    Some(format!("{}#{}.{token}", loader?, compact_key(door?)?))
}

/// Where links open: the loader on `virtues.ch` (`apps/loader`), or
/// `VIRTUES_SHARE_LOADER_URL` for a loader served somewhere else, such as a
/// dev machine.
const DEFAULT_LOADER_URL: &str = "https://s.virtues.ch/";

fn link(token: &str) -> Option<String> {
    let loader = std::env::var("VIRTUES_SHARE_LOADER_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_LOADER_URL.to_string());
    link_for(Some(&loader), door_id().as_deref(), token)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Publication {
    pub id: String,
    #[serde(skip)]
    pub token: String,
    pub producer_kind: String,
    pub producer_id: String,
    pub title: String,
    pub size_bytes: i64,
    pub card_title: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub open_count: i64,
    pub last_opened_at: Option<DateTime<Utc>>,
    /// Filled on the way out; `None` until the door has run and a loader is
    /// configured.
    #[sqlx(skip)]
    pub link: Option<String>,
}

impl Publication {
    fn with_link(mut self) -> Self {
        if self.revoked_at.is_none() {
            self.link = link(&self.token);
        }
        self
    }
}

const COLUMNS: &str = "id, token, producer_kind, producer_id, title, size_bytes, card_title, \
     created_at, updated_at, expires_at, revoked_at, open_count, last_opened_at";

/// The page in `face_dir`, if it stands alone off the box. `Err` names what
/// to change.
pub fn standalone_html(face_dir: &Path) -> Result<String> {
    // One file goes out. A face that loads a sibling file would publish with
    // that file missing.
    let siblings: Vec<String> = std::fs::read_dir(face_dir)
        .map_err(|e| Error::Other(format!("reading the face: {e}")))?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n != "index.html")
        .collect();
    if !siblings.is_empty() {
        return Err(Error::InvalidInput(format!(
            "only a single-file face publishes; this one also has {}. Inline them \
             (CSS in <style>, images as data: URIs) and remove the files.",
            siblings.join(", ")
        )));
    }
    let html = std::fs::read_to_string(face_dir.join("index.html"))
        .map_err(|e| Error::Other(format!("reading the face: {e}")))?;
    if html.len() > MAX_FACE_BYTES {
        return Err(Error::InvalidInput(format!(
            "the face is {} KB; the limit is {} KB",
            html.len() / 1024,
            MAX_FACE_BYTES / 1024
        )));
    }
    let findings = box_only_findings(&html);
    if !findings.is_empty() {
        return Err(Error::InvalidInput(format!(
            "this page only works on your server: {}. Write the content into the HTML itself, \
             then publish again.",
            findings.join("; ")
        )));
    }
    Ok(html)
}

/// An applet's name and its face, if the face stands alone.
pub async fn standalone_face(pool: &PgPool, applet_id: &str) -> Result<(String, String)> {
    let name = crate::scheduler::applets::get_applet(pool, applet_id)
        .await
        .map(|a| a.name)
        .map_err(|_| Error::NotFound(format!("no applet {applet_id:?}")))?;
    let face_dir = crate::server::faces::face_dir_for(applet_id)
        .ok_or_else(|| Error::InvalidInput(format!("\"{name}\" has no face to publish")))?;
    let html = standalone_html(&face_dir)?;
    Ok((name, html))
}

/// What the Share sheet shows before anything leaves: the page as it will
/// appear, and what in it the owner might not mean to send.
#[derive(Debug, Serialize)]
pub struct SharePreview {
    pub title: String,
    /// The page, or `None` when it cannot be shared as it is.
    pub html: Option<String>,
    /// Why it cannot be shared, and what to change.
    pub problem: Option<String>,
    pub size_bytes: usize,
    pub image_count: usize,
    /// Addresses of other sites the page links to.
    pub links: Vec<String>,
    /// Text that looks like contact details: emails and phone numbers.
    pub looks_private: Vec<String>,
}

/// Everything in `html` the Share sheet should name.
fn scan(html: &str) -> (usize, Vec<String>, Vec<String>) {
    use std::sync::OnceLock;
    static LINK: OnceLock<regex::Regex> = OnceLock::new();
    static EMAIL: OnceLock<regex::Regex> = OnceLock::new();
    static PHONE: OnceLock<regex::Regex> = OnceLock::new();
    let link = LINK.get_or_init(|| regex::Regex::new(r#"https?://[^\s"'<>)]+"#).unwrap());
    let email = EMAIL.get_or_init(|| {
        regex::Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}").unwrap()
    });
    // Ten or more digits, allowing the separators people write numbers with.
    let phone = PHONE.get_or_init(|| regex::Regex::new(r"\+?\d[\d\s().-]{8,}\d").unwrap());

    let image_count = html.matches("data:image/").count() + html.matches("<img").count();
    let mut links: Vec<String> = link.find_iter(html).map(|m| m.as_str().to_string()).collect();
    links.sort();
    links.dedup();
    // Inline images are base64; digits inside them are not phone numbers.
    let mut parts = html.split("data:");
    let mut text = parts.next().unwrap_or_default().to_string();
    for part in parts {
        // Drop the URI itself, up to the quote or paren that closes it.
        text.push(' ');
        text.push_str(part.split_once(['"', '\'', ')']).map_or("", |(_, rest)| rest));
    }
    let mut private: Vec<String> = email
        .find_iter(&text)
        .chain(phone.find_iter(&text).filter(|m| m.as_str().chars().filter(char::is_ascii_digit).count() >= 10))
        .map(|m| m.as_str().trim().to_string())
        .collect();
    private.sort();
    private.dedup();
    (image_count, links, private)
}

/// What sharing this applet's face would send.
pub async fn preview(pool: &PgPool, applet_id: &str) -> Result<SharePreview> {
    let name = crate::scheduler::applets::get_applet(pool, applet_id)
        .await
        .map(|a| a.name)
        .map_err(|_| Error::NotFound(format!("no applet {applet_id:?}")))?;
    let Some(face_dir) = crate::server::faces::face_dir_for(applet_id) else {
        return Ok(SharePreview {
            title: name,
            html: None,
            problem: Some("This applet has nothing to show, so there is no page to share.".into()),
            size_bytes: 0,
            image_count: 0,
            links: Vec::new(),
            looks_private: Vec::new(),
        });
    };
    Ok(match standalone_html(&face_dir) {
        Ok(html) => {
            let (image_count, links, looks_private) = scan(&html);
            SharePreview {
                title: name,
                size_bytes: html.len(),
                html: Some(html),
                problem: None,
                image_count,
                links,
                looks_private,
            }
        }
        Err(Error::InvalidInput(why)) => SharePreview {
            title: name,
            html: None,
            problem: Some(why),
            size_bytes: 0,
            image_count: 0,
            links: Vec::new(),
            looks_private: Vec::new(),
        },
        Err(e) => return Err(e),
    })
}

fn content_hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn meta_for(expires_at: Option<DateTime<Utc>>) -> bundle::Meta {
    bundle::Meta { expires_at_unix: expires_at.map(|t| t.timestamp()) }
}

fn door_error(e: anyhow::Error) -> Error {
    Error::Storage(format!("{e:#}"))
}

#[derive(Debug, Deserialize)]
pub struct CreateRequest {
    pub applet_id: String,
    /// Days until the link stops working. Absent means
    /// [`DEFAULT_EXPIRY_DAYS`]; `0` means never.
    pub expires_in_days: Option<u32>,
}

/// Share an applet's face: freeze it, put the bundle where the door serves
/// it, record it.
pub async fn create(pool: &PgPool, req: CreateRequest) -> Result<Publication> {
    let (title, html) = standalone_face(pool, &req.applet_id).await?;
    create_frozen(pool, &bundles_dir(), &req.applet_id, &title, html.as_bytes(), req.expires_in_days)
        .await
        .map(Publication::with_link)
}

async fn create_frozen(
    pool: &PgPool,
    root: &Path,
    applet_id: &str,
    title: &str,
    page: &[u8],
    expires_in_days: Option<u32>,
) -> Result<Publication> {
    let token = token::generate();
    let id = crate::ids::generate_id("pub", &[&token]);
    let expires_at = match expires_in_days.unwrap_or(DEFAULT_EXPIRY_DAYS) {
        0 => None,
        days => Some(Utc::now() + Duration::days(i64::from(days))),
    };
    // The bundle first: a row with no bundle would list a link that answers
    // nothing. A bundle with no row is unreachable (nobody has the token)
    // and is swept with the rest.
    bundle::write(root, &token, page, &meta_for(expires_at)).map_err(door_error)?;
    let inserted = sqlx::query_as::<_, Publication>(&format!(
        "INSERT INTO app_publications \
            (id, token, producer_kind, producer_id, title, content_hash, size_bytes, expires_at) \
         VALUES ($1, $2, 'applet', $3, $4, $5, $6, $7) RETURNING {COLUMNS}"
    ))
    .bind(&id)
    .bind(&token)
    .bind(applet_id)
    .bind(title)
    .bind(content_hash(page))
    .bind(page.len() as i64)
    .bind(expires_at)
    .fetch_one(pool)
    .await;
    match inserted {
        Ok(p) => {
            crate::door::wake();
            Ok(p)
        }
        Err(e) => {
            let _ = bundle::remove(root, &token);
            Err(Error::Database(format!("record publication: {e}")))
        }
    }
}

async fn get(pool: &PgPool, id: &str) -> Result<Publication> {
    sqlx::query_as::<_, Publication>(&format!("SELECT {COLUMNS} FROM app_publications WHERE id = $1"))
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::Database(format!("read publication: {e}")))?
        .ok_or_else(|| Error::NotFound(format!("no publication {id}")))
}

/// Everything the owner has shared, newest first, revoked ones included.
pub async fn list(pool: &PgPool) -> Result<Vec<Publication>> {
    let rows = sqlx::query_as::<_, Publication>(&format!(
        "SELECT {COLUMNS} FROM app_publications ORDER BY created_at DESC"
    ))
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("list publications: {e}")))?;
    Ok(rows.into_iter().map(Publication::with_link).collect())
}

/// Re-freeze the applet's face under the same link.
pub async fn update(pool: &PgPool, id: &str) -> Result<Publication> {
    let current = get(pool, id).await?;
    let (title, html) = standalone_face(pool, &current.producer_id).await?;
    update_frozen(pool, &bundles_dir(), current, &title, html.as_bytes())
        .await
        .map(Publication::with_link)
}

async fn update_frozen(
    pool: &PgPool,
    root: &Path,
    current: Publication,
    title: &str,
    page: &[u8],
) -> Result<Publication> {
    if current.revoked_at.is_some() {
        return Err(Error::InvalidInput("you revoked this link; share the page again for a new one".into()));
    }
    bundle::write(root, &current.token, page, &meta_for(current.expires_at)).map_err(door_error)?;
    sqlx::query_as::<_, Publication>(&format!(
        "UPDATE app_publications SET title = $2, content_hash = $3, size_bytes = $4, \
            updated_at = now() WHERE id = $1 RETURNING {COLUMNS}"
    ))
    .bind(&current.id)
    .bind(title)
    .bind(content_hash(page))
    .bind(page.len() as i64)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("update publication: {e}")))
}

/// Stop serving the link. The bundle is deleted; there is no other copy.
pub async fn revoke(pool: &PgPool, id: &str) -> Result<Publication> {
    revoke_in(pool, &bundles_dir(), id).await
}

async fn revoke_in(pool: &PgPool, root: &Path, id: &str) -> Result<Publication> {
    let current = get(pool, id).await?;
    bundle::remove(root, &current.token).map_err(door_error)?;
    let revoked = sqlx::query_as::<_, Publication>(&format!(
        "UPDATE app_publications SET revoked_at = COALESCE(revoked_at, now()) \
         WHERE id = $1 RETURNING {COLUMNS}"
    ))
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("revoke publication: {e}")))?;
    // The last live link revoked closes the door.
    crate::door::wake();
    Ok(revoked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_face_that_needs_the_box_is_named() {
        let live = r#"<script src="virtues.js"></script><script>virtues.query("select 1")</script>"#;
        assert_eq!(box_only_findings(live).len(), 2);
        assert_eq!(box_only_findings("<img src='http://LOCALHOST:7117/x.png'>").len(), 1);
        assert!(box_only_findings("<h1>Rome</h1><p>Day 1: Pantheon</p>").is_empty());
    }

    #[test]
    fn only_a_single_standalone_file_publishes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.html"), "<h1>Rome</h1>").unwrap();
        assert_eq!(standalone_html(dir.path()).unwrap(), "<h1>Rome</h1>");

        std::fs::write(dir.path().join("style.css"), "h1{}").unwrap();
        assert!(matches!(standalone_html(dir.path()), Err(Error::InvalidInput(_))));
        std::fs::remove_file(dir.path().join("style.css")).unwrap();

        std::fs::write(dir.path().join("index.html"), "<script src=virtues.js></script>").unwrap();
        assert!(matches!(standalone_html(dir.path()), Err(Error::InvalidInput(_))));
    }

    #[test]
    fn the_preview_names_what_leaves() {
        let html = r#"<p>Call +1 512 555 0142 or mail nick@example.com</p>
            <a href="https://maps.example.com/rome">map</a>
            <img src="data:image/png;base64,MTIzNDU2Nzg5MDEyMzQ1Ng==">"#;
        let (images, links, private) = scan(html);
        assert_eq!(images, 2, "one <img>, one data:image");
        assert_eq!(links, vec!["https://maps.example.com/rome".to_string()]);
        assert_eq!(private, vec!["+1 512 555 0142".to_string(), "nick@example.com".to_string()]);
        let (_, _, none) = scan("<h1>Day 3 · Vatican</h1><p>Museums at 9:00, 2026-10-14</p>");
        assert!(none.is_empty(), "{none:?}");
    }

    #[test]
    fn a_link_needs_both_a_loader_and_a_door() {
        let door = "dc70ba6300c2042b3d3c6ae34dba6b978c1e0d38cb9a7a1b90ccd6acb324baff";
        let link = link_for(Some("https://l.example/"), Some(door), "tok").unwrap();
        assert_eq!(link, "https://l.example/#3HC6YwDCBCs9PGrjTbprl4weDTjLmnobkMzWrLMkuv8.tok");
        assert_eq!(link_for(None, Some(door), "tok"), None);
        assert_eq!(link_for(Some("https://l.example/"), None, "tok"), None);
        assert_eq!(link_for(Some("https://l.example/"), Some("not-a-key"), "tok"), None);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn share_update_revoke(pool: PgPool) {
        let dir = tempfile::tempdir().unwrap();
        let store = bundle::Store::new(dir.path());

        let p = create_frozen(&pool, dir.path(), "applet_user__rome", "Rome", b"v1", None)
            .await
            .unwrap();
        assert_eq!(store.load(&p.token).as_deref(), Some(&b"v1"[..]));
        let days = (p.expires_at.unwrap() - p.created_at).num_days();
        assert!((29..=30).contains(&days), "{days}");

        let p = update_frozen(&pool, dir.path(), p, "Roma", b"v2").await.unwrap();
        assert_eq!(p.title, "Roma");
        assert_eq!(p.size_bytes, 2);
        assert_eq!(store.load(&p.token).as_deref(), Some(&b"v2"[..]));

        let revoked = revoke_in(&pool, dir.path(), &p.id).await.unwrap();
        assert!(revoked.revoked_at.is_some());
        assert_eq!(store.load(&p.token), None);
        assert!(update_frozen(&pool, dir.path(), revoked, "Rome", b"v3").await.is_err());
        assert_eq!(store.load(&p.token), None, "a revoked link stays dead");

        let never = create_frozen(&pool, dir.path(), "applet_user__rome", "Rome", b"x", Some(0))
            .await
            .unwrap();
        assert!(never.expires_at.is_none());
        assert_eq!(list(&pool).await.unwrap().len(), 2);
    }
}

//! Publications: what the owner has shared (migration 0043).
//!
//! Publishing freezes an applet's face into a bundle the door serves
//! (`virtues-door`), records it in `app_publications`, and hands back a link.
//! The door has no database access, so the bundle directory is what is live
//! and this table is the owner's record of it: updating rewrites the bundle
//! under the same token, revoking deletes it.
//!
//! A face that reads data (`virtues.query`) shares in one of two ways. A
//! **snapshot** carries the rows its queries returned when the owner last
//! looked at it, baked into the page, and asks the box for nothing. A **live**
//! page carries the same queries' keys and asks the box, through the loader
//! and the door, each time someone opens it; the core runs a query only if
//! its key is one approved for that link ([`answer`]). Either way the face's
//! `virtues.js` is replaced by a small inline stand-in ([`freeze`]).
//!
//! Anything else that only works on the box (`/api/`, a face token, a
//! localhost URL) is refused, as is any face for the GitHub path, which has
//! no door to answer queries ([`standalone_face`]).

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

/// The face runtime, which [`freeze`] replaces when sharing through the door.
const RUNTIME: &[&str] = &["virtues.query", "virtues.js", "virtues.css"];

/// Every reason `html` would not stand on its own off the box.
pub fn box_only_findings(html: &str) -> Vec<String> {
    findings(html, |_| true)
}

/// The reasons that survive [`freeze`]: everything but the face runtime.
fn server_only_findings(html: &str) -> Vec<String> {
    findings(html, |needle| !RUNTIME.contains(&needle))
}

fn findings(html: &str, keep: impl Fn(&str) -> bool) -> Vec<String> {
    let lower = html.to_lowercase();
    BOX_ONLY
        .iter()
        .filter(|(needle, _)| keep(needle) && lower.contains(needle))
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

/// Where the door key lives in `box_secrets`, sealed with the vault key, so
/// it travels with database backups. Losing it ends every link the box has
/// shared.
const DOOR_KEY_SECRET: &str = "door_key";

fn door_id_cache() -> &'static std::sync::RwLock<Option<String>> {
    static ID: std::sync::OnceLock<std::sync::RwLock<Option<String>>> = std::sync::OnceLock::new();
    ID.get_or_init(|| std::sync::RwLock::new(None))
}

/// The door's EndpointId (hex), once [`door_key`] has run in this process.
fn door_id() -> Option<String> {
    door_id_cache().read().ok()?.clone()
}

/// The door's secret key: from `box_secrets`, else adopted from a key file a
/// door made for itself before the key moved here (so its links keep
/// working), else new. Also caches the door's EndpointId for links.
pub async fn door_key(pool: &PgPool) -> Result<[u8; 32]> {
    let decode = |hex_key: &str| -> Option<[u8; 32]> {
        hex::decode(hex_key.trim()).ok()?.try_into().ok()
    };
    let stored = crate::box_secrets::get(pool, DOOR_KEY_SECRET)
        .await
        .map_err(|e| Error::Other(format!("read the door key: {e:#}")))?;
    let seed = match stored.and_then(|(secret, _)| decode(&secret)) {
        Some(seed) => seed,
        None => {
            let adopted = std::fs::read(door_dir().join("door.key"))
                .ok()
                .and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok());
            let seed = match adopted {
                Some(seed) => seed,
                None => {
                    use ring::rand::SecureRandom;
                    let mut seed = [0u8; 32];
                    ring::rand::SystemRandom::new()
                        .fill(&mut seed)
                        .map_err(|_| Error::Other("the system random source failed".into()))?;
                    seed
                }
            };
            let id = virtues_iroh::SecretKey::from_bytes(&seed).public().to_string();
            crate::box_secrets::put_if_absent(
                pool,
                DOOR_KEY_SECRET,
                &hex::encode(seed),
                &serde_json::json!({ "endpoint_id": id }),
            )
            .await
            .map_err(|e| Error::Other(format!("store the door key: {e:#}")))?;
            // Another caller may have stored one first; theirs wins.
            let (secret, _) = crate::box_secrets::get(pool, DOOR_KEY_SECRET)
                .await
                .map_err(|e| Error::Other(format!("read the door key: {e:#}")))?
                .ok_or_else(|| Error::Other("the door key vanished after storing it".into()))?;
            decode(&secret).ok_or_else(|| Error::Other("the door key in box_secrets is not 32 bytes of hex".into()))?
        }
    };
    let id = virtues_iroh::SecretKey::from_bytes(&seed).public().to_string();
    if let Ok(mut cached) = door_id_cache().write() {
        *cached = Some(id);
    }
    Ok(seed)
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
    /// A live page asks the server for its data when opened; otherwise it
    /// carries a snapshot (or reads no data at all).
    pub is_live: bool,
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
     created_at, updated_at, expires_at, revoked_at, open_count, last_opened_at, \
     EXISTS (SELECT 1 FROM app_publication_queries q \
             WHERE q.publication_id = app_publications.id) AS is_live";

/// The page in `face_dir`, if it stands alone off the box with nothing to
/// stand in for. `Err` names what to change.
pub fn standalone_html(face_dir: &Path) -> Result<String> {
    let html = read_face(face_dir)?;
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

/// The page in `face_dir` if sharing it through the door can work, and
/// whether it reads data.
fn shareable_html(face_dir: &Path) -> Result<(String, bool)> {
    let html = read_face(face_dir)?;
    let findings = server_only_findings(&html);
    if !findings.is_empty() {
        return Err(Error::InvalidInput(format!(
            "this page only works on your server: {}. Ask the assistant to change that part, \
             then share it again.",
            findings.join("; ")
        )));
    }
    let reads = html.to_lowercase().contains("virtues.query");
    Ok((html, reads))
}

/// One face file, within the size limit.
fn read_face(face_dir: &Path) -> Result<String> {
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
    Ok(html)
}

/// A query a shared page runs: the key the page sends, the SQL the core
/// runs for that key, and (for a snapshot) the rows baked in.
#[derive(Debug, Clone)]
pub struct FrozenQuery {
    pub key: String,
    pub sql: String,
    pub rows: serde_json::Value,
}

/// The key a shared page sends for `sql`: never the SQL itself, so the only
/// statements the core will run for a link are the ones stored for it.
fn query_key(sql: &str) -> String {
    hex::encode(&Sha256::digest(sql.as_bytes())[..16])
}

/// JSON that is safe inside a `<script>` element.
fn script_json(v: &serde_json::Value) -> String {
    v.to_string().replace('<', "\\u003c")
}

const SHIM: &str = r#"(function () {
  var KEYS = __KEYS__;
  var ROWS = __ROWS__;
  var LIVE = __LIVE__;
  var dark = window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches;
  var theme = dark ? 'dark' : 'light';
  document.documentElement.dataset.theme = theme;
  var pending = {};
  var next = 0;
  window.addEventListener('message', function (e) {
    var r = e.data && e.data.virtuesReply;
    if (!r || !pending[r.id]) return;
    var p = pending[r.id];
    delete pending[r.id];
    if (r.rows) p.resolve(r.rows);
    else p.reject(new Error("The server that shared this page couldn't send this data."));
  });
  function query(sql) {
    var key = KEYS[sql];
    if (!key) return Promise.reject(new Error('This page shows only what its owner shared.'));
    if (!LIVE) return Promise.resolve(ROWS[key]);
    return new Promise(function (resolve, reject) {
      var id = String(++next);
      pending[id] = { resolve: resolve, reject: reject };
      parent.postMessage({ virtuesQuery: { id: id, key: key } }, '*');
    });
  }
  window.virtues = { query: query, theme: theme, surface: '' };
})();"#;

/// Replace the face runtime in `html` with the inline stand-in. A snapshot
/// carries each query's rows; a live page carries only keys.
pub fn freeze(html: &str, queries: &[FrozenQuery], live: bool) -> Result<String> {
    use std::sync::OnceLock;
    static SCRIPT: OnceLock<regex::Regex> = OnceLock::new();
    static LINK: OnceLock<regex::Regex> = OnceLock::new();
    let script = SCRIPT.get_or_init(|| {
        regex::Regex::new(r#"(?is)<script\b[^>]*\bsrc\s*=\s*["']?(?:\./)?virtues\.js["']?[^>]*>\s*</script>"#).unwrap()
    });
    let link = LINK.get_or_init(|| {
        regex::Regex::new(r#"(?is)<link\b[^>]*\bhref\s*=\s*["']?(?:\./)?virtues\.css["']?[^>]*>"#).unwrap()
    });
    let keys: serde_json::Map<String, serde_json::Value> =
        queries.iter().map(|q| (q.sql.clone(), q.key.clone().into())).collect();
    let rows: serde_json::Map<String, serde_json::Value> = if live {
        serde_json::Map::new()
    } else {
        queries.iter().map(|q| (q.key.clone(), q.rows.clone())).collect()
    };
    let shim = SHIM
        .replace("__KEYS__", &script_json(&keys.into()))
        .replace("__ROWS__", &script_json(&rows.into()))
        .replace("__LIVE__", if live { "true" } else { "false" });
    // Placeholders first, so the check below sees only what the face itself
    // still references (the inlined stylesheet names itself in a comment).
    const SHIM_AT: &str = "\u{0}virtues-shim\u{0}";
    const CSS_AT: &str = "\u{0}virtues-css\u{0}";
    let marked = script.replace_all(html, regex::NoExpand(SHIM_AT));
    let marked = link.replace_all(&marked, regex::NoExpand(CSS_AT));
    let lower = marked.to_lowercase();
    if lower.contains("virtues.js") || lower.contains("virtues.css") {
        return Err(Error::InvalidInput(
            "this page loads virtues.js or virtues.css in a way sharing can't replace. Ask the \
             assistant to load them with plain <script src> and <link href> tags."
                .into(),
        ));
    }
    let out = marked
        .replace(SHIM_AT, &format!("<script>{shim}</script>"))
        .replace(CSS_AT, &format!("<style>{}</style>", crate::server::faces::VIRTUES_CSS));
    if out.len() > MAX_FACE_BYTES {
        return Err(Error::InvalidInput(format!(
            "with its data the page is {} KB; the limit is {} KB. Show fewer rows, then share it again.",
            out.len() / 1024,
            MAX_FACE_BYTES / 1024
        )));
    }
    Ok(out)
}

/// Run each query this applet's face ran recently, for a snapshot or for the
/// owner to see what a live page would show today.
async fn run_seen_queries(pool: &PgPool, applet_id: &str) -> Result<Vec<FrozenQuery>> {
    let seen = crate::server::faces::seen_queries(applet_id);
    if seen.is_empty() {
        return Err(Error::InvalidInput(
            "open this page in virtues once so your server can see what it shows, then share it."
                .into(),
        ));
    }
    let mut out = Vec::with_capacity(seen.len());
    for sql in seen {
        let rows = crate::server::faces::run_face_query(pool, &sql).await.map_err(|e| {
            Error::InvalidInput(format!(
                "one of this page's queries failed just now ({e}). Open the page again, then share it."
            ))
        })?;
        out.push(FrozenQuery { key: query_key(&sql), sql, rows });
    }
    Ok(out)
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
    /// Text that looks like contact details: emails and phone numbers,
    /// in the page or in the rows it carries.
    pub looks_private: Vec<String>,
    /// Whether the page reads data, and so can be a snapshot or live.
    pub reads_data: bool,
    /// The queries it runs, with what each returns right now.
    pub queries: Vec<QueryPreview>,
}

#[derive(Debug, Serialize)]
pub struct QueryPreview {
    pub sql: String,
    pub row_count: usize,
    /// The first few rows, as they would leave.
    pub sample: serde_json::Value,
}

impl SharePreview {
    fn cannot(title: String, why: String) -> Self {
        // The same sentences are API errors, which start lowercase; on the
        // sheet they stand alone.
        let mut chars = why.chars();
        let why = chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or(why);
        SharePreview {
            title,
            html: None,
            problem: Some(why),
            size_bytes: 0,
            image_count: 0,
            links: Vec::new(),
            looks_private: Vec::new(),
            reads_data: false,
            queries: Vec::new(),
        }
    }
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

/// What sharing this applet's face would send. For a page that reads data,
/// `html` is the snapshot, which is also what a live page shows today.
pub async fn preview(pool: &PgPool, applet_id: &str) -> Result<SharePreview> {
    let name = crate::scheduler::applets::get_applet(pool, applet_id)
        .await
        .map(|a| a.name)
        .map_err(|_| Error::NotFound(format!("no applet {applet_id:?}")))?;
    let Some(face_dir) = crate::server::faces::face_dir_for(applet_id) else {
        return Ok(SharePreview::cannot(
            name,
            "This applet has nothing to show, so there is no page to share.".into(),
        ));
    };
    let built = async {
        let (html, reads) = shareable_html(&face_dir)?;
        let queries = if reads { run_seen_queries(pool, applet_id).await? } else { Vec::new() };
        let page = if reads || html.to_lowercase().contains("virtues.") {
            freeze(&html, &queries, false)?
        } else {
            html
        };
        Ok::<_, Error>((page, reads, queries))
    };
    Ok(match built.await {
        Ok((page, reads, queries)) => {
            let (image_count, links, looks_private) = scan(&page);
            SharePreview {
                title: name,
                size_bytes: page.len(),
                html: Some(page),
                problem: None,
                image_count,
                links,
                looks_private,
                reads_data: reads,
                queries: queries
                    .into_iter()
                    .map(|q| {
                        let all = q.rows.as_array().cloned().unwrap_or_default();
                        QueryPreview {
                            sql: q.sql,
                            row_count: all.len(),
                            sample: all.into_iter().take(3).collect::<Vec<_>>().into(),
                        }
                    })
                    .collect(),
            }
        }
        Err(Error::InvalidInput(why)) => SharePreview::cannot(name, why),
        Err(e) => return Err(e),
    })
}

/// The page to publish for an applet, and the queries a live page may ask
/// for (empty for a snapshot or a page that reads nothing).
async fn build(pool: &PgPool, applet_id: &str, live: bool) -> Result<(String, String, Vec<FrozenQuery>)> {
    let name = crate::scheduler::applets::get_applet(pool, applet_id)
        .await
        .map(|a| a.name)
        .map_err(|_| Error::NotFound(format!("no applet {applet_id:?}")))?;
    let face_dir = crate::server::faces::face_dir_for(applet_id)
        .ok_or_else(|| Error::InvalidInput(format!("\"{name}\" has no face to publish")))?;
    let (html, reads) = shareable_html(&face_dir)?;
    if !reads {
        let page = if html.to_lowercase().contains("virtues.") { freeze(&html, &[], false)? } else { html };
        return Ok((name, page, Vec::new()));
    }
    let queries = run_seen_queries(pool, applet_id).await?;
    let page = freeze(&html, &queries, live)?;
    Ok((name, page, if live { queries } else { Vec::new() }))
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
    /// For a page that reads data: ask the server each time it is opened,
    /// rather than carry a snapshot. Ignored for a page that reads nothing.
    #[serde(default)]
    pub live: bool,
}

/// Share an applet's face: freeze it, put the bundle where the door serves
/// it, record it.
pub async fn create(pool: &PgPool, req: CreateRequest) -> Result<Publication> {
    door_key(pool).await?;
    let (title, page, live) = build(pool, &req.applet_id, req.live).await?;
    create_frozen(pool, &bundles_dir(), &req.applet_id, &title, page.as_bytes(), req.expires_in_days, &live)
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
    live: &[FrozenQuery],
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
            (id, token, producer_kind, producer_id, title, content_hash, size_bytes, expires_at, page) \
         VALUES ($1, $2, 'applet', $3, $4, $5, $6, $7, $8) RETURNING {COLUMNS}"
    ))
    .bind(&id)
    .bind(&token)
    .bind(applet_id)
    .bind(title)
    .bind(content_hash(page))
    .bind(page.len() as i64)
    .bind(expires_at)
    .bind(page)
    .fetch_one(pool)
    .await;
    let inserted = match inserted {
        Ok(p) => replace_queries(pool, &p.id, live).await.map(|()| Publication { is_live: !live.is_empty(), ..p }),
        Err(e) => Err(Error::Database(format!("record publication: {e}"))),
    };
    match inserted {
        Ok(p) => {
            crate::door::wake();
            Ok(p)
        }
        Err(e) => {
            let _ = bundle::remove(root, &token);
            Err(e)
        }
    }
}

/// Make `queries` the whole set a link may ask for.
async fn replace_queries(pool: &PgPool, publication_id: &str, queries: &[FrozenQuery]) -> Result<()> {
    let mut tx = pool.begin().await.map_err(|e| Error::Database(e.to_string()))?;
    sqlx::query("DELETE FROM app_publication_queries WHERE publication_id = $1")
        .bind(publication_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("clear approved queries: {e}")))?;
    for q in queries {
        sqlx::query(
            "INSERT INTO app_publication_queries (publication_id, query_key, sql) VALUES ($1, $2, $3) \
             ON CONFLICT DO NOTHING",
        )
        .bind(publication_id)
        .bind(&q.key)
        .bind(&q.sql)
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("approve query: {e}")))?;
    }
    tx.commit().await.map_err(|e| Error::Database(e.to_string()))
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
    door_key(pool).await?;
    let rows = sqlx::query_as::<_, Publication>(&format!(
        "SELECT {COLUMNS} FROM app_publications ORDER BY created_at DESC"
    ))
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("list publications: {e}")))?;
    Ok(rows.into_iter().map(Publication::with_link).collect())
}

/// Re-freeze the applet's face under the same link.
/// A live link stays live and a snapshot takes a new snapshot.
pub async fn update(pool: &PgPool, id: &str) -> Result<Publication> {
    door_key(pool).await?;
    let current = get(pool, id).await?;
    let was_live: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM app_publication_queries WHERE publication_id = $1)",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("read approved queries: {e}")))?;
    let (title, page, live) = build(pool, &current.producer_id, was_live).await?;
    let updated = update_frozen(pool, &bundles_dir(), current, &title, page.as_bytes()).await?;
    replace_queries(pool, &updated.id, &live).await?;
    Ok(Publication { is_live: !live.is_empty(), ..updated }.with_link())
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
        "UPDATE app_publications SET title = $2, content_hash = $3, size_bytes = $4, page = $5, \
            updated_at = now() WHERE id = $1 RETURNING {COLUMNS}"
    ))
    .bind(&current.id)
    .bind(title)
    .bind(content_hash(page))
    .bind(page.len() as i64)
    .bind(page)
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

/// Make the bundle directory match the database: every live link's page
/// written (unless the copy on disk already matches), and every revoked or
/// unknown bundle removed. Rows from before pages were stored (`page IS
/// NULL`) keep whatever bundle they have. This is what lets a restored
/// database bring its links back.
pub async fn materialize(pool: &PgPool, root: &Path) -> Result<()> {
    let rows: Vec<(String, Option<Vec<u8>>, String, Option<DateTime<Utc>>, bool)> = sqlx::query_as(
        "SELECT token, page, content_hash, expires_at, \
                (revoked_at IS NULL AND (expires_at IS NULL OR expires_at > now())) AS live \
         FROM app_publications",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("read publications: {e}")))?;

    let mut keep = std::collections::HashSet::new();
    for (token, page, hash, expires_at, live) in rows {
        if !live {
            continue;
        }
        keep.insert(token.clone());
        let Some(page) = page else { continue };
        let on_disk = std::fs::read(root.join(&token).join("index.html")).ok();
        if on_disk.as_deref().map(content_hash).as_deref() == Some(hash.as_str()) {
            continue;
        }
        bundle::write(root, &token, &page, &meta_for(expires_at)).map_err(door_error)?;
    }
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if token::is_valid(&name) && !keep.contains(&name) {
                bundle::remove(root, &name).map_err(door_error)?;
            }
        }
    }
    Ok(())
}

/// What the core tells the door (`virtues_door::core`): rows for a query
/// approved for a live link, and a count when a page is served. A dead link
/// or an unknown key is `ok: false`, the same as any failure.
pub async fn answer(pool: &PgPool, request: virtues_door::core::CoreRequest) -> virtues_door::core::CoreAnswer {
    use virtues_door::core::{CoreAnswer, CoreRequest};
    let no = CoreAnswer { ok: false, rows: None };
    match request {
        CoreRequest::Query { token, key } => {
            let sql: Option<String> = match sqlx::query_scalar(
                "SELECT q.sql FROM app_publication_queries q \
                 JOIN app_publications p ON p.id = q.publication_id \
                 WHERE p.token = $1 AND q.query_key = $2 AND p.revoked_at IS NULL \
                   AND (p.expires_at IS NULL OR p.expires_at > now())",
            )
            .bind(&token)
            .bind(&key)
            .fetch_optional(pool)
            .await
            {
                Ok(sql) => sql,
                Err(e) => {
                    tracing::warn!(error = %e, "door: approved-query lookup failed");
                    return no;
                }
            };
            let Some(sql) = sql else { return no };
            match crate::server::faces::run_face_query(pool, &sql).await {
                Ok(rows) => CoreAnswer { ok: true, rows: Some(rows) },
                Err(e) => {
                    tracing::warn!(error = %e, "door: an approved query failed");
                    no
                }
            }
        }
        CoreRequest::Opened { token } => {
            match sqlx::query(
                "UPDATE app_publications SET open_count = open_count + 1, last_opened_at = now() \
                 WHERE token = $1 AND revoked_at IS NULL",
            )
            .bind(&token)
            .execute(pool)
            .await
            {
                Ok(_) => CoreAnswer { ok: true, rows: None },
                Err(e) => {
                    tracing::warn!(error = %e, "door: could not count an open");
                    no
                }
            }
        }
    }
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

        let p = create_frozen(&pool, dir.path(), "applet_user__rome", "Rome", b"v1", None, &[])
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

        let never = create_frozen(&pool, dir.path(), "applet_user__rome", "Rome", b"x", Some(0), &[])
            .await
            .unwrap();
        assert!(never.expires_at.is_none());
        assert_eq!(list(&pool).await.unwrap().len(), 2);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_live_link_answers_only_its_approved_queries(pool: PgPool) {
        use virtues_door::core::CoreRequest;
        let dir = tempfile::tempdir().unwrap();
        let approved = FrozenQuery { key: query_key("SELECT 1 AS n"), sql: "SELECT 1 AS n".into(), rows: serde_json::Value::Null };
        let p = create_frozen(&pool, dir.path(), "applet_user__rome", "Rome", b"x", None, &[approved.clone()])
            .await
            .unwrap();

        let ask = |key: &str| CoreRequest::Query { token: p.token.clone(), key: key.into() };
        let got = answer(&pool, ask(&approved.key)).await;
        assert!(got.ok);
        assert_eq!(got.rows.unwrap(), serde_json::json!([{ "n": 1 }]));
        assert!(!answer(&pool, ask(&query_key("SELECT 2"))).await.ok, "not approved");
        let other = CoreRequest::Query { token: token::generate(), key: approved.key.clone() };
        assert!(!answer(&pool, other).await.ok, "another link's token");

        assert!(answer(&pool, CoreRequest::Opened { token: p.token.clone() }).await.ok);
        assert_eq!(get(&pool, &p.id).await.unwrap().open_count, 1);

        revoke_in(&pool, dir.path(), &p.id).await.unwrap();
        assert!(!answer(&pool, ask(&approved.key)).await.ok, "a revoked link answers nothing");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_restored_database_rebuilds_its_bundles(pool: PgPool) {
        let dir = tempfile::tempdir().unwrap();
        let store = bundle::Store::new(dir.path());
        let kept = create_frozen(&pool, dir.path(), "applet_user__rome", "Rome", b"kept", None, &[]).await.unwrap();
        let gone = create_frozen(&pool, dir.path(), "applet_user__rome", "Rome", b"gone", None, &[]).await.unwrap();
        revoke_in(&pool, dir.path(), &gone.id).await.unwrap();

        // A fresh disk, as after a restore onto a new box, plus a stray bundle
        // no row knows about and the revoked one somehow back.
        let fresh = tempfile::tempdir().unwrap();
        bundle::write(fresh.path(), &token::generate(), b"stray", &bundle::Meta::default()).unwrap();
        bundle::write(fresh.path(), &gone.token, b"gone", &bundle::Meta::default()).unwrap();
        materialize(&pool, fresh.path()).await.unwrap();

        let fresh_store = bundle::Store::new(fresh.path());
        assert_eq!(fresh_store.load(&kept.token).as_deref(), Some(&b"kept"[..]));
        assert_eq!(fresh_store.load(&gone.token), None);
        assert_eq!(std::fs::read_dir(fresh.path()).unwrap().count(), 1, "only the live bundle remains");
        assert_eq!(store.load(&kept.token).as_deref(), Some(&b"kept"[..]));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn the_door_key_is_minted_once_and_kept(pool: PgPool) {
        let a = door_key(&pool).await.unwrap();
        let b = door_key(&pool).await.unwrap();
        assert_eq!(a, b);
        let id = virtues_iroh::SecretKey::from_bytes(&a).public().to_string();
        assert_eq!(door_id().as_deref(), Some(id.as_str()));
    }

    #[test]
    fn freezing_replaces_the_runtime_and_carries_rows_or_keys() {
        let face = r#"<html><head><link rel="stylesheet" href="virtues.css"><script src="virtues.js"></script></head>
            <body><script>virtues.query("SELECT 1").then(r => document.body.dataset.n = r[0].n)</script></body></html>"#;
        let q = FrozenQuery { key: query_key("SELECT 1"), sql: "SELECT 1".into(), rows: serde_json::json!([{ "n": "</script>" }]) };
        let snap = freeze(face, &[q.clone()], false).unwrap();
        assert!(!snap.contains("src=\"virtues.js\""));
        assert!(!snap.contains("href=\"virtues.css\""));
        assert!(snap.contains(&q.key));
        assert!(snap.contains("\\u003c/script>"), "rows cannot close the script element");
        assert!(snap.contains("var LIVE = false"));
        let live = freeze(face, &[q.clone()], true).unwrap();
        assert!(live.contains("var LIVE = true"));
        assert!(!live.contains("u003c/script>"), "a live page carries no rows");
        assert!(server_only_findings(face).is_empty());
        assert_eq!(box_only_findings(face).len(), 3);
    }
}

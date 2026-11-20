//! Home backgrounds: the styles installed on this server, the ones it can
//! download, and their files.
//!
//! A style is a folder under [`state_dir`] holding a `manifest.json` and the
//! files it names (`tools/backgrounds/build.py` writes both). The apps read
//! the manifests to resolve the background setting, and load the pictures
//! from this server only. See agents/plan/backgrounds-plan.md.
//!
//! Styles this server does not have are listed in a catalog
//! (`tools/backgrounds/package.py` writes it) that comes from virtues-api,
//! `<api>/v1/backgrounds/catalog.json`, or from `VIRTUES_BACKGROUNDS_SOURCE`
//! for a self-hosted copy. When you pick one, this server downloads its
//! archive once, checks its size and sha256, unpacks it beside the others and
//! serves it from then on. When the catalog later names a newer archive for
//! a style this server downloaded, the server fetches it the same way and
//! keeps serving the old copy until the new one is in. A request to
//! virtues-api carries this server's key, as every request to it does; the
//! catalog request names nothing and an archive request names a style, (docs/use/backgrounds.md, agents/build/privacy-boundary.md).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

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
    /// Styles in the catalog that this server does not have yet.
    pub available: Vec<Available>,
    /// Downloads in progress, by style.
    pub installing: HashMap<String, Progress>,
    /// The last download that failed, by style, as a sentence for the person.
    pub failed: HashMap<String, String>,
    /// Whether the catalog is being fetched now, so a picker can ask again
    /// shortly for styles it does not list yet.
    pub catalog_pending: bool,
}

/// Downloads in progress and failed, without the manifests and previews, for
/// a picker that is following a download.
#[derive(Debug, Serialize)]
pub struct Downloads {
    pub installing: HashMap<String, Progress>,
    pub failed: HashMap<String, String>,
}

/// A style the server can download.
#[derive(Debug, Clone, Serialize)]
pub struct Available {
    pub style: String,
    pub title: String,
    pub bytes: u64,
    /// One small picture per appearance, as `data:` URLs from the catalog.
    pub preview: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub received_bytes: u64,
    pub total_bytes: u64,
}

/// The installed styles, the ones the catalog offers, and any download in
/// progress or failed. Never waits for the catalog: a missing or day-old one
/// is fetched in the background and `catalog_pending` says so. A style this
/// server downloaded whose archive the catalog has replaced is updated in the
/// background.
pub async fn list(pool: &PgPool) -> Result<Styles> {
    let root = state_dir();
    recover_interrupted_swaps(&root).await;
    let installed = installed_in(&root).await?;
    let have: Vec<String> = installed.iter().filter_map(|m| m["style"].as_str().map(String::from)).collect();
    let mut available = Vec::new();
    if let Some(catalog) = catalog(pool, &root).await {
        for entry in catalog.styles {
            if !have.contains(&entry.style) {
                available.push(Available {
                    style: entry.style,
                    title: entry.title,
                    bytes: entry.bytes,
                    preview: entry.preview,
                });
            } else if installed_sha(&root, &entry.style).await.is_some_and(|sha| sha != entry.sha256) {
                update_in_background(pool, &root, entry).await;
            }
        }
    }
    let Downloads { installing, failed } = downloads();
    Ok(Styles { styles: installed, available, installing, failed, catalog_pending: REFRESHING.load(Ordering::SeqCst) })
}

/// The sha256 of the archive an installed style came from, or `None` for a
/// style put in place by hand (the build tool's output in a dev checkout),
/// which is never replaced.
async fn installed_sha(root: &Path, style: &str) -> Option<String> {
    let raw = tokio::fs::read(root.join(style).join(".archive.json")).await.ok()?;
    let v: serde_json::Value = serde_json::from_slice(&raw).ok()?;
    v["sha256"].as_str().map(String::from)
}

/// Put back a style whose swap stopped between its two renames (the server
/// stopped there), so it is not missing until someone downloads it again.
async fn recover_interrupted_swaps(root: &Path) {
    let Ok(mut entries) = tokio::fs::read_dir(root).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(style) = name.strip_prefix(".old-") else {
            continue;
        };
        if is_style_id(style) && !root.join(style).exists() {
            match tokio::fs::rename(entry.path(), root.join(style)).await {
                Ok(()) => tracing::info!("backgrounds: put back {style} after an interrupted update"),
                Err(e) => tracing::warn!("backgrounds: putting back {style}: {e}"),
            }
        }
    }
}

/// Every installed style whose manifest parses, has this server's version,
/// and names the folder it is in. A style that fails one of those is logged
/// and left out, so one bad folder cannot take the others down.
async fn installed_in(root: &Path) -> Result<Vec<serde_json::Value>> {
    let mut styles = Vec::new();
    let mut entries = match tokio::fs::read_dir(root).await {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(styles),
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
    Ok(styles)
}

/// Check an unpacked style before it goes live: a manifest this server can
/// read, naming the style it was downloaded as.
fn check_manifest(dir: &Path, style: &str) -> std::result::Result<(), String> {
    let text = std::fs::read_to_string(dir.join("manifest.json")).map_err(|_| "it has no manifest".to_string())?;
    let manifest: serde_json::Value = serde_json::from_str(&text).map_err(|_| "its manifest does not parse".to_string())?;
    if manifest.get("version").and_then(|v| v.as_u64()) != Some(MANIFEST_VERSION) {
        return Err(format!("its manifest is not version {MANIFEST_VERSION}"));
    }
    if manifest.get("style").and_then(|v| v.as_str()) != Some(style) {
        return Err("its manifest names a different style".to_string());
    }
    Ok(())
}

// ── The catalog ────────────────────────────────────────────────────────────

/// How old the cached catalog may be before the next listing refreshes it.
const CATALOG_MAX_AGE: Duration = Duration::from_secs(24 * 3600);
/// How long after a failed catalog fetch the next one may start, so a server
/// that cannot reach the source does not ask on every listing.
const CATALOG_RETRY_AFTER: Duration = Duration::from_secs(3600);
/// The most a catalog may weigh. Today's, with previews, is about 130 KB.
const CATALOG_MAX_BYTES: u64 = 4 * 1024 * 1024;
/// The most one preview may weigh, as a `data:` URL.
const PREVIEW_MAX_CHARS: usize = 256 * 1024;

#[derive(Debug, Clone, Deserialize)]
struct Catalog {
    version: u64,
    styles: Vec<CatalogStyle>,
}

#[derive(Debug, Clone, Deserialize)]
struct CatalogStyle {
    style: String,
    title: String,
    /// The archive's file name next to the catalog: `<style>-<hash>.tar`.
    archive: String,
    bytes: u64,
    sha256: String,
    #[serde(default)]
    preview: serde_json::Value,
}

/// Where the catalog and archives come from: `VIRTUES_BACKGROUNDS_SOURCE` (a
/// self-hosted copy laid out as `package.py` writes it), else virtues-api.
fn source_base() -> (String, bool) {
    match std::env::var("VIRTUES_BACKGROUNDS_SOURCE") {
        Ok(s) if !s.is_empty() => (s.trim_end_matches('/').to_string(), true),
        _ => (format!("{}/v1/backgrounds", crate::virtues_api::api_url().trim_end_matches('/')), false),
    }
}

fn http() -> std::result::Result<reqwest::Client, String> {
    crate::http_client::base_builder()
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(3600))
        .build()
        .map_err(|_| "could not set up the download".to_string())
}

/// The bearer for a request to the source: this server's key for virtues-api,
/// nothing for a self-hosted source.
async fn bearer(pool: &PgPool, self_hosted: bool) -> Option<String> {
    if self_hosted {
        return None;
    }
    match crate::virtues_api::box_key(pool).await {
        Ok(key) => key,
        Err(e) => {
            tracing::warn!("backgrounds: reading this server's key failed: {e:#}");
            None
        }
    }
}

/// A catalog entry this server may act on: a plain style id and an archive
/// named after it.
fn entry_is_sound(e: &CatalogStyle) -> bool {
    is_style_id(&e.style)
        && e.archive.starts_with(&format!("{}-", e.style))
        && e.archive.strip_suffix(".tar").is_some_and(|stem| {
            stem.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
        && e.sha256.len() == 64
        && e.sha256.chars().all(|c| c.is_ascii_hexdigit())
}

fn parse_catalog(bytes: &[u8]) -> Option<Catalog> {
    let mut catalog: Catalog = serde_json::from_slice(bytes).ok()?;
    if catalog.version != 1 {
        return None;
    }
    catalog.styles.retain(entry_is_sound);
    for entry in &mut catalog.styles {
        entry.preview = sound_preview(&entry.preview);
    }
    Some(catalog)
}

/// Keep only previews the apps can draw without asking another host: small
/// `data:image/` URLs, one per appearance.
fn sound_preview(preview: &serde_json::Value) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    for theme in ["light", "dark"] {
        if let Some(url) = preview.get(theme).and_then(|v| v.as_str()) {
            if url.starts_with("data:image/") && url.len() <= PREVIEW_MAX_CHARS {
                out.insert(theme.to_string(), serde_json::Value::String(url.to_string()));
            }
        }
    }
    serde_json::Value::Object(out)
}

static REFRESHING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Clears `REFRESHING` when a refresh ends, however it ends.
struct RefreshDone;

impl Drop for RefreshDone {
    fn drop(&mut self) {
        REFRESHING.store(false, Ordering::SeqCst);
    }
}

async fn age_of(path: &Path) -> Option<Duration> {
    tokio::fs::metadata(path).await.ok()?.modified().ok()?.elapsed().ok()
}

/// The cached catalog. When it is missing or a day old, and no fetch started
/// in the last hour, a fetch starts in the background; this never waits for
/// it. `None` when there is no catalog yet (offline, no account, none
/// published, or the first fetch still running).
async fn catalog(pool: &PgPool, root: &Path) -> Option<Catalog> {
    let path = root.join(".catalog.json");
    let cached = tokio::fs::read(&path).await.ok().and_then(|b| parse_catalog(&b));
    let stale = age_of(&path).await.is_none_or(|a| a > CATALOG_MAX_AGE);
    let tried_recently = age_of(&root.join(".catalog-attempt")).await.is_some_and(|a| a < CATALOG_RETRY_AFTER);
    if stale && !tried_recently && !REFRESHING.swap(true, Ordering::SeqCst) {
        let _ = tokio::fs::create_dir_all(root).await;
        if let Err(e) = tokio::fs::write(root.join(".catalog-attempt"), b"").await {
            tracing::warn!("backgrounds: noting the catalog fetch: {e}");
        }
        let (pool, root) = (pool.clone(), root.to_path_buf());
        tokio::spawn(async move {
            let _done = RefreshDone;
            refresh_catalog(&pool, &root).await;
        });
    }
    cached
}

async fn refresh_catalog(pool: &PgPool, root: &Path) -> Option<Catalog> {
    let (base, self_hosted) = source_base();
    let key = bearer(pool, self_hosted).await;
    if key.is_none() && !self_hosted {
        return None;
    }
    let http = http().ok()?;
    let mut req = http.get(format!("{base}/catalog.json")).timeout(Duration::from_secs(30));
    if let Some(k) = &key {
        req = req.bearer_auth(k);
    }
    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::info!("backgrounds: the catalog did not load: {}", e.without_url());
            return None;
        }
    };
    if !resp.status().is_success() {
        tracing::info!("backgrounds: the catalog answered {}", resp.status());
        return None;
    }
    if resp.content_length().is_some_and(|n| n > CATALOG_MAX_BYTES) {
        tracing::warn!("backgrounds: the catalog is larger than {CATALOG_MAX_BYTES} bytes; ignoring it");
        return None;
    }
    let bytes = resp.bytes().await.ok()?;
    if bytes.len() as u64 > CATALOG_MAX_BYTES {
        tracing::warn!("backgrounds: the catalog is larger than {CATALOG_MAX_BYTES} bytes; ignoring it");
        return None;
    }
    let catalog = parse_catalog(&bytes)?;
    if let Err(e) = tokio::fs::create_dir_all(root).await {
        tracing::warn!("backgrounds: creating {}: {e}", root.display());
        return Some(catalog);
    }
    let tmp = root.join(".catalog.json.tmp");
    if tokio::fs::write(&tmp, &bytes).await.is_ok() {
        let _ = tokio::fs::rename(&tmp, root.join(".catalog.json")).await;
    }
    Some(catalog)
}

// ── Installing ─────────────────────────────────────────────────────────────

enum InstallState {
    Downloading { received: Arc<AtomicU64>, total: u64 },
    Failed(String),
}

static INSTALLS: LazyLock<Mutex<HashMap<String, InstallState>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Downloads in progress and failed, for `GET /api/backgrounds/downloads`.
pub fn downloads() -> Downloads {
    let installs = INSTALLS.lock().unwrap_or_else(|e| e.into_inner());
    let mut progress = HashMap::new();
    let mut failed = HashMap::new();
    for (style, state) in installs.iter() {
        match state {
            InstallState::Downloading { received, total } => {
                progress.insert(
                    style.clone(),
                    Progress { received_bytes: received.load(Ordering::Relaxed), total_bytes: *total },
                );
            }
            InstallState::Failed(message) => {
                failed.insert(style.clone(), message.clone());
            }
        }
    }
    Downloads { installing: progress, failed }
}

/// Room to leave on the disk beyond the archive and its unpacked copy.
const MIN_FREE_BYTES: u64 = 512 * 1024 * 1024;

/// Start downloading and installing a style from the catalog. Returns at
/// once; the listing reports progress and the outcome.
pub async fn install(pool: &PgPool, style: &str) -> Result<()> {
    if !is_style_id(style) {
        return Err(Error::NotFound(format!("no background style {style}")));
    }
    let root = state_dir();
    let entry = catalog(pool, &root)
        .await
        .and_then(|c| c.styles.into_iter().find(|e| e.style == style))
        .ok_or_else(|| Error::NotFound("This style is not published any more.".to_string()))?;
    start_install(pool, &root, entry).await
}

/// Fetch a newer archive of a style this server already has. Skipped while
/// one is running, and after one failed in this run of the server, so a
/// failing update is not retried on every listing.
async fn update_in_background(pool: &PgPool, root: &Path, entry: CatalogStyle) {
    if INSTALLS.lock().unwrap_or_else(|e| e.into_inner()).contains_key(&entry.style) {
        return;
    }
    let style = entry.style.clone();
    match start_install(pool, root, entry).await {
        Ok(()) => tracing::info!("backgrounds: updating {style} to the catalog's newer archive"),
        Err(e) => tracing::warn!("backgrounds: not updating {style}: {e}"),
    }
}

async fn start_install(pool: &PgPool, root: &Path, entry: CatalogStyle) -> Result<()> {
    tokio::fs::create_dir_all(root.join(".download"))
        .await
        .map_err(|e| Error::Storage(format!("creating the backgrounds folder: {e}")))?;
    let free = crate::storage::lake::free_bytes_at(root).unwrap_or(u64::MAX);
    if free < entry.bytes.saturating_mul(2) + MIN_FREE_BYTES {
        return Err(Error::Storage("There is not enough free space on your server for this style.".to_string()));
    }
    let received = Arc::new(AtomicU64::new(0));
    {
        let mut installs = INSTALLS.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(installs.get(&entry.style), Some(InstallState::Downloading { .. })) {
            return Ok(());
        }
        installs.insert(entry.style.clone(), InstallState::Downloading { received: received.clone(), total: entry.bytes });
    }
    let (pool, root) = (pool.clone(), root.to_path_buf());
    tokio::spawn(async move {
        let style = entry.style.clone();
        let outcome = download_and_install(&pool, &root, entry, &received).await;
        let mut installs = INSTALLS.lock().unwrap_or_else(|e| e.into_inner());
        match outcome {
            Ok(()) => {
                installs.remove(&style);
                tracing::info!("backgrounds: installed {style}");
            }
            Err(message) => {
                installs.insert(style, InstallState::Failed(message));
            }
        }
    });
    Ok(())
}

/// Why a download did not produce the archive.
enum Fetch {
    /// The source has no such archive: the catalog this server holds is older
    /// than the one published.
    Gone,
    /// The bytes arrived but are not what the catalog describes.
    Mismatch,
    /// The connection failed or the source refused.
    Failed,
}

async fn download_and_install(
    pool: &PgPool,
    root: &Path,
    mut entry: CatalogStyle,
    received: &AtomicU64,
) -> std::result::Result<(), String> {
    const GONE: &str = "This style is not published any more.";
    for attempt in 0..2 {
        match fetch_archive(pool, root, &entry, received).await {
            Ok(archive) => return install_downloaded(root, &entry, archive).await,
            Err(Fetch::Gone) if attempt == 0 => {
                // Read the catalog again and try the archive it names now.
                let fresh = refresh_catalog(pool, root).await.and_then(|c| c.styles.into_iter().find(|e| e.style == entry.style));
                match fresh {
                    Some(fresh) if fresh.archive != entry.archive => {
                        received.store(0, Ordering::Relaxed);
                        if let Some(InstallState::Downloading { total, .. }) =
                            INSTALLS.lock().unwrap_or_else(|e| e.into_inner()).get_mut(&entry.style)
                        {
                            *total = fresh.bytes;
                        }
                        entry = fresh;
                    }
                    _ => return Err(GONE.to_string()),
                }
            }
            Err(Fetch::Gone) => return Err(GONE.to_string()),
            Err(Fetch::Mismatch) => return Err("The download did not match the published style. Try again later.".to_string()),
            Err(Fetch::Failed) => {
                return Err("The download did not finish. Check your server's connection and try again.".to_string())
            }
        }
    }
    Err(GONE.to_string())
}

/// Download one archive into `.download/`, resuming a partial copy of the
/// same archive. A partial file is named after its archive, so a resume can
/// never append one archive to another; partial files of the style's older
/// archives are removed first.
async fn fetch_archive(pool: &PgPool, root: &Path, entry: &CatalogStyle, received: &AtomicU64) -> std::result::Result<PathBuf, Fetch> {
    let dir = root.join(".download");
    let tmp_name = format!("{}.tmp", entry.archive);
    if let Ok(mut entries) = tokio::fs::read_dir(&dir).await {
        while let Ok(Some(f)) = entries.next_entry().await {
            let name = f.file_name().to_string_lossy().into_owned();
            if name.starts_with(&format!("{}-", entry.style)) && name != tmp_name && name != entry.archive {
                let _ = tokio::fs::remove_file(f.path()).await;
            }
        }
    }
    let (base, self_hosted) = source_base();
    let key = bearer(pool, self_hosted).await;
    let http = http().map_err(|_| Fetch::Failed)?;
    let archive = dir.join(&entry.archive);
    let expected = crate::downloads::Expected { bytes: entry.bytes, sha256: &entry.sha256, what: "background" };
    let url = format!("{base}/{}", entry.archive);
    match crate::downloads::verified(&http, &url, key.as_deref(), &dir.join(&tmp_name), &archive, &expected, Some(received)).await {
        Ok(()) => Ok(archive),
        Err(e) => {
            tracing::warn!("backgrounds: downloading {}: {e:#}", entry.style);
            let text = format!("{e:#}");
            if text.contains("answered 404") {
                Err(Fetch::Gone)
            } else if text.contains("checksum") || text.contains("expected size") {
                Err(Fetch::Mismatch)
            } else {
                Err(Fetch::Failed)
            }
        }
    }
}

async fn install_downloaded(root: &Path, entry: &CatalogStyle, archive: PathBuf) -> std::result::Result<(), String> {
    let (root_owned, style, sha) = (root.to_path_buf(), entry.style.clone(), entry.sha256.clone());
    let result = tokio::task::spawn_blocking(move || install_archive(&root_owned, &style, &archive, &sha)).await;
    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => {
            tracing::warn!("backgrounds: installing {}: {e}", entry.style);
            Err("The style could not be installed on your server. Try again.".to_string())
        }
        Err(e) => {
            tracing::warn!("backgrounds: the install task for {} stopped: {e}", entry.style);
            Err("The style could not be installed on your server. Try again.".to_string())
        }
    }
}

/// Unpack a downloaded archive beside the installed styles, check it, and put
/// it in place of any older copy. The archive is removed once it is in.
fn install_archive(root: &Path, style: &str, archive: &Path, sha256: &str) -> std::result::Result<(), String> {
    let staging = root.join(format!(".staging-{style}"));
    crate::downloads::unpack_without_links(archive, &staging, "background").map_err(|e| format!("{e}"))?;
    if let Err(e) = check_manifest(&staging, style) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }
    // Which archive this copy came from, for telling a newer one apart later.
    let source = serde_json::json!({ "sha256": sha256 });
    std::fs::write(staging.join(".archive.json"), source.to_string()).map_err(|e| format!("{e}"))?;
    crate::downloads::swap_in(&staging, &root.join(style), &root.join(format!(".old-{style}")))
        .map_err(|e| format!("{e}"))?;
    let _ = std::fs::remove_file(archive);
    Ok(())
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

        let styles = installed_in(root).await.unwrap();
        let ids: Vec<_> = styles.iter().map(|s| s["style"].as_str().unwrap()).collect();
        assert_eq!(ids, ["paintings-a", "paintings-b"]);
    }

    #[tokio::test]
    async fn a_missing_folder_is_no_styles() {
        let dir = tempfile::tempdir().unwrap();
        let styles = installed_in(&dir.path().join("absent")).await.unwrap();
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

    fn tar_of(dir: &Path, files: &[(&str, &[u8])]) -> PathBuf {
        let path = dir.join("style.tar");
        let mut b = tar::Builder::new(std::fs::File::create(&path).unwrap());
        for (name, body) in files {
            let mut h = tar::Header::new_gnu();
            h.set_size(body.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            b.append_data(&mut h, name, *body).unwrap();
        }
        b.finish().unwrap();
        path
    }

    #[test]
    fn installs_an_archive_in_place_of_the_old_copy() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("dots")).unwrap();
        std::fs::write(root.join("dots/stale.webp"), b"old").unwrap();
        let body = manifest("dots", 1);
        let archive = tar_of(root, &[("manifest.json", body.as_bytes()), ("forest/light/a-896.webp", b"new")]);

        install_archive(root, "dots", &archive, &"a".repeat(64)).unwrap();
        assert_eq!(std::fs::read(root.join("dots/forest/light/a-896.webp")).unwrap(), b"new");
        assert!(!root.join("dots/stale.webp").exists());
        assert!(root.join("dots/.archive.json").exists());
        assert!(!archive.exists());
        assert!(!root.join(".staging-dots").exists());
    }

    #[test]
    fn refuses_an_archive_for_another_style() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let body = manifest("other", 1);
        let archive = tar_of(root, &[("manifest.json", body.as_bytes())]);
        let err = install_archive(root, "dots", &archive, &"a".repeat(64)).unwrap_err();
        assert!(err.contains("different style"), "{err}");
        assert!(!root.join("dots").exists());
        assert!(!root.join(".staging-dots").exists());
    }

    #[test]
    fn the_catalog_keeps_only_sound_entries() {
        let sha = "b".repeat(64);
        let json = format!(
            r#"{{"version":1,"styles":[
                {{"style":"dots","title":"Dots","archive":"dots-1a2b.tar","bytes":10,"sha256":"{sha}"}},
                {{"style":"dots","title":"Dots","archive":"../dots.tar","bytes":10,"sha256":"{sha}"}},
                {{"style":"dots","title":"Dots","archive":"other-1.tar","bytes":10,"sha256":"{sha}"}},
                {{"style":"Bad Id","title":"x","archive":"x.tar","bytes":10,"sha256":"{sha}"}},
                {{"style":"dots","title":"Dots","archive":"dots-2.tar","bytes":10,"sha256":"short"}}
            ]}}"#
        );
        let catalog = parse_catalog(json.as_bytes()).unwrap();
        let archives: Vec<_> = catalog.styles.iter().map(|e| e.archive.as_str()).collect();
        assert_eq!(archives, ["dots-1a2b.tar"]);
        assert!(parse_catalog(br#"{"version":2,"styles":[]}"#).is_none());
    }

    #[test]
    fn previews_are_small_data_urls_only() {
        let big = format!("data:image/webp;base64,{}", "A".repeat(PREVIEW_MAX_CHARS));
        let preview = serde_json::json!({
            "light": "data:image/webp;base64,AAAA",
            "dark": "https://example.com/tracker.webp",
            "dusk": "data:image/webp;base64,AAAA",
        });
        assert_eq!(sound_preview(&preview), serde_json::json!({ "light": "data:image/webp;base64,AAAA" }));
        assert_eq!(sound_preview(&serde_json::json!({ "light": big })), serde_json::json!({}));
    }

    #[tokio::test]
    async fn an_interrupted_swap_is_put_back() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".old-dots")).unwrap();
        std::fs::write(root.join(".old-dots/manifest.json"), manifest("dots", 1)).unwrap();
        // A finished swap leaves both; the live copy wins and the old one stays for swap_in to clear.
        std::fs::create_dir_all(root.join(".old-waves")).unwrap();
        std::fs::create_dir_all(root.join("waves")).unwrap();

        recover_interrupted_swaps(root).await;
        assert!(root.join("dots/manifest.json").exists());
        assert!(!root.join(".old-dots").exists());
        assert!(root.join("waves").exists());
    }
}

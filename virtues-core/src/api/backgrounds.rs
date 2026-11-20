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
//! serves it from then on. A request to virtues-api carries this server's
//! key, as every request to it does; it says which style was picked and
//! nothing else (docs/backgrounds.md, agents/build/privacy-boundary.md).

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
/// progress or failed.
pub async fn list(pool: &PgPool) -> Result<Styles> {
    let root = state_dir();
    let installed = installed_in(&root).await?;
    let catalog = catalog(pool, &root).await;
    let have: Vec<&str> = installed.iter().filter_map(|m| m["style"].as_str()).collect();
    let available = catalog
        .map(|c| {
            c.styles
                .into_iter()
                .filter(|e| !have.contains(&e.style.as_str()))
                .map(|e| Available { style: e.style, title: e.title, bytes: e.bytes, preview: e.preview })
                .collect()
        })
        .unwrap_or_default();
    let (installing, failed) = install_states();
    Ok(Styles { styles: installed, available, installing, failed })
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
/// How long a listing waits for a first catalog when none is cached.
const FIRST_CATALOG_WAIT: Duration = Duration::from_secs(5);

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
    Some(catalog)
}

static REFRESHING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The cached catalog, refreshed when it is missing or a day old. A missing
/// one is waited for briefly; a stale one is served while a refresh runs.
/// `None` when there is no catalog at all (offline, no account, or none
/// published), which lists only the installed styles.
async fn catalog(pool: &PgPool, root: &Path) -> Option<Catalog> {
    let path = root.join(".catalog.json");
    let cached = tokio::fs::read(&path).await.ok().and_then(|b| parse_catalog(&b));
    let age = tokio::fs::metadata(&path)
        .await
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.elapsed().ok());
    let stale = age.is_none_or(|a| a > CATALOG_MAX_AGE);
    if !stale {
        return cached;
    }
    if REFRESHING.swap(true, Ordering::SeqCst) {
        return cached;
    }
    let (pool, root_owned) = (pool.clone(), root.to_path_buf());
    let task = tokio::spawn(async move {
        let fresh = refresh_catalog(&pool, &root_owned).await;
        REFRESHING.store(false, Ordering::SeqCst);
        fresh
    });
    if cached.is_some() {
        return cached;
    }
    match tokio::time::timeout(FIRST_CATALOG_WAIT, task).await {
        Ok(Ok(fresh)) => fresh,
        _ => None,
    }
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
    let bytes = resp.bytes().await.ok()?;
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

fn install_states() -> (HashMap<String, Progress>, HashMap<String, String>) {
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
    (progress, failed)
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
        .ok_or_else(|| Error::NotFound(format!("no background style {style} to download")))?;
    tokio::fs::create_dir_all(root.join(".download"))
        .await
        .map_err(|e| Error::Storage(format!("creating the backgrounds folder: {e}")))?;
    let free = crate::storage::lake::free_bytes_at(&root).unwrap_or(u64::MAX);
    if free < entry.bytes.saturating_mul(2) + MIN_FREE_BYTES {
        return Err(Error::Storage("There is not enough free space on your server for this style.".to_string()));
    }
    let received = Arc::new(AtomicU64::new(0));
    {
        let mut installs = INSTALLS.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(installs.get(style), Some(InstallState::Downloading { .. })) {
            return Ok(());
        }
        installs.insert(style.to_string(), InstallState::Downloading { received: received.clone(), total: entry.bytes });
    }
    let pool = pool.clone();
    tokio::spawn(async move {
        let style = entry.style.clone();
        let outcome = download_and_install(&pool, &root, &entry, &received).await;
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

async fn download_and_install(
    pool: &PgPool,
    root: &Path,
    entry: &CatalogStyle,
    received: &AtomicU64,
) -> std::result::Result<(), String> {
    let style = &entry.style;
    let (base, self_hosted) = source_base();
    let key = bearer(pool, self_hosted).await;
    let http = http()?;
    let archive = root.join(".download").join(format!("{style}.tar"));
    let tmp = root.join(".download").join(format!("{style}.tar.tmp"));
    let expected = crate::downloads::Expected { bytes: entry.bytes, sha256: &entry.sha256, what: "background" };
    let url = format!("{base}/{}", entry.archive);
    if let Err(e) = crate::downloads::verified(&http, &url, key.as_deref(), &tmp, &archive, &expected, Some(received)).await
    {
        tracing::warn!("backgrounds: downloading {style}: {e:#}");
        // A file that arrived but is not what the catalog describes is not
        // the connection's fault, and trying again at once will not help.
        let text = format!("{e:#}");
        if text.contains("checksum") || text.contains("expected size") {
            return Err("The download did not match the published style. Try again later.".to_string());
        }
        return Err("The download did not finish. Check your server's connection and try again.".to_string());
    }
    let (root_owned, style_owned, sha) = (root.to_path_buf(), style.clone(), entry.sha256.clone());
    let result = tokio::task::spawn_blocking(move || install_archive(&root_owned, &style_owned, &archive, &sha))
        .await
        .map_err(|_| "Installing the style stopped partway. Try again.".to_string())?;
    if let Err(e) = &result {
        tracing::warn!("backgrounds: installing {style}: {e}");
    }
    result.map_err(|e| format!("The downloaded style could not be installed: {e}."))
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
}

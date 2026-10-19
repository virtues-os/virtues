//! `GET /api/web-bundle/version` + `GET /api/web-bundle/tarball`.
//!
//! The box already serves the web UI from disk (`STATIC_DIR`, wired in
//! `server/mod.rs`). These two endpoints let a *client* fetch that same build
//! instead of only rendering it live, which is what turns the box into the
//! update server for every paired client — no CDN, no cloud, over whatever
//! transport already reaches the box.
//!
//! **Why the box is the only source.** Because it is the only source certain to
//! have the API the UI calls. On 2026-08-05 a TestFlight build shipped UI
//! calling `/api/wiki/day/{date}/heart-rate?tz=…` against a box with no `tz`
//! handler, and the box silently ignored the unknown parameter — wrong
//! midnight, no error, unfindable from the UI. A bundle carrying that call can
//! only be served by a box that also has the handler.
//!
//! **This no longer kills that class by construction**, and the comment here
//! claimed it did until 2026-09-14. The claim rested on a client never being
//! able to get ahead of its box, which stopped being true when the client
//! learned to refuse a downgrade: a phone updates on Apple's cadence and a box
//! when its owner runs `virtues upgrade`, so a shell carrying UI newer than the
//! box's is ordinary, and it now keeps that UI instead of taking the box's
//! older one. See the "Forward only" section in
//! `apps/web/src-tauri/src/web_bundle.rs` for the trade and what would close
//! the class properly (a bundle declaring a minimum BOX, mirroring
//! `minShellVersion`). Nothing on this side enforces it today.
//!
//! **What is NOT here.** Applying a bundle — unpack, atomic flip, rollback —
//! is the client's job, and the `minShellVersion` in the manifest is what stops
//! a client applying a bundle its native shell cannot run. See
//! `agents/record/spa-delivery.md`.

use axum::{
    body::{Body, Bytes},
    extract::State,
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::server::webhook::AppState;

/// Manifest filename inside the static build, stamped at build time by
/// `apps/web/scripts/write-bundle-manifest.mjs`.
const MANIFEST_NAME: &str = ".virtues-bundle.json";

/// Resolve the directory the box serves the web UI from.
///
/// THE one definition. It used to be two — this and an identical literal in
/// `server/mod.rs` — under a comment saying "the two must agree, or the box
/// would hand out a manifest describing a build it is not serving". That is
/// the right requirement and a convention is the wrong way to hold it: the
/// consequence of a drift is a client told the box serves a build it does not.
/// `server/mod.rs` calls this now, so agreement is structural.
///
/// The default is relative to the process's working directory, which is what
/// makes `make dev` work from a checkout. A box sets `STATIC_DIR` outright
/// (`/usr/local/share/virtues/web`; see `cli/upgrade.rs`).
pub fn static_dir() -> PathBuf {
    PathBuf::from(
        std::env::var("STATIC_DIR").unwrap_or_else(|_| "../../apps/web/build".to_string()),
    )
}

/// Outcome of reading the manifest out of a served build. Separated from the
/// handler so it is testable without an `AppState` (and therefore without a
/// database), which is what let these endpoints be verified before any box was
/// running them.
#[derive(Debug, PartialEq)]
pub enum ManifestRead {
    /// The manifest, verbatim.
    Found(serde_json::Value),
    /// No static build here. A real state, not a failure: headless installs
    /// have none, and dev boxes serve the UI from vite instead.
    Absent,
    /// A manifest exists but is not valid JSON — a broken build, worth saying
    /// so loudly rather than reporting "no bundle" and looking normal.
    Malformed,
}

/// Read `.virtues-bundle.json` out of a served build directory.
pub fn read_manifest(dir: &Path) -> ManifestRead {
    let path = dir.join(MANIFEST_NAME);
    let Ok(body) = std::fs::read_to_string(&path) else {
        return ManifestRead::Absent;
    };
    match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(v) => ManifestRead::Found(v),
        Err(e) => {
            tracing::warn!("web-bundle manifest at {} is not valid JSON: {e}", path.display());
            ManifestRead::Malformed
        }
    }
}

/// `GET /api/web-bundle/version` — what build this box is serving.
///
/// Returns the manifest verbatim: `{version, sha, channel, minShellVersion,
/// contentHash}`. A client compares `contentHash`, not `version`: dev and local
/// builds all report `dev`, and two builds of one tag can legitimately differ.
pub async fn version_handler(State(_state): State<AppState>) -> impl IntoResponse {
    match read_manifest(&static_dir()) {
        ManifestRead::Found(v) => (StatusCode::OK, Json(v)).into_response(),
        ManifestRead::Malformed => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "manifest_unreadable" })),
        )
            .into_response(),
        ManifestRead::Absent => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "no_bundle", "static_dir": static_dir().display().to_string() })),
        )
            .into_response(),
    }
}

/// `GET /api/web-bundle/tarball` — the served build as a gzipped tar.
///
/// Built in memory and kept: the build is tens of MB of tar, and every client
/// that sees a new `contentHash` asks for the same bytes. One entry, keyed by
/// that hash, is enough — a box serves one build at a time.
///
/// The manifest rides inside the archive, so an unpacked bundle carries its own
/// identity and a client never has to remember what it downloaded.
pub async fn tarball_handler(State(_state): State<AppState>) -> impl IntoResponse {
    // Blocking IO (resolve + walk + read + deflate) off the async runtime.
    let built = tokio::task::spawn_blocking(|| TARBALL_CACHE.get_or_build(&static_dir())).await;

    match built {
        Ok(Ok(Some(bytes))) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/gzip"),
                (
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=\"virtues-web-bundle.tar.gz\"",
                ),
            ],
            Body::from(bytes),
        )
            .into_response(),
        Ok(Ok(None)) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "no_bundle" })),
        )
            .into_response(),
        Ok(Err(e)) => {
            tracing::warn!("web-bundle tarball failed: {e:#}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "tarball_failed" })),
            )
                .into_response()
        }
        Err(e) => {
            tracing::warn!("web-bundle tarball task panicked: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "tarball_failed" })),
            )
                .into_response()
        }
    }
}

static TARBALL_CACHE: TarballCache = TarballCache::new();

/// The last tarball built, keyed by the build's real path and `contentHash`.
pub struct TarballCache {
    entry: Mutex<Option<CachedTarball>>,
}

struct CachedTarball {
    dir: PathBuf,
    content_hash: String,
    bytes: Bytes,
}

impl TarballCache {
    pub const fn new() -> Self {
        Self { entry: Mutex::new(None) }
    }

    /// The tarball of the build at `dir`, or `None` when there is no build.
    ///
    /// `dir` is resolved ONCE, and the manifest and the walk both read the
    /// resolved path. On a box `STATIC_DIR` runs through the `current` slot
    /// link, and an upgrade can flip that link mid-request; reading the
    /// manifest through one target and the files through another would ship
    /// a tarball whose hash matches neither build.
    ///
    /// A build with no `contentHash` is served but never cached: without the
    /// hash there is no telling a rebuild from the build already cached.
    pub fn get_or_build(&self, dir: &Path) -> anyhow::Result<Option<Bytes>> {
        let dir = match std::fs::canonicalize(dir) {
            Ok(d) if d.is_dir() => d,
            Ok(_) => return Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(anyhow::anyhow!("resolve {}: {e}", dir.display())),
        };

        let hash = content_hash(&dir);
        if let Some(hash) = &hash {
            let entry = self.entry.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(c) = entry.as_ref().filter(|c| c.dir == dir && &c.content_hash == hash) {
                return Ok(Some(c.bytes.clone()));
            }
        }

        let bytes = Bytes::from(build_tarball_bytes(&dir)?);

        // Cache only if the build did not change under the walk: the hash
        // read before must still be the hash after, or these bytes belong to
        // neither.
        if let Some(hash) = hash.filter(|h| content_hash(&dir).as_ref() == Some(h)) {
            *self.entry.lock().unwrap_or_else(|p| p.into_inner()) = Some(CachedTarball {
                dir,
                content_hash: hash,
                bytes: bytes.clone(),
            });
        }
        Ok(Some(bytes))
    }
}

fn content_hash(dir: &Path) -> Option<String> {
    match read_manifest(dir) {
        ManifestRead::Found(v) => v.get("contentHash")?.as_str().map(str::to_owned),
        ManifestRead::Absent | ManifestRead::Malformed => None,
    }
}

/// Tar + gzip `dir`, with paths relative to it so a client unpacks into its own
/// bundle directory without a leading component to strip.
///
/// The `.gz` siblings the web build writes beside each asset (for the box's
/// own precompressed serving) are skipped: a phone unpacks and serves the
/// originals from disk, so shipping both would double every OTA for nothing.
/// Symlinks are skipped too, which is why `write-bundle-manifest.mjs` refuses
/// a build that contains one: its hash would cover a file no client receives.
pub fn build_tarball_bytes(dir: &Path) -> anyhow::Result<Vec<u8>> {
    use flate2::{write::GzEncoder, Compression};

    let encoder = GzEncoder::new(Vec::new(), Compression::default());
    let mut builder = tar::Builder::new(encoder);
    builder.follow_symlinks(false);
    append_tree(&mut builder, dir, Path::new(""))?;
    let encoder = builder.into_inner()?;
    Ok(encoder.finish()?)
}

fn append_tree<W: std::io::Write>(
    builder: &mut tar::Builder<W>,
    root: &Path,
    rel: &Path,
) -> anyhow::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(root.join(rel))?.collect::<Result<_, _>>()?;
    // Deterministic order, so two tarballs of one build are byte-identical.
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name();
        let rel_path = rel.join(&name);
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            builder.append_dir(&rel_path, entry.path())?;
            append_tree(builder, root, &rel_path)?;
        } else if rel_path.extension().is_some_and(|ext| ext == "gz") {
            continue;
        } else {
            builder.append_path_with_name(entry.path(), &rel_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp() -> PathBuf {
        // A counter as well as the clock: macOS clocks tick in microseconds,
        // and one test can ask for two directories inside one tick.
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let d = std::env::temp_dir().join(format!(
            "virtues-webbundle-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn absent_manifest_is_a_state_not_an_error() {
        // A headless box, or a dev box serving the UI from vite. The client
        // must read this as "nothing to update from", never as a retryable
        // failure — so it is distinct from Malformed.
        assert_eq!(read_manifest(&tmp()), ManifestRead::Absent);
    }

    #[test]
    fn malformed_manifest_is_distinguished_from_absent() {
        let d = tmp();
        fs::write(d.join(MANIFEST_NAME), "{not json").unwrap();
        assert_eq!(read_manifest(&d), ManifestRead::Malformed);
    }

    #[test]
    fn reads_a_real_manifest_verbatim() {
        let d = tmp();
        fs::write(
            d.join(MANIFEST_NAME),
            r#"{"version":"0.3.0","sha":"abc1234","channel":"stable",
                "minShellVersion":1,"contentHash":"f9f5785de8567c67"}"#,
        )
        .unwrap();
        let ManifestRead::Found(v) = read_manifest(&d) else {
            panic!("expected Found");
        };
        assert_eq!(v["contentHash"], "f9f5785de8567c67");
        assert_eq!(v["minShellVersion"], 1);
    }

    #[test]
    fn tarball_round_trips_the_served_build() {
        let src = tmp();
        fs::write(src.join("index.html"), "<html>box</html>").unwrap();
        fs::create_dir_all(src.join("_app")).unwrap();
        fs::write(src.join("_app/chunk.js"), "console.log(1)").unwrap();
        // The box's precompressed sibling — served by the box, never shipped.
        fs::write(src.join("_app/chunk.js.gz"), "not really gzip").unwrap();
        fs::write(src.join(MANIFEST_NAME), r#"{"contentHash":"deadbeef"}"#).unwrap();

        let gz = build_tarball_bytes(&src).expect("tarball");

        let dest = tmp();
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(&gz[..]));
        archive.unpack(&dest).unwrap();

        assert_eq!(fs::read_to_string(dest.join("index.html")).unwrap(), "<html>box</html>");
        assert_eq!(fs::read_to_string(dest.join("_app/chunk.js")).unwrap(), "console.log(1)");
        assert!(
            !dest.join("_app/chunk.js.gz").exists(),
            "precompressed siblings must not ride in the OTA tarball"
        );
        // The manifest must ride inside, so an unpacked bundle carries its own
        // identity and a client never has to remember what it downloaded.
        assert!(dest.join(MANIFEST_NAME).is_file(), "manifest travels in the archive");
    }

    #[test]
    fn tarball_is_cached_by_content_hash() {
        let src = tmp();
        fs::write(src.join("index.html"), "<html>one</html>").unwrap();
        fs::write(src.join(MANIFEST_NAME), r#"{"contentHash":"aaaa"}"#).unwrap();
        let cache = TarballCache::new();

        let first = cache.get_or_build(&src).unwrap().expect("a build");
        let again = cache.get_or_build(&src).unwrap().expect("a build");
        assert_eq!(first.as_ptr(), again.as_ptr(), "same hash serves the cached bytes");

        fs::write(src.join("index.html"), "<html>two</html>").unwrap();
        fs::write(src.join(MANIFEST_NAME), r#"{"contentHash":"bbbb"}"#).unwrap();
        let rebuilt = cache.get_or_build(&src).unwrap().expect("a build");
        assert_ne!(first, rebuilt, "a new hash rebuilds");
    }

    #[test]
    fn tarball_without_a_hash_is_rebuilt_every_time() {
        let src = tmp();
        fs::write(src.join("index.html"), "<html>one</html>").unwrap();
        let cache = TarballCache::new();
        let first = cache.get_or_build(&src).unwrap().expect("a build");
        let again = cache.get_or_build(&src).unwrap().expect("a build");
        assert_ne!(first.as_ptr(), again.as_ptr());
    }

    #[test]
    fn missing_build_is_none_not_an_error() {
        let cache = TarballCache::new();
        assert!(cache.get_or_build(&tmp().join("nope")).unwrap().is_none());
    }

    /// The walk reads the directory the link pointed at when the request
    /// began, not wherever it points by the time each file is read.
    #[cfg(unix)]
    #[test]
    fn tarball_reads_through_the_link_once() {
        let root = tmp();
        let a = root.join("a");
        fs::create_dir_all(&a).unwrap();
        fs::write(a.join("index.html"), "a").unwrap();
        fs::write(a.join(MANIFEST_NAME), r#"{"contentHash":"a"}"#).unwrap();
        let link = root.join("current");
        std::os::unix::fs::symlink(&a, &link).unwrap();

        let gz = TarballCache::new().get_or_build(&link).unwrap().expect("a build");
        let dest = tmp();
        tar::Archive::new(flate2::read::GzDecoder::new(&gz[..])).unpack(&dest).unwrap();
        assert_eq!(fs::read_to_string(dest.join("index.html")).unwrap(), "a");
    }

    /// The box half of the real-pipeline CI test (`ci.yml`, "OTA pipeline").
    /// With `OTA_BUILD_DIR` set to a real `pnpm build`, writes the tarball the
    /// box would serve to `OTA_TARBALL_OUT`, for the shell's
    /// `real_box_bundle_applies` to unpack. Without it, there is nothing to do.
    #[test]
    fn real_build_tarball_for_the_shell() {
        let Ok(build_dir) = std::env::var("OTA_BUILD_DIR") else {
            return;
        };
        let out = std::env::var("OTA_TARBALL_OUT").expect("Set OTA_TARBALL_OUT along with OTA_BUILD_DIR");
        let bytes = TarballCache::new()
            .get_or_build(Path::new(&build_dir))
            .expect("tarball")
            .expect("OTA_BUILD_DIR holds no build");
        assert!(
            content_hash(&fs::canonicalize(&build_dir).unwrap()).is_some(),
            "the build carries no manifest contentHash"
        );
        fs::write(&out, &bytes).unwrap();
        eprintln!("wrote {} bytes to {out}", bytes.len());
    }
}

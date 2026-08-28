//! `GET /api/web-bundle/version` + `GET /api/web-bundle/tarball`.
//!
//! The box already serves the web UI from disk (`STATIC_DIR`, wired in
//! `server/mod.rs`). These two endpoints let a *client* fetch that same build
//! instead of only rendering it live, which is what turns the box into the
//! update server for every paired client — no CDN, no cloud, over whatever
//! transport already reaches the box.
//!
//! **Why the box is the only source.** A client that can only ever run a bundle
//! the box handed it cannot get ahead of the box. That kills a whole class of
//! bug by construction: on 2026-08-05 a TestFlight build shipped UI calling
//! `/api/wiki/day/{date}/heart-rate?tz=…` against a box with no `tz` handler,
//! and the box silently ignored the unknown parameter — wrong midnight, no
//! error, unfindable from the UI. The bundle carrying that call can only ship
//! from a box that also has the handler.
//!
//! **What is NOT here.** Applying a bundle — unpack, atomic flip, rollback —
//! is the client's job, and the `minShellVersion` in the manifest is what stops
//! a client applying a bundle its native shell cannot run. See
//! `agents/plan/spa-delivery-plan.md`.

use axum::{
    body::Body,
    extract::State,
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use serde_json::json;
use std::path::{Path, PathBuf};

use crate::server::webhook::AppState;

/// Manifest filename inside the static build, stamped at build time by
/// `apps/web/scripts/write-bundle-manifest.mjs`.
const MANIFEST_NAME: &str = ".virtues-bundle.json";

/// Resolve the directory the box serves the web UI from. Mirrors the same
/// `STATIC_DIR` default as `server/mod.rs` — the two must agree, or the box
/// would hand out a manifest describing a build it is not serving.
fn static_dir() -> PathBuf {
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
/// Built in memory. The web build is a few MB, so streaming it from disk buys
/// nothing over the simplicity of handing back one buffer; revisit if the
/// bundle ever grows into the tens of MB.
///
/// The manifest rides inside the archive, so an unpacked bundle carries its own
/// identity and a client never has to remember what it downloaded.
pub async fn tarball_handler(State(_state): State<AppState>) -> impl IntoResponse {
    let dir = static_dir();
    if !dir.is_dir() {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "no_bundle" })),
        )
            .into_response();
    }

    // Blocking IO (walk + read + deflate) off the async runtime.
    let built = tokio::task::spawn_blocking(move || build_tarball(&dir)).await;

    match built {
        Ok(Ok(bytes)) => (
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

/// Tar + gzip `dir`, with paths relative to it so a client unpacks into its own
/// bundle directory without a leading component to strip.
fn build_tarball(dir: &Path) -> anyhow::Result<Vec<u8>> {
    use flate2::{write::GzEncoder, Compression};

    let encoder = GzEncoder::new(Vec::new(), Compression::default());
    let mut builder = tar::Builder::new(encoder);
    // Text assets compress well and the archive is transient; favor speed of
    // the walk over squeezing the last few percent.
    builder.follow_symlinks(false);
    builder.append_dir_all(".", dir)?;
    let encoder = builder.into_inner()?;
    Ok(encoder.finish()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "virtues-webbundle-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
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
        fs::write(src.join(MANIFEST_NAME), r#"{"contentHash":"deadbeef"}"#).unwrap();

        let gz = build_tarball(&src).expect("tarball");

        let dest = tmp();
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(&gz[..]));
        archive.unpack(&dest).unwrap();

        assert_eq!(fs::read_to_string(dest.join("index.html")).unwrap(), "<html>box</html>");
        assert_eq!(fs::read_to_string(dest.join("_app/chunk.js")).unwrap(), "console.log(1)");
        // The manifest must ride inside, so an unpacked bundle carries its own
        // identity and a client never has to remember what it downloaded.
        assert!(dest.join(MANIFEST_NAME).is_file(), "manifest travels in the archive");
    }
}

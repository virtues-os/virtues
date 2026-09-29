//! Map files for the boxes (agents/record/map-tiles.md).
//!
//! A monthly job cuts Protomaps extracts into `<maps_dir>/<build>/` and
//! writes `index.json` there, then names the build in `<maps_dir>/latest.json`.
//! Every file is a fixed, pre-cut square identical for every box that takes
//! it, so a download never says where in the square anyone lives.
//!
//! Both routes need a subscriber's bearer (a lapsed one gets the usual 402;
//! what a box already holds keeps working). **They log nothing**: they are
//! merged outside `TraceLayer` in `main.rs`, and neither handler writes the
//! path or the account anywhere. Keep it that way: which squares a box takes
//! is location data.

use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use tower_http::services::ServeFile;

use crate::bearer_auth::BearerAuth;
use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/maps/index", get(index))
        .route("/v1/maps/:build/:file", get(file))
}

fn err(status: StatusCode, code: &str, message: &str) -> Response {
    (status, Json(json!({ "error": code, "message": message }))).into_response()
}

#[derive(Deserialize)]
struct Latest {
    build: String,
}

/// A build is the Protomaps build date it was cut from: `YYYYMMDD`.
fn is_build(s: &str) -> bool {
    s.len() == 8 && s.bytes().all(|b| b.is_ascii_digit())
}

/// The only names a build holds: the world file, a tier square, the assets
/// bundle, and the index itself.
fn is_map_file(s: &str) -> bool {
    if matches!(s, "world.pmtiles" | "assets.tar" | "index.json") {
        return true;
    }
    let Some(stem) = s.strip_suffix(".pmtiles") else {
        return false;
    };
    let mut parts = stem.split('-');
    let tier_zoom = match (parts.next(), parts.next()) {
        (Some("home"), Some("z7")) => 7,
        (Some("visited"), Some("z5")) => 5,
        _ => return false,
    };
    let n = 1u32 << tier_zoom;
    let coord = |p: Option<&str>| p.and_then(|v| v.parse::<u32>().ok()).filter(|v| *v < n);
    coord(parts.next()).is_some() && coord(parts.next()).is_some() && parts.next().is_none()
}

async fn current_build(dir: &FsPath) -> Option<String> {
    let raw = tokio::fs::read(dir.join("latest.json")).await.ok()?;
    let latest: Latest = serde_json::from_slice(&raw).ok()?;
    is_build(&latest.build).then_some(latest.build)
}

/// GET /v1/maps/index — the current build's index: every file with its tier,
/// square, size and sha256.
async fn index(State(state): State<Arc<AppState>>, BearerAuth(_ent): BearerAuth) -> Response {
    let dir = &state.config.maps_dir;
    let Some(build) = current_build(dir).await else {
        return err(StatusCode::SERVICE_UNAVAILABLE, "maps_not_available", "No map build is published yet");
    };
    match tokio::fs::read(dir.join(&build).join("index.json")).await {
        Ok(bytes) => (
            [(header::CONTENT_TYPE, "application/json"), (header::CACHE_CONTROL, "no-store")],
            bytes,
        )
            .into_response(),
        Err(_) => err(StatusCode::SERVICE_UNAVAILABLE, "maps_not_available", "The map build has no index"),
    }
}

/// GET /v1/maps/:build/:file — one file, with `Range` so an interrupted
/// download resumes where it stopped.
async fn file(
    State(state): State<Arc<AppState>>,
    BearerAuth(_ent): BearerAuth,
    Path((build, name)): Path<(String, String)>,
    req: Request,
) -> Response {
    if !is_build(&build) || !is_map_file(&name) {
        return err(StatusCode::NOT_FOUND, "not_found", "No such map file");
    }
    let path: PathBuf = state.config.maps_dir.join(&build).join(&name);
    if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
        return err(StatusCode::NOT_FOUND, "not_found", "No such map file");
    }
    match ServeFile::new(path).try_call(req).await {
        Ok(res) => res.map(Body::new),
        Err(_) => err(StatusCode::INTERNAL_SERVER_ERROR, "read_failed", "Could not read the map file"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_map_files_are_served() {
        for ok in ["world.pmtiles", "assets.tar", "index.json", "home-z7-29-52.pmtiles", "visited-z5-7-13.pmtiles"] {
            assert!(is_map_file(ok), "{ok}");
        }
        for bad in [
            "../latest.json",
            "home-z7-29-52.pmtiles.tmp",
            "home-z5-7-13.pmtiles",
            "visited-z5-32-0.pmtiles",
            "home-z7-29.pmtiles",
            "home-z7-29-52-1.pmtiles",
            "planet.pmtiles",
            ".env",
        ] {
            assert!(!is_map_file(bad), "{bad}");
        }
    }

    #[test]
    fn builds_are_dates() {
        assert!(is_build("20260925"));
        assert!(!is_build("latest"));
        assert!(!is_build("../2026"));
        assert!(!is_build("2026092"));
    }
}

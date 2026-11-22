//! Home background styles for the boxes (agents/plan/backgrounds-plan.md).
//!
//! `tools/backgrounds/package.py` writes a `catalog.json` and one archive per
//! style, `<style>-<hash>.tar`, and they are uploaded to the maps bucket
//! under `backgrounds/`. A box reads the catalog to show the styles it can
//! download, and fetches an archive once when its owner picks that style.
//! Every file is the same for every box.
//!
//! Both routes need a subscriber's bearer, as the map routes do. They log
//! nothing: they are merged outside `TraceLayer` in `main.rs`, and neither
//! handler writes the file or the account anywhere. Reads go through the
//! signed bucket reads in `routes/maps.rs`, so a read that reaches storage
//! carries this server's address and the file's name, and no box's address
//! or key. Without `Config::maps_bucket` there are no styles to offer.

use std::sync::Arc;

use axum::{
    extract::{Path, Request, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde_json::json;

use super::maps::{bucket_get, forward, small_body};
use crate::bearer_auth::BearerAuth;
use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/backgrounds/catalog.json", get(catalog))
        .route("/v1/backgrounds/:file", get(file))
}

/// The folder in the bucket that holds the catalog and the archives.
const PREFIX: &str = "backgrounds";

fn err(status: StatusCode, code: &str, message: &str) -> Response {
    (status, Json(json!({ "error": code, "message": message }))).into_response()
}

fn not_available() -> Response {
    err(StatusCode::SERVICE_UNAVAILABLE, "backgrounds_not_available", "No background styles are published yet")
}

fn storage_unavailable() -> Response {
    err(
        StatusCode::BAD_GATEWAY,
        "backgrounds_storage_unavailable",
        "Could not read the background styles. Try again later.",
    )
}

/// An archive's name: `<style>-<hash>.tar`, lowercase letters, digits and
/// hyphens only, as package.py writes it.
fn is_archive(name: &str) -> bool {
    name.strip_suffix(".tar").is_some_and(|stem| {
        !stem.is_empty()
            && stem.len() <= 96
            && stem.contains('-')
            && !stem.starts_with('-')
            && stem.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    })
}

/// GET /v1/backgrounds/catalog.json — every published style with its archive,
/// size, sha256 and a small preview.
async fn catalog(State(state): State<Arc<AppState>>, BearerAuth(_ent): BearerAuth) -> Response {
    let Some(bucket) = &state.config.maps_bucket else {
        return not_available();
    };
    let Ok(upstream) = bucket_get(&state.stream_client, bucket, &format!("{PREFIX}/catalog.json"), None).await else {
        return storage_unavailable();
    };
    match upstream.status() {
        StatusCode::OK => {}
        StatusCode::NOT_FOUND => return not_available(),
        _ => return storage_unavailable(),
    }
    match small_body(upstream).await {
        Some(bytes) => (
            [(header::CONTENT_TYPE, "application/json"), (header::CACHE_CONTROL, "no-store")],
            bytes,
        )
            .into_response(),
        None => storage_unavailable(),
    }
}

/// GET /v1/backgrounds/:file — one style's archive, with `Range` so an
/// interrupted download resumes where it stopped.
async fn file(
    State(state): State<Arc<AppState>>,
    BearerAuth(_ent): BearerAuth,
    Path(name): Path<String>,
    req: Request,
) -> Response {
    if !is_archive(&name) {
        return err(StatusCode::NOT_FOUND, "not_found", "No such background style");
    }
    let Some(bucket) = &state.config.maps_bucket else {
        return not_available();
    };
    let range = req.headers().get(header::RANGE);
    let Ok(upstream) = bucket_get(&state.stream_client, bucket, &format!("{PREFIX}/{name}"), range).await else {
        return storage_unavailable();
    };
    match forward(upstream, "application/x-tar") {
        Ok(res) => res,
        Err(StatusCode::NOT_FOUND) => err(StatusCode::NOT_FOUND, "not_found", "No such background style"),
        Err(_) => storage_unavailable(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_archives_are_served() {
        for ok in ["watercolors-1a2b3c4d5e6f.tar", "oil-paintings-0f9e.tar"] {
            assert!(is_archive(ok), "{ok}");
        }
        for bad in ["catalog.json", "../catalog.json", "watercolors.tar.tmp", "-x.tar", "Water-1.tar", "a b-1.tar", ".tar", "x.tar"] {
            assert!(!is_archive(bad), "{bad}");
        }
    }
}

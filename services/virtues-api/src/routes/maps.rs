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
//!
//! The files are read from one of two places with the same layout. When
//! `Config::maps_bucket` is set they come from object storage: the API signs
//! each read and streams the bytes to the box, so a read that reaches storage
//! carries this server's address and the square's name, and no box's address
//! or key.
//! A read that fails is answered and not logged, and a `reqwest::Error` is
//! never formatted, because it carries the URL. Otherwise they come from
//! `Config::maps_dir`.
//!
//! TODO(2026-11-19): remove the directory source, `maps_dir` and
//! `VIRTUES_MAPS_DIR` once the services have moved
//! (agents/plan/cloud-vps-move-plan.md, slice 5).

use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use futures::TryStreamExt;
use serde::Deserialize;
use serde_json::json;
use tower_http::services::ServeFile;

use virtues_helpers::crypto::sigv4::{sign_s3_get, S3Get};

use crate::bearer_auth::BearerAuth;
use crate::config::MapsBucket;
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
    // Digits only: `parse` alone would take `+5`, which is not a file name.
    let coord = |p: Option<&str>| {
        p.filter(|v| v.bytes().all(|b| b.is_ascii_digit())).and_then(|v| v.parse::<u32>().ok()).filter(|v| *v < n)
    };
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
    if let Some(bucket) = &state.config.maps_bucket {
        return bucket_index(&state.stream_client, bucket).await;
    }
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
    if let Some(bucket) = &state.config.maps_bucket {
        return bucket_file(&state.stream_client, bucket, &build, &name, req.headers().get(header::RANGE)).await;
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

/// How long storage has to start answering a read, and to send a whole
/// `latest.json` or `index.json`. A file's body has no total: it streams for
/// as long as bytes keep coming (`stream_client`'s read timeout).
const STORAGE_ANSWER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

fn storage_unavailable() -> Response {
    err(StatusCode::BAD_GATEWAY, "maps_storage_unavailable", "Could not read the map files. Try again later.")
}

/// One signed read of `<endpoint>/<bucket>/<key>`. `key` is `latest.json` or
/// a `<build>/<file>` both parts of which passed `is_build` and
/// `is_map_file`, so it needs no URI encoding and cannot name anything else.
/// `Err` is a request that never got an answer.
async fn bucket_get(
    http: &reqwest::Client,
    bucket: &MapsBucket,
    key: &str,
    range: Option<&HeaderValue>,
) -> Result<reqwest::Response, ()> {
    let url = reqwest::Url::parse(&format!("{}/{}/{}", bucket.endpoint, bucket.bucket, key)).map_err(|_| ())?;
    let host = match (url.host_str(), url.port()) {
        (Some(host), Some(port)) => format!("{host}:{port}"),
        (Some(host), None) => host.to_string(),
        (None, _) => return Err(()),
    };
    let range = range.and_then(|v| v.to_str().ok());
    let signed = sign_s3_get(&S3Get {
        access_key: &bucket.access_key,
        secret_key: &bucket.secret_key,
        region: &bucket.region,
        host: &host,
        path: url.path(),
        range,
        now: chrono::Utc::now(),
    });
    let mut req = http.get(url);
    if let Some(range) = range {
        req = req.header(header::RANGE, range);
    }
    for (name, value) in signed {
        req = req.header(name, value);
    }
    match tokio::time::timeout(STORAGE_ANSWER_TIMEOUT, req.send()).await {
        Ok(Ok(res)) => Ok(res),
        _ => Err(()),
    }
}

/// A small object's whole body, or `None` when it does not arrive in time.
async fn small_body(res: reqwest::Response) -> Option<bytes::Bytes> {
    tokio::time::timeout(STORAGE_ANSWER_TIMEOUT, res.bytes()).await.ok()?.ok()
}

/// The current build's index, from storage. No `latest.json` or no index is
/// "no build published yet" (503, which a box takes quietly); any other
/// failure is the storage's (502).
async fn bucket_index(http: &reqwest::Client, bucket: &MapsBucket) -> Response {
    let not_published = || err(StatusCode::SERVICE_UNAVAILABLE, "maps_not_available", "No map build is published yet");
    let Ok(latest) = bucket_get(http, bucket, "latest.json", None).await else {
        return storage_unavailable();
    };
    match latest.status() {
        StatusCode::OK => {}
        StatusCode::NOT_FOUND => return not_published(),
        _ => return storage_unavailable(),
    }
    let Some(raw) = small_body(latest).await else {
        return storage_unavailable();
    };
    let Some(build) = serde_json::from_slice::<Latest>(&raw).ok().map(|l| l.build).filter(|b| is_build(b)) else {
        return not_published();
    };
    let Ok(index) = bucket_get(http, bucket, &format!("{build}/index.json"), None).await else {
        return storage_unavailable();
    };
    match index.status() {
        StatusCode::OK => {}
        StatusCode::NOT_FOUND => {
            return err(StatusCode::SERVICE_UNAVAILABLE, "maps_not_available", "The map build has no index")
        }
        _ => return storage_unavailable(),
    }
    match small_body(index).await {
        Some(bytes) => (
            [(header::CONTENT_TYPE, "application/json"), (header::CACHE_CONTROL, "no-store")],
            bytes,
        )
            .into_response(),
        None => storage_unavailable(),
    }
}

/// One file from storage, streamed. The box's `Range` goes to storage as it
/// came, and storage's `206` and `Content-Range` come back as they are, which
/// is what a resumed download checks.
async fn bucket_file(
    http: &reqwest::Client,
    bucket: &MapsBucket,
    build: &str,
    name: &str,
    range: Option<&HeaderValue>,
) -> Response {
    let Ok(upstream) = bucket_get(http, bucket, &format!("{build}/{name}"), range).await else {
        return storage_unavailable();
    };
    let status = upstream.status();
    let mut headers = HeaderMap::new();
    for name in [header::CONTENT_LENGTH, header::CONTENT_RANGE, header::ACCEPT_RANGES, header::ETAG, header::LAST_MODIFIED] {
        if let Some(value) = upstream.headers().get(&name) {
            headers.insert(name, value.clone());
        }
    }
    match status {
        StatusCode::OK | StatusCode::PARTIAL_CONTENT => {
            headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type(name)));
            // A stream error is stripped of its URL before anything can print it.
            let bytes = upstream.bytes_stream().map_err(reqwest::Error::without_url);
            (status, headers, Body::from_stream(bytes)).into_response()
        }
        StatusCode::NOT_FOUND => err(StatusCode::NOT_FOUND, "not_found", "No such map file"),
        StatusCode::RANGE_NOT_SATISFIABLE => {
            headers.remove(header::CONTENT_LENGTH);
            (status, headers).into_response()
        }
        _ => storage_unavailable(),
    }
}

fn content_type(name: &str) -> &'static str {
    match name.rsplit('.').next() {
        Some("json") => "application/json",
        Some("tar") => "application/x-tar",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_map_files_are_served() {
        for ok in ["world.pmtiles", "assets.tar", "index.json", "home-z7-100-80.pmtiles", "visited-z5-25-20.pmtiles"] {
            assert!(is_map_file(ok), "{ok}");
        }
        for bad in [
            "../latest.json",
            "home-z7-100-80.pmtiles.tmp",
            "home-z5-25-20.pmtiles",
            "visited-z5-32-0.pmtiles",
            "home-z7-100.pmtiles",
            "home-z7-100-80-1.pmtiles",
            "home-z7-+5-80.pmtiles",
            "home-z7-5-.pmtiles",
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

    const BUILD: &str = "20260925";
    const WORLD: &[u8] = b"0123456789";

    /// A stand-in for the storage service. It serves a directory holding one
    /// build under `/maps-test/`. Like the real one, it works out the
    /// signature itself from the `Host`, path, `Range` and date it received
    /// and refuses a read whose `Authorization` differs.
    async fn storage(with_latest: bool) -> (reqwest::Client, MapsBucket) {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let dir = std::env::temp_dir().join(format!("virtues-maps-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join(BUILD)).unwrap();
        std::fs::write(dir.join(BUILD).join("world.pmtiles"), WORLD).unwrap();
        std::fs::write(dir.join(BUILD).join("index.json"), br#"{"build":"20260925","files":[]}"#).unwrap();
        if with_latest {
            std::fs::write(dir.join("latest.json"), br#"{"build": "20260925"}"#).unwrap();
        }
        let app = Router::new().route(
            "/maps-test/*key",
            get(move |Path(key): Path<String>, req: Request| {
                let file = dir.join(key);
                async move {
                    // Worked out before the await, so no borrow of `req` crosses it.
                    let signed = {
                        let text = |name: &str| req.headers().get(name).and_then(|v| v.to_str().ok());
                        let now = text("x-amz-date")
                            .and_then(|d| chrono::NaiveDateTime::parse_from_str(d, "%Y%m%dT%H%M%SZ").ok())
                            .map(|d| d.and_utc());
                        now.is_some_and(|now| {
                            let expected = sign_s3_get(&S3Get {
                                access_key: "test-access",
                                secret_key: "test-secret",
                                region: "test-1",
                                host: text("host").unwrap_or(""),
                                path: req.uri().path(),
                                range: text("range"),
                                now,
                            });
                            text("authorization") == Some(expected[0].1.as_str())
                        })
                    };
                    if !signed {
                        return StatusCode::FORBIDDEN.into_response();
                    }
                    match ServeFile::new(file).try_call(req).await {
                        Ok(res) => res.map(Body::new),
                        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let bucket = MapsBucket {
            endpoint,
            bucket: "maps-test".into(),
            region: "test-1".into(),
            access_key: "test-access".into(),
            secret_key: "test-secret".into(),
        };
        (reqwest::Client::new(), bucket)
    }

    async fn body(res: Response) -> Vec<u8> {
        axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap().to_vec()
    }

    #[tokio::test]
    async fn a_whole_file_comes_from_storage() {
        let (http, bucket) = storage(true).await;
        let res = bucket_file(&http, &bucket, BUILD, "world.pmtiles", None).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers()[header::CONTENT_LENGTH], "10");
        assert_eq!(res.headers()[header::CONTENT_TYPE], "application/octet-stream");
        assert_eq!(body(res).await, WORLD);
    }

    #[tokio::test]
    async fn a_resumed_download_gets_the_range_it_asked_for() {
        let (http, bucket) = storage(true).await;
        let range = HeaderValue::from_static("bytes=4-");
        let res = bucket_file(&http, &bucket, BUILD, "world.pmtiles", Some(&range)).await;
        assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(res.headers()[header::CONTENT_RANGE], "bytes 4-9/10");
        assert_eq!(body(res).await, b"456789");
    }

    #[tokio::test]
    async fn a_range_past_the_end_is_answered_as_one() {
        let (http, bucket) = storage(true).await;
        let range = HeaderValue::from_static("bytes=50-");
        let res = bucket_file(&http, &bucket, BUILD, "world.pmtiles", Some(&range)).await;
        assert_eq!(res.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    }

    #[tokio::test]
    async fn a_file_storage_does_not_have_is_a_404() {
        let (http, bucket) = storage(true).await;
        let res = bucket_file(&http, &bucket, BUILD, "home-z7-1-1.pmtiles", None).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn the_index_is_the_one_latest_json_names() {
        let (http, bucket) = storage(true).await;
        let res = bucket_index(&http, &bucket).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(body(res).await, br#"{"build":"20260925","files":[]}"#);
    }

    #[tokio::test]
    async fn storage_without_latest_json_has_no_build_yet() {
        let (http, bucket) = storage(false).await;
        assert_eq!(bucket_index(&http, &bucket).await.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn a_read_storage_refuses_is_a_502() {
        let (http, mut bucket) = storage(true).await;
        bucket.secret_key = "another-secret".into();
        assert_eq!(bucket_index(&http, &bucket).await.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(bucket_file(&http, &bucket, BUILD, "world.pmtiles", None).await.status(), StatusCode::BAD_GATEWAY);
    }

    #[tokio::test]
    async fn storage_that_does_not_answer_is_a_502() {
        let (http, mut bucket) = storage(true).await;
        // A port nothing listens on: bind one, read its number, and close it.
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
        bucket.endpoint = format!("http://{closed}");
        assert_eq!(bucket_index(&http, &bucket).await.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(bucket_file(&http, &bucket, BUILD, "world.pmtiles", None).await.status(), StatusCode::BAD_GATEWAY);
    }
}

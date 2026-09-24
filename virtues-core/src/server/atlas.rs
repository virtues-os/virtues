//! The Atlas — the box's own basemap. See agents/record/map-atlas-plan.md.
//!
//! The browser only ever talks to the box. Every piece of the map (the style,
//! the vector tiles, the label glyphs, the icon sprite) is fetched from
//! OpenFreeMap the first time it is needed, stored in the lake under
//! `map_atlas/`, and served from there forever after. So no third party learns
//! which places the person looks at, and areas already seen work offline.
//!
//! The browser draws the tiles itself (MapLibre GL inside the Leaflet maps),
//! because OpenFreeMap publishes vector tiles only. The style it draws with is
//! OpenFreeMap's, rewritten here so every URL in it points back at this box.
//!
//! OpenFreeMap needs no key and allows commercial use; its terms forbid only
//! automated bulk collection. Fetching lazily, per tile a person actually
//! looked at, is that line. Never add a prefetch-a-region feature against it.
//! If a box ever needs a whole region offline, serve a Protomaps extract from
//! the lake instead.

use std::time::{Duration, Instant};

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::{json, Value};

use super::webhook::AppState;
use crate::error::Error;

const UPSTREAM: &str = "https://tiles.openfreemap.org";

/// Our style name → OpenFreeMap's. The only place the upstream styles are named.
const STYLES: &[(&str, &str)] = &[("light", "positron"), ("dark", "dark")];

/// OpenFreeMap's required credit (the "OpenFreeMap" part is optional but asked
/// for). Carried on the style's source, where the Leaflet binding reads it into
/// the attribution control, so it cannot be dropped by a component.
const ATTRIBUTION: &str = "<a href=\"https://openfreemap.org\" target=\"_blank\">OpenFreeMap</a> \
     <a href=\"https://www.openmaptiles.org/\" target=\"_blank\">&copy; OpenMapTiles</a> \
     Data from <a href=\"https://www.openstreetmap.org/copyright\" target=\"_blank\">OpenStreetMap</a>";

/// OpenMapTiles stops at z14; MapLibre overzooms past it from the z14 tile.
const MAX_ZOOM: u32 = 14;

/// Where the Atlas lives in the lake (so `virtues backup` carries it).
const CACHE_ROOT: &str = "map_atlas";

/// The raster cache from the CARTO era. CARTO began requiring an API key on
/// 2026-09-23 and answered every keyless request with HTTP 200 and a grey
/// "API KEY REQUIRED" image, which the old handler cached as if it were a real
/// tile. Nothing reads this directory any more; it is removed once per process.
const LEGACY_RASTER_CACHE: &str = "map_tiles";

const VECTOR_TILE_TYPES: &[&str] = &["application/vnd.mapbox-vector-tile", "application/x-protobuf"];

fn client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        crate::http_client::base_builder()
            .timeout(Duration::from_secs(15))
            .user_agent("virtues-box atlas (self-hosted personal map cache)")
            .build()
            .expect("atlas http client")
    })
}

/// Fetch one upstream object, refusing anything whose content type is not one
/// we asked for.
///
/// A provider that changes terms tends to answer with a 200 and a placeholder
/// (an HTML page, a watermark image) rather than an error, and a cache that
/// trusts the status stores the placeholder forever. The type check catches
/// every placeholder that is not itself a valid map object; it is the one
/// guard that does not depend on knowing what the next placeholder looks like.
async fn fetch_checked(url: &str, allowed: &[&str]) -> Result<Vec<u8>, Error> {
    let resp = client()
        .get(url)
        .send()
        .await
        .map_err(|e| Error::Other(format!("atlas GET {url}: {e}")))?
        .error_for_status()
        .map_err(|e| Error::Other(format!("atlas status {url}: {e}")))?;
    let content_type = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !allowed.iter().any(|t| content_type.starts_with(t)) {
        return Err(Error::Other(format!(
            "atlas {url}: upstream sent {content_type:?}, expected one of {allowed:?}"
        )));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| Error::Other(format!("atlas body {url}: {e}")))?;
    Ok(bytes.to_vec())
}

fn respond(bytes: Vec<u8>, content_type: &'static str, cache_control: &'static str) -> Response {
    (
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, cache_control),
        ],
        bytes,
    )
        .into_response()
}

fn unavailable(what: &str, e: Error) -> Response {
    tracing::warn!("atlas: {what} unavailable: {e}");
    // Offline, or upstream refused. MapLibre draws the gap and carries on.
    (StatusCode::BAD_GATEWAY, "map data unavailable").into_response()
}

/// Serve `key` from the lake, or fetch it from `url`, store it, and serve it.
async fn cached_or_fetch(
    state: &AppState,
    key: &str,
    url: &str,
    allowed: &[&str],
) -> Result<Vec<u8>, Error> {
    if let Ok(bytes) = state.storage.download(key).await {
        return Ok(bytes);
    }
    let bytes = fetch_checked(url, allowed).await?;
    // Best-effort cache; the person still gets the map if the write fails.
    if let Err(e) = state.storage.upload(key, bytes.clone()).await {
        tracing::warn!("atlas: could not cache {key}: {e}");
    }
    Ok(bytes)
}

fn purge_legacy_raster_cache_once() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        tokio::spawn(async {
            let dir = crate::storage::lake::lake_root().join(LEGACY_RASTER_CACHE);
            match tokio::fs::remove_dir_all(&dir).await {
                Ok(()) => tracing::info!("atlas: removed the legacy raster tile cache at {dir:?}"),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => tracing::warn!("atlas: could not remove {dir:?}: {e}"),
            }
        });
    });
}

// ─── Style ──────────────────────────────────────────────────────────────────

/// Revalidate every load. The style is small and comes from the box, and a
/// fix to it must reach a browser that already holds the old one at once,
/// not a day later.
const STYLE_CACHE: &str = "no-cache";

/// Rewrite an OpenFreeMap style so every URL in it names this box.
///
/// Only the `openmaptiles` vector source survives; the tiles go through
/// `/api/map/vt`, the glyphs through `/api/map/fonts`, the sprite through
/// `/api/map/sprite`. Any layer drawing from a dropped source is dropped with
/// it. Paths are root-relative; the client makes them absolute against the
/// box's origin, which only it knows (it differs on the phone).
fn rewrite_style(mut style: Value) -> Result<Value, Error> {
    let bad = |what: &str| Error::Other(format!("atlas: upstream style has no usable {what}"));

    let obj = style.as_object_mut().ok_or_else(|| bad("root object"))?;
    let sources = obj.get("sources").and_then(Value::as_object).ok_or_else(|| bad("sources"))?;
    if !sources.contains_key("openmaptiles") {
        return Err(bad("openmaptiles source"));
    }
    obj.insert(
        "sources".into(),
        json!({
            "openmaptiles": {
                "type": "vector",
                "tiles": ["/api/map/vt/{z}/{x}/{y}"],
                "minzoom": 0,
                "maxzoom": MAX_ZOOM,
                "attribution": ATTRIBUTION,
            }
        }),
    );

    let sprite = obj.get("sprite").and_then(Value::as_str).ok_or_else(|| bad("sprite"))?;
    let sprite_path = sprite
        .strip_prefix(&format!("{UPSTREAM}/sprites/"))
        .ok_or_else(|| bad("sprite URL"))?;
    let (set, name) = sprite_path.split_once('/').ok_or_else(|| bad("sprite path"))?;
    if !is_token(set) || !is_token(name) {
        return Err(bad("sprite path"));
    }
    obj.insert("sprite".into(), json!(format!("/api/map/sprite/{set}/{name}")));
    obj.insert("glyphs".into(), json!("/api/map/fonts/{fontstack}/{range}.pbf"));

    let layers = obj.get_mut("layers").and_then(Value::as_array_mut).ok_or_else(|| bad("layers"))?;
    layers.retain(|l| match l.get("source").and_then(Value::as_str) {
        None => true, // background
        Some(s) => s == "openmaptiles",
    });
    Ok(style)
}

/// GET /api/map/style/:style — the MapLibre style, every URL pointing here.
pub async fn style_handler(State(state): State<AppState>, Path(style): Path<String>) -> Response {
    purge_legacy_raster_cache_once();
    let Some((_, upstream)) = STYLES.iter().find(|(s, _)| *s == style) else {
        return (StatusCode::NOT_FOUND, "unknown map style").into_response();
    };
    let key = format!("{CACHE_ROOT}/style/{style}.json");
    if let Ok(bytes) = state.storage.download(&key).await {
        return respond(bytes, "application/json", STYLE_CACHE);
    }

    let url = format!("{UPSTREAM}/styles/{upstream}");
    let fetched = async {
        let raw = fetch_checked(&url, &["application/json"]).await?;
        let parsed: Value = serde_json::from_slice(&raw)
            .map_err(|e| Error::Other(format!("atlas style {url}: {e}")))?;
        let rewritten = rewrite_style(parsed)?;
        serde_json::to_vec(&rewritten).map_err(|e| Error::Other(format!("atlas style: {e}")))
    }
    .await;
    match fetched {
        Ok(bytes) => {
            if let Err(e) = state.storage.upload(&key, bytes.clone()).await {
                tracing::warn!("atlas: could not cache {key}: {e}");
            }
            respond(bytes, "application/json", STYLE_CACHE)
        }
        Err(e) => unavailable("style", e),
    }
}

// ─── Vector tiles ───────────────────────────────────────────────────────────

/// OpenFreeMap versions its tile URLs by build (`/planet/20260913_164504_pt/…`)
/// and names the current build in its TileJSON. The build is resolved here and
/// kept for a few hours; it never enters the cache key, so a tile cached from
/// an older build keeps serving (and keeps working offline) after upstream
/// rebuilds.
async fn tile_template() -> Result<String, Error> {
    static TEMPLATE: tokio::sync::Mutex<Option<(Instant, String)>> = tokio::sync::Mutex::const_new(None);
    const TTL: Duration = Duration::from_secs(6 * 3600);

    let mut guard = TEMPLATE.lock().await;
    if let Some((at, t)) = guard.as_ref() {
        if at.elapsed() < TTL {
            return Ok(t.clone());
        }
    }
    let url = format!("{UPSTREAM}/planet");
    let raw = fetch_checked(&url, &["application/json"]).await?;
    let tilejson: Value =
        serde_json::from_slice(&raw).map_err(|e| Error::Other(format!("atlas tilejson: {e}")))?;
    let template = tilejson
        .get("tiles")
        .and_then(|t| t.get(0))
        .and_then(Value::as_str)
        .filter(|t| t.starts_with(&format!("{UPSTREAM}/")) && t.contains("{z}"))
        .ok_or_else(|| Error::Other("atlas tilejson: no tile template".into()))?
        .to_string();
    *guard = Some((Instant::now(), template.clone()));
    Ok(template)
}

/// GET /api/map/vt/:z/:x/:y — one OpenMapTiles vector tile.
pub async fn vector_tile_handler(
    State(state): State<AppState>,
    Path((z, x, y)): Path<(u32, u32, u32)>,
) -> Response {
    // Reject nonsense coordinates so nobody can drive arbitrary upstream URLs.
    if z > MAX_ZOOM || x >= (1u32 << z) || y >= (1u32 << z) {
        return (StatusCode::BAD_REQUEST, "tile out of range").into_response();
    }
    let key = format!("{CACHE_ROOT}/vt/{z}/{x}/{y}.pbf");
    let bytes = match state.storage.download(&key).await {
        Ok(bytes) => Ok(bytes),
        Err(_) => match tile_template().await {
            Ok(t) => {
                let url = t
                    .replace("{z}", &z.to_string())
                    .replace("{x}", &x.to_string())
                    .replace("{y}", &y.to_string());
                cached_or_fetch(&state, &key, &url, VECTOR_TILE_TYPES).await
            }
            Err(e) => Err(e),
        },
    };
    match bytes {
        // The box's copy never changes, so the browser may keep it as long.
        Ok(b) => respond(b, "application/x-protobuf", "public, max-age=31536000, immutable"),
        Err(e) => unavailable("tile", e),
    }
}

// ─── Glyphs + sprite ────────────────────────────────────────────────────────

/// `[a-z0-9_]+` — sprite set and sprite name.
fn is_token(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// A font stack as MapLibre sends it: `Noto Sans Regular`, comma-joined.
fn is_fontstack(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b' ' | b',' | b'-'))
}

/// A glyph range file: `0-255.pbf`.
fn is_glyph_range(s: &str) -> bool {
    let Some(stem) = s.strip_suffix(".pbf") else { return false };
    let Some((a, b)) = stem.split_once('-') else { return false };
    [a, b].iter().all(|n| !n.is_empty() && n.len() <= 5 && n.bytes().all(|c| c.is_ascii_digit()))
}

/// GET /api/map/fonts/:fontstack/:range — label glyphs.
pub async fn glyphs_handler(
    State(state): State<AppState>,
    Path((fontstack, range)): Path<(String, String)>,
) -> Response {
    if !is_fontstack(&fontstack) || !is_glyph_range(&range) {
        return (StatusCode::BAD_REQUEST, "bad glyph request").into_response();
    }
    let key = format!("{CACHE_ROOT}/fonts/{fontstack}/{range}");
    let encoded = fontstack.replace(' ', "%20").replace(',', "%2C");
    let url = format!("{UPSTREAM}/fonts/{encoded}/{range}");
    match cached_or_fetch(&state, &key, &url, &["application/x-protobuf"]).await {
        Ok(b) => respond(b, "application/x-protobuf", "public, max-age=31536000, immutable"),
        Err(e) => unavailable("glyphs", e),
    }
}

/// GET /api/map/sprite/:set/:file — the icon sheet (`ofm.json`, `ofm@2x.png`, …).
/// The set is versioned upstream (`ofm_f384`), so a cached file never goes stale.
pub async fn sprite_handler(
    State(state): State<AppState>,
    Path((set, file)): Path<(String, String)>,
) -> Response {
    let parsed = file
        .rsplit_once('.')
        .map(|(stem, ext)| (stem.strip_suffix("@2x").unwrap_or(stem), ext));
    let content_type = match parsed {
        Some((stem, "json")) if is_token(stem) => "application/json",
        Some((stem, "png")) if is_token(stem) => "image/png",
        _ => return (StatusCode::BAD_REQUEST, "bad sprite request").into_response(),
    };
    if !is_token(&set) {
        return (StatusCode::BAD_REQUEST, "bad sprite request").into_response();
    }
    let key = format!("{CACHE_ROOT}/sprite/{set}/{file}");
    let url = format!("{UPSTREAM}/sprites/{set}/{file}");
    match cached_or_fetch(&state, &key, &url, &[content_type]).await {
        Ok(b) => respond(b, content_type, "public, max-age=31536000, immutable"),
        Err(e) => unavailable("sprite", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upstream_style() -> Value {
        json!({
            "version": 8,
            "sources": {
                "ne2_shaded": {"type": "raster", "tiles": ["https://tiles.openfreemap.org/natural_earth/ne2sr/{z}/{x}/{y}.png"]},
                "openmaptiles": {"type": "vector", "url": "https://tiles.openfreemap.org/planet"}
            },
            "sprite": "https://tiles.openfreemap.org/sprites/ofm_f384/ofm",
            "glyphs": "https://tiles.openfreemap.org/fonts/{fontstack}/{range}.pbf",
            "layers": [
                {"id": "background", "type": "background"},
                {"id": "shade", "type": "raster", "source": "ne2_shaded"},
                {"id": "water", "type": "fill", "source": "openmaptiles", "source-layer": "water"}
            ]
        })
    }

    #[test]
    fn the_rewritten_style_names_no_third_party() {
        let out = rewrite_style(upstream_style()).unwrap();
        let text = out.to_string();
        // The attribution links out by design; nothing the map LOADS may.
        let without_credit = text.replace(&serde_json::to_string(ATTRIBUTION).unwrap(), "");
        assert!(!without_credit.contains("http"), "{without_credit}");
        assert_eq!(out["sprite"], "/api/map/sprite/ofm_f384/ofm");
        assert_eq!(out["glyphs"], "/api/map/fonts/{fontstack}/{range}.pbf");
        assert_eq!(out["sources"]["openmaptiles"]["tiles"][0], "/api/map/vt/{z}/{x}/{y}");
        assert_eq!(out["sources"]["openmaptiles"]["attribution"], ATTRIBUTION);
    }

    #[test]
    fn layers_on_a_dropped_source_go_with_it() {
        let out = rewrite_style(upstream_style()).unwrap();
        let ids: Vec<_> = out["layers"].as_array().unwrap().iter().map(|l| l["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["background", "water"]);
        assert!(out["sources"].get("ne2_shaded").is_none());
    }

    #[test]
    fn a_style_from_somewhere_else_is_refused() {
        let mut s = upstream_style();
        s["sprite"] = json!("https://evil.example.com/sprites/x/y");
        assert!(rewrite_style(s).is_err());
        let mut s = upstream_style();
        s["sources"].as_object_mut().unwrap().remove("openmaptiles");
        assert!(rewrite_style(s).is_err());
    }

    #[test]
    fn request_paths_cannot_escape_the_cache() {
        assert!(is_fontstack("Noto Sans Regular"));
        assert!(is_fontstack("Noto Sans Bold,Noto Sans Regular"));
        assert!(!is_fontstack("../etc"));
        assert!(!is_fontstack("a/b"));
        assert!(is_glyph_range("0-255.pbf"));
        assert!(is_glyph_range("65280-65535.pbf"));
        assert!(!is_glyph_range("0-255"));
        assert!(!is_glyph_range("..-255.pbf"));
        assert!(is_token("ofm_f384"));
        assert!(!is_token("ofm/../x"));
        assert!(!is_token(""));
    }
}

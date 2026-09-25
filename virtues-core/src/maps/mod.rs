//! The box's own maps. See agents/plan/offline-maps-plan.md.
//!
//! Map data lives on the box as Protomaps `.pmtiles` archives, and every tile
//! a browser draws is read out of them here. Nothing about which streets a
//! person looks at leaves the box, and the maps work with the network
//! unplugged.
//!
//! Three tiers of archive, each a fixed, pre-cut file that is identical for
//! every box that has it (so a download never says where in a square someone
//! lives):
//!
//! | File                            | Detail | Covers                 |
//! |---------------------------------|--------|------------------------|
//! | `world.pmtiles`                 | z0–7   | the planet             |
//! | `visited-z5-<x>-<y>.pmtiles`    | z0–13  | one z5 tile, ~1,000 km |
//! | `home-z7-<x>-<y>.pmtiles`       | z0–15  | one z7 tile, ~300 km   |
//!
//! The browser draws the three as separate MapLibre sources (world underneath,
//! home on top), so each keeps its own max zoom and MapLibre overzooms it.
//! Fonts and icons sit beside the archives under `assets/`.

use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use axum::{
    extract::Path,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use pmtiles::{AsyncPmTilesReader, Compression, MmapBackend, TileCoord};
use serde::Serialize;

/// Where the maps live on an installed box, beside the lake rather than in
/// it: the archives are a regenerable cache, and `virtues backup` archives
/// the whole lake.
const WELL_KNOWN_MAPS_DIR: &str = "/var/lib/virtues/maps";

/// Dev-only, relative to virtues-core, like the lake's (`data/` is gitignored).
const DEV_MAPS_DIR_FROM_CORE: &str = "../data/maps";

/// The credit the data's license requires on every map (ODbL).
pub const ATTRIBUTION: &str =
    "<a href=\"https://www.openstreetmap.org/copyright\" target=\"_blank\">&copy; OpenStreetMap</a>";

/// The maps directory: `VIRTUES_MAPS_DIR`, else the box path when
/// `/var/lib/virtues` exists, else the dev path. Same precedence, for the same
/// reasons, as `storage::lake::lake_root`.
pub fn maps_root() -> PathBuf {
    let on_a_box = FsPath::new(WELL_KNOWN_MAPS_DIR)
        .parent()
        .is_some_and(|p| p.is_dir());
    resolve_maps_root(std::env::var("VIRTUES_MAPS_DIR").ok().as_deref(), on_a_box)
}

fn resolve_maps_root(configured: Option<&str>, on_a_box: bool) -> PathBuf {
    if let Some(dir) = configured.filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    if on_a_box {
        return PathBuf::from(WELL_KNOWN_MAPS_DIR);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DEV_MAPS_DIR_FROM_CORE)
}

// ─── The archives on disk ───────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    World,
    Visited,
    Home,
}

impl Tier {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "world" => Some(Self::World),
            "visited" => Some(Self::Visited),
            "home" => Some(Self::Home),
            _ => None,
        }
    }

    /// The zoom of the tile one file of this tier covers.
    fn square_zoom(self) -> u8 {
        match self {
            Self::World => 0,
            Self::Visited => 5,
            Self::Home => 7,
        }
    }

    /// The deepest zoom a file of this tier holds; MapLibre overzooms past it.
    fn max_zoom(self) -> u8 {
        match self {
            Self::World => 7,
            Self::Visited => 13,
            Self::Home => 15,
        }
    }
}

/// The tile one archive covers: `z/x/y` in the web-mercator pyramid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Square {
    z: u8,
    x: u32,
    y: u32,
}

impl Square {
    /// Does this square hold tile `z/x/y`? Below the square's own zoom a file
    /// holds the ancestors of its square (an extract keeps every zoom over its
    /// bounds), so any file under that ancestor answers.
    fn holds(self, z: u8, x: u32, y: u32) -> bool {
        if z >= self.z {
            let d = z - self.z;
            (x >> d, y >> d) == (self.x, self.y)
        } else {
            let d = self.z - z;
            (self.x >> d, self.y >> d) == (x, y)
        }
    }

    /// `[west, south, east, north]` in degrees.
    fn bounds(self) -> [f64; 4] {
        let n = f64::from(1u32 << self.z);
        let lon = |x: f64| x / n * 360.0 - 180.0;
        let lat = |y: f64| (std::f64::consts::PI * (1.0 - 2.0 * y / n)).sinh().atan().to_degrees();
        [
            lon(f64::from(self.x)),
            lat(f64::from(self.y + 1)),
            lon(f64::from(self.x + 1)),
            lat(f64::from(self.y)),
        ]
    }
}

/// Which tier and square a file name claims, or `None` for anything else in
/// the directory (a `.tmp` mid-download, a stray file).
fn parse_file_name(name: &str) -> Option<(Tier, Square)> {
    let stem = name.strip_suffix(".pmtiles")?;
    if stem == "world" {
        return Some((Tier::World, Square { z: 0, x: 0, y: 0 }));
    }
    let mut parts = stem.split('-');
    let tier = Tier::parse(parts.next()?)?;
    let z: u8 = parts.next()?.strip_prefix('z')?.parse().ok()?;
    let x: u32 = parts.next()?.parse().ok()?;
    let y: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || tier == Tier::World || z != tier.square_zoom() {
        return None;
    }
    let n = 1u32 << z;
    (x < n && y < n).then_some((tier, Square { z, x, y }))
}

struct Archive {
    tier: Tier,
    square: Square,
    gzip: bool,
    reader: AsyncPmTilesReader<MmapBackend>,
}

/// Every archive the box holds. Rebuilt whole by [`reload`], never edited in
/// place: a request holds the `Arc` it started with, so a swap mid-request is
/// safe.
#[derive(Default)]
pub struct Registry {
    archives: Vec<Archive>,
}

impl Registry {
    async fn load(dir: &FsPath) -> Self {
        let mut archives = Vec::new();
        let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
            return Self::default(); // no maps yet: every tile is a 204
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name();
            let Some((tier, square)) = name.to_str().and_then(parse_file_name) else {
                continue;
            };
            let path = entry.path();
            let opened = async {
                let backend = MmapBackend::try_from(&path).await?;
                AsyncPmTilesReader::try_from_source(backend).await
            };
            match opened.await {
                Ok(reader) => {
                    let gzip = matches!(reader.get_header().tile_compression, Compression::Gzip);
                    archives.push(Archive { tier, square, gzip, reader });
                }
                // A truncated or corrupt file must not take the other maps down.
                Err(e) => tracing::warn!("maps: skipping {}: {e}", path.display()),
            }
        }
        Self { archives }
    }

    fn find(&self, tier: Tier, z: u8, x: u32, y: u32) -> Option<&Archive> {
        self.archives
            .iter()
            .find(|a| a.tier == tier && a.square.holds(z, x, y))
    }
}

static REGISTRY: tokio::sync::RwLock<Option<Arc<Registry>>> = tokio::sync::RwLock::const_new(None);

async fn registry() -> Arc<Registry> {
    if let Some(r) = REGISTRY.read().await.as_ref() {
        return r.clone();
    }
    let mut slot = REGISTRY.write().await;
    if let Some(r) = slot.as_ref() {
        return r.clone();
    }
    purge_legacy_caches();
    let loaded = Arc::new(Registry::load(&maps_root()).await);
    *slot = Some(loaded.clone());
    loaded
}

/// Re-read the maps directory, after a download or a refresh swaps files in.
pub async fn reload() {
    let fresh = Arc::new(Registry::load(&maps_root()).await);
    *REGISTRY.write().await = Some(fresh);
}

/// The tile caches from before the box had its own maps. `map_tiles/` holds
/// CARTO rasters, including the "API KEY REQUIRED" placeholders CARTO served
/// with HTTP 200 once it went key-only (2026-09-23), cached as if they were
/// tiles. `map_atlas/` is the OpenFreeMap proxy's, on staging boxes only.
/// Nothing reads either; they go once per process.
fn purge_legacy_caches() {
    tokio::spawn(async {
        for name in ["map_tiles", "map_atlas"] {
            let dir = crate::storage::lake::lake_root().join(name);
            match tokio::fs::remove_dir_all(&dir).await {
                Ok(()) => tracing::info!("maps: removed the legacy tile cache at {}", dir.display()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => tracing::warn!("maps: could not remove {}: {e}", dir.display()),
            }
        }
    });
}

// ─── HTTP ───────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct Sources {
    /// The world overview is on disk.
    world: bool,
    /// Bounds `[w, s, e, n]` of each visited square.
    visited: Vec<[f64; 4]>,
    /// Bounds of each home square.
    home: Vec<[f64; 4]>,
    attribution: &'static str,
}

/// GET /api/map/sources — which map data the box holds, so the SPA builds a
/// style with only the sources that exist.
pub async fn sources_handler() -> Json<Sources> {
    let reg = registry().await;
    let bounds = |t: Tier| reg.archives.iter().filter(|a| a.tier == t).map(|a| a.square.bounds()).collect();
    Json(Sources {
        world: reg.archives.iter().any(|a| a.tier == Tier::World),
        visited: bounds(Tier::Visited),
        home: bounds(Tier::Home),
        attribution: ATTRIBUTION,
    })
}

/// GET /api/map/vt/:tier/:z/:x/:y — one vector tile, straight out of an
/// archive. 204 where the box holds nothing: MapLibre draws the gap and the
/// tier underneath shows through.
pub async fn tile_handler(Path((tier, z, x, y)): Path<(String, u8, u32, u32)>) -> Response {
    let Some(tier) = Tier::parse(&tier) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if z > tier.max_zoom() || x >= (1u32 << z) || y >= (1u32 << z) {
        return StatusCode::NO_CONTENT.into_response();
    }
    let reg = registry().await;
    let Some(archive) = reg.find(tier, z, x, y) else {
        return StatusCode::NO_CONTENT.into_response();
    };
    let Ok(coord) = TileCoord::new(z, x, y) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    match archive.reader.get_tile(coord).await {
        Ok(Some(bytes)) => {
            // Archives change only when a refresh swaps them in, every few
            // months; a day in the browser's cache is plenty.
            let mut res = (
                [
                    (header::CONTENT_TYPE, "application/x-protobuf"),
                    (header::CACHE_CONTROL, "public, max-age=86400"),
                ],
                bytes,
            )
                .into_response();
            if archive.gzip {
                // Stored gzipped; the browser inflates it, the box never does.
                res.headers_mut()
                    .insert(header::CONTENT_ENCODING, header::HeaderValue::from_static("gzip"));
            }
            res
        }
        Ok(None) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::warn!("maps: reading {tier:?} {z}/{x}/{y}: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// A font stack as MapLibre sends it: `Noto Sans Regular`, comma-joined.
fn is_fontstack(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b' ' | b',' | b'-'))
}

/// A glyph range file: `0-255.pbf`.
fn is_glyph_range(s: &str) -> bool {
    let Some((a, b)) = s.strip_suffix(".pbf").and_then(|stem| stem.split_once('-')) else {
        return false;
    };
    [a, b].iter().all(|n| !n.is_empty() && n.len() <= 5 && n.bytes().all(|c| c.is_ascii_digit()))
}

/// A sprite file: `light.json`, `dark@2x.png`.
fn sprite_type(file: &str) -> Option<&'static str> {
    let (stem, ext) = file.rsplit_once('.')?;
    let stem = stem.strip_suffix("@2x").unwrap_or(stem);
    let ok = !stem.is_empty() && stem.bytes().all(|b| b.is_ascii_lowercase() || b == b'_');
    match ext {
        "json" if ok => Some("application/json"),
        "png" if ok => Some("image/png"),
        _ => None,
    }
}

async fn asset(path: PathBuf, content_type: &'static str) -> Response {
    match tokio::fs::read(&path).await {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, content_type),
                (header::CACHE_CONTROL, "public, max-age=86400"),
            ],
            bytes,
        )
            .into_response(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::warn!("maps: reading {}: {e}", path.display());
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// GET /api/map/fonts/:fontstack/:range — label glyphs, shipped with the maps.
pub async fn glyphs_handler(Path((fontstack, range)): Path<(String, String)>) -> Response {
    // MapLibre asks for a comma-joined stack; the files are per font, and the
    // styles here only ever name one.
    let font = fontstack.split(',').next().unwrap_or_default();
    if !is_fontstack(font) || !is_glyph_range(&range) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    asset(maps_root().join("assets/fonts").join(font).join(&range), "application/x-protobuf").await
}

/// GET /api/map/sprite/:file — the icon sheet for a flavor.
pub async fn sprite_handler(Path(file): Path<String>) -> Response {
    let Some(content_type) = sprite_type(&file) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    asset(maps_root().join("assets/sprites").join(&file), content_type).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_name_their_tier_and_square() {
        assert_eq!(parse_file_name("world.pmtiles"), Some((Tier::World, Square { z: 0, x: 0, y: 0 })));
        assert_eq!(parse_file_name("home-z7-29-53.pmtiles"), Some((Tier::Home, Square { z: 7, x: 29, y: 53 })));
        assert_eq!(parse_file_name("visited-z5-7-13.pmtiles"), Some((Tier::Visited, Square { z: 5, x: 7, y: 13 })));
    }

    #[test]
    fn a_misnamed_or_half_written_file_is_ignored() {
        for name in [
            "home-z7-29-53.pmtiles.tmp",
            "home-z5-7-13.pmtiles",     // wrong zoom for the tier
            "visited-z5-32-0.pmtiles",  // x out of range at z5
            "world-z0-0-0.pmtiles",
            "home-z7-29.pmtiles",
            "home-z7-29-53-1.pmtiles",
            "notes.txt",
        ] {
            assert_eq!(parse_file_name(name), None, "{name}");
        }
    }

    #[test]
    fn a_square_holds_its_descendants_and_its_ancestors() {
        let home = Square { z: 7, x: 29, y: 53 };
        assert!(home.holds(7, 29, 53));
        assert!(home.holds(15, 29 << 8, 53 << 8));
        assert!(home.holds(15, (29 << 8) + 255, (53 << 8) + 255));
        assert!(!home.holds(15, 30 << 8, 53 << 8));
        assert!(home.holds(5, 29 >> 2, 53 >> 2));
        assert!(home.holds(0, 0, 0));
        assert!(!home.holds(5, 0, 0));
    }

    #[test]
    fn square_bounds_are_the_tile_edges() {
        let [w, s, e, n] = Square { z: 1, x: 0, y: 0 }.bounds();
        assert_eq!((w, e), (-180.0, 0.0));
        assert!((s - 0.0).abs() < 1e-9 && (n - 85.051_128_78).abs() < 1e-6);
    }

    #[test]
    fn asset_paths_cannot_escape_the_maps_directory() {
        assert!(is_fontstack("Noto Sans Regular"));
        assert!(!is_fontstack("../etc"));
        assert!(!is_fontstack("a/b"));
        assert!(is_glyph_range("0-255.pbf"));
        assert!(!is_glyph_range("../0-255.pbf"));
        assert!(!is_glyph_range("0-255"));
        assert_eq!(sprite_type("light@2x.png"), Some("image/png"));
        assert_eq!(sprite_type("dark.json"), Some("application/json"));
        assert_eq!(sprite_type("../light.json"), None);
        assert_eq!(sprite_type("light.svg"), None);
    }

    #[test]
    fn the_maps_directory_follows_the_lake_precedence() {
        assert_eq!(resolve_maps_root(Some("/srv/maps"), true), PathBuf::from("/srv/maps"));
        assert_eq!(resolve_maps_root(Some(""), true), PathBuf::from(WELL_KNOWN_MAPS_DIR));
        assert!(resolve_maps_root(None, false).ends_with("data/maps"));
    }
}

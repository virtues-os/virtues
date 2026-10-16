//! Open water is not a place. A stop whose centre lies on a lake, a river or
//! the sea - drifting in a boat, a long wait on a ferry - is no stay, so its
//! time joins the travel around it.
//!
//! The water is the box's own map data: the `water` layer of the Protomaps
//! tiles the map draws, read only where the box holds street detail (a home
//! or a visited square). The world overview's shorelines are too coarse to
//! judge a stop by, so outside those squares, or when a tile can't be read,
//! a stop is kept.

use std::collections::HashMap;

use super::mvt::{self, Polygon};

/// The most detailed zooms of a home square (15) and a visited one (13).
const DETAIL_ZOOMS: [u8; 2] = [15, 13];

/// Water polygons that are open water. A pool, a fountain, a dry basin or a
/// ditch is water on the map, but nowhere a stop is a boat.
fn is_open(kind: &str, detail: Option<&str>) -> bool {
    matches!(kind, "water" | "lake" | "ocean") && !matches!(detail, Some("basin" | "ditch"))
}

struct Tile {
    z: u8,
    x: u32,
    y: u32,
    extent: f64,
    open_water: Vec<Polygon>,
}

impl Tile {
    fn holds(&self, lat: f64, lon: f64) -> bool {
        let (px, py) = mvt::to_tile(self.z, self.x, self.y, self.extent, lat, lon);
        self.open_water.iter().any(|p| inside(px, py, p))
    }
}

/// Tiles read once per run, keyed by the point's z15 tile.
#[derive(Default)]
pub(crate) struct Water {
    tiles: HashMap<(u32, u32), Option<Tile>>,
}

impl Water {
    /// Whether a point lies on open water, by the box's most detailed tile
    /// holding it; `false` where it holds none.
    pub(crate) async fn covers(&mut self, lat: f64, lon: f64) -> bool {
        let key = super::tile_at(lat, lon, 15);
        if !self.tiles.contains_key(&key) {
            let tile = detailed(lat, lon).await;
            self.tiles.insert(key, tile);
        }
        self.tiles[&key].as_ref().is_some_and(|t| t.holds(lat, lon))
    }
}

async fn detailed(lat: f64, lon: f64) -> Option<Tile> {
    for z in DETAIL_ZOOMS {
        let (x, y) = super::tile_at(lat, lon, z);
        let Some(t) = super::tile(z, x, y).await else { continue };
        return match decode(&t.mvt) {
            Some((extent, open_water)) => Some(Tile { z, x, y, extent, open_water }),
            None => {
                tracing::warn!(z, x, y, "maps: a tile's water didn't decode; its stops are kept");
                None
            }
        };
    }
    None
}

/// Even-odd over every ring of one polygon, so a point on an island in a
/// lake (a hole) is not on the water.
pub(super) fn inside(px: f64, py: f64, rings: &[Vec<(f64, f64)>]) -> bool {
    let mut odd = false;
    for ring in rings {
        let n = ring.len();
        for i in 0..n {
            let (xi, yi) = ring[i];
            let (xj, yj) = ring[(i + n - 1) % n];
            if (yi > py) != (yj > py) && px < (xj - xi) * (py - yi) / (yj - yi) + xi {
                odd = !odd;
            }
        }
    }
    odd
}

/// A tile's extent and its open-water polygons; `None` when it doesn't parse.
fn decode(mvt: &[u8]) -> Option<(f64, Vec<Polygon>)> {
    let Some(layer) = mvt::layer(mvt, "water")? else {
        // No water layer: no water here.
        return Some((4096.0, Vec::new()));
    };
    let open = layer
        .features
        .iter()
        // Type 3 is a polygon; points and lines (a river's centre line) cover nothing.
        .filter(|f| f.kind == 3 && f.tags.get("kind").is_some_and(|k| is_open(k, f.tags.get("kind_detail").copied())))
        .map(|f| mvt::rings(&f.geometry))
        .collect();
    Some((layer.extent, open))
}

#[cfg(test)]
mod tests {
    use super::super::mvt::build::{encode_layer, encode_tile, feature, zz};
    use super::*;

    /// A closed ring as MVT commands, starting from the cursor at `from`.
    fn ring(from: (i32, i32), pts: &[(i32, i32)]) -> (Vec<u32>, (i32, i32)) {
        let mut g = vec![9, zz(pts[0].0 - from.0), zz(pts[0].1 - from.1), 2 | ((pts.len() as u32 - 1) << 3)];
        for w in pts.windows(2) {
            g.push(zz(w[1].0 - w[0].0));
            g.push(zz(w[1].1 - w[0].1));
        }
        g.push(15);
        (g, *pts.last().unwrap())
    }
    fn square(a: i32, b: i32) -> Vec<(i32, i32)> {
        vec![(a, a), (b, a), (b, b), (a, b)]
    }
    /// A tile with one `water` layer: a lake with an island, and a pool.
    fn tile() -> Vec<u8> {
        let (outer, at) = ring((0, 0), &square(1000, 3000));
        let (island, _) = ring(at, &square(1800, 2200));
        let (pool, _) = ring((0, 0), &square(100, 300));
        let features = [feature(3, &[0, 0, 1, 1], &[outer, island].concat()), feature(3, &[0, 2], &pool)];
        encode_tile(&[encode_layer("water", &features, &["kind", "kind_detail"], &["water", "lake", "swimming_pool"])])
    }

    #[test]
    fn a_lake_is_open_water_but_its_island_and_a_pool_are_not() {
        let (extent, open) = decode(&tile()).expect("the tile parses");
        assert_eq!(extent, 4096.0);
        assert_eq!(open.len(), 1, "the pool is not open water");
        assert!(inside(1500.0, 1500.0, &open[0]));
        assert!(!inside(2000.0, 2000.0, &open[0]), "the island is a hole");
        assert!(!inside(200.0, 200.0, &open[0]));
        assert!(!inside(3500.0, 3500.0, &open[0]));
    }

    #[test]
    fn a_tile_without_water_has_none_and_a_broken_one_is_unknown() {
        let t = encode_tile(&[encode_layer("roads", &[], &[], &[])]);
        assert_eq!(decode(&t).map(|(_, p)| p.len()), Some(0));
        assert!(decode(&[0x1a, 0xff]).is_none());
    }

    #[test]
    fn only_open_water_counts() {
        assert!(is_open("water", Some("lake")));
        assert!(is_open("water", None));
        assert!(is_open("ocean", None));
        assert!(!is_open("water", Some("basin")));
        assert!(!is_open("swimming_pool", None));
        assert!(!is_open("fountain", None));
    }

    #[test]
    fn a_point_and_its_tile_units_round_trip() {
        let (z, (x, y)) = (15, super::super::tile_at(12.5, -40.25, 15));
        let (px, py) = mvt::to_tile(z, x, y, 4096.0, 12.5, -40.25);
        let (lat, lon) = mvt::to_lat_lon(z, x, y, 4096.0, px, py);
        assert!((lat - 12.5).abs() < 1e-9 && (lon + 40.25).abs() < 1e-9);
    }
}

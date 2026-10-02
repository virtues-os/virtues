//! Open water is not a place. A stop whose centre lies on a lake, a river or
//! the sea - drifting in a boat, a long wait on a ferry - is no stay, so its
//! time joins the travel around it (the owner's call; the prototype never
//! met it).
//!
//! The water is the box's own map data: the `water` layer of the Protomaps
//! tiles the map draws, read only where the box holds street detail (a home
//! or a visited square). The world overview's shorelines are too coarse to
//! judge a stop by, so outside those squares, or when a tile can't be read,
//! a stop is kept.

use std::collections::HashMap;

use super::Stop;

/// Water polygons that are open water. A pool, a fountain, a dry basin or a
/// ditch is water on the map, but nowhere a stop is a boat.
fn is_open(kind: &str, detail: Option<&str>) -> bool {
    matches!(kind, "water" | "lake" | "ocean") && !matches!(detail, Some("basin" | "ditch"))
}

/// One polygon, as its rings in tile units.
type Polygon = Vec<Vec<(f64, f64)>>;

struct Tile {
    z: u8,
    x: u32,
    y: u32,
    extent: f64,
    open_water: Vec<Polygon>,
}

impl Tile {
    fn holds_water(&self, lat: f64, lon: f64) -> bool {
        let n = f64::from(1u32 << self.z);
        let px = ((lon + 180.0) / 360.0 * n - f64::from(self.x)) * self.extent;
        let merc = lat.to_radians().tan().asinh();
        let py = ((1.0 - merc / std::f64::consts::PI) / 2.0 * n - f64::from(self.y)) * self.extent;
        self.open_water.iter().any(|p| inside(px, py, p))
    }
}

/// For each stop, whether its centre lies on open water.
pub(crate) async fn on_open_water(stops: &[Stop]) -> Vec<bool> {
    // Keyed by the stop's z15 tile, so each tile is read and decoded once.
    let mut tiles: HashMap<(u32, u32), Option<Tile>> = HashMap::new();
    let mut out = Vec::with_capacity(stops.len());
    for s in stops {
        let key = crate::maps::tile_at(s.lat, s.lon, 15);
        if !tiles.contains_key(&key) {
            let tile = match crate::maps::detailed_tile_at(s.lat, s.lon).await {
                Some(t) => match decode(&t.mvt) {
                    Some((extent, open_water)) => Some(Tile { z: t.z, x: t.x, y: t.y, extent, open_water }),
                    None => {
                        tracing::warn!(z = t.z, x = t.x, y = t.y, "timeline: a map tile didn't decode; its stops are kept");
                        None
                    }
                },
                None => None,
            };
            tiles.insert(key, tile);
        }
        out.push(tiles[&key].as_ref().is_some_and(|t| t.holds_water(s.lat, s.lon)));
    }
    out
}

/// Even-odd over every ring of one polygon, so a point on an island in a
/// lake (a hole) is not on the water.
fn inside(px: f64, py: f64, rings: &[Vec<(f64, f64)>]) -> bool {
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

// ---------------------------------------------------------------------------
// The Mapbox Vector Tile format, read only as far as the water layer needs:
// a tile is protobuf layers, a layer names its features' tag keys and values,
// and a feature's geometry is move / line / close commands on a cursor.
// ---------------------------------------------------------------------------

struct Pb<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Pb<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }
    fn done(&self) -> bool {
        self.pos >= self.buf.len()
    }
    fn varint(&mut self) -> Option<u64> {
        let mut v = 0u64;
        let mut shift = 0;
        loop {
            let b = *self.buf.get(self.pos)?;
            self.pos += 1;
            v |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return Some(v);
            }
            shift += 7;
            if shift > 63 {
                return None;
            }
        }
    }
    fn bytes(&mut self) -> Option<&'a [u8]> {
        let len = usize::try_from(self.varint()?).ok()?;
        let end = self.pos.checked_add(len)?;
        let s = self.buf.get(self.pos..end)?;
        self.pos = end;
        Some(s)
    }
    /// The next field's number and wire type.
    fn key(&mut self) -> Option<(u64, u64)> {
        let k = self.varint()?;
        Some((k >> 3, k & 7))
    }
    fn skip(&mut self, wire: u64) -> Option<()> {
        match wire {
            0 => {
                self.varint()?;
            }
            1 => self.pos = self.pos.checked_add(8)?,
            2 => {
                self.bytes()?;
            }
            5 => self.pos = self.pos.checked_add(4)?,
            _ => return None,
        }
        (self.pos <= self.buf.len()).then_some(())
    }
    /// A packed run of varints.
    fn packed(&mut self) -> Option<Vec<u32>> {
        let mut inner = Pb::new(self.bytes()?);
        let mut out = Vec::new();
        while !inner.done() {
            out.push(u32::try_from(inner.varint()?).ok()?);
        }
        Some(out)
    }
}

/// A tile's extent and its open-water polygons; `None` when it doesn't parse.
fn decode(mvt: &[u8]) -> Option<(f64, Vec<Polygon>)> {
    let mut tile = Pb::new(mvt);
    while !tile.done() {
        let (field, wire) = tile.key()?;
        if field != 3 || wire != 2 {
            tile.skip(wire)?;
            continue;
        }
        if let Some(found) = water_layer(tile.bytes()?)? {
            return Some(found);
        }
    }
    // No water layer: no water here.
    Some((4096.0, Vec::new()))
}

/// `Some(None)` for a layer that isn't `water`; `None` when it doesn't parse.
fn water_layer(buf: &[u8]) -> Option<Option<(f64, Vec<Polygon>)>> {
    let (mut name, mut extent) = (None, 4096u64);
    let (mut keys, mut values, mut features) = (Vec::new(), Vec::new(), Vec::new());
    let mut layer = Pb::new(buf);
    while !layer.done() {
        match layer.key()? {
            (1, 2) => name = Some(std::str::from_utf8(layer.bytes()?).ok()?),
            (2, 2) => features.push(layer.bytes()?),
            (3, 2) => keys.push(std::str::from_utf8(layer.bytes()?).ok()?),
            (4, 2) => values.push(string_value(layer.bytes()?)?),
            (5, 0) => extent = layer.varint()?,
            (_, wire) => layer.skip(wire)?,
        }
    }
    if name != Some("water") {
        return Some(None);
    }
    let tag = |tags: &[u32], want: &str| {
        tags.chunks_exact(2)
            .find(|kv| keys.get(kv[0] as usize) == Some(&want))
            .and_then(|kv| values.get(kv[1] as usize).copied().flatten())
    };
    let mut open = Vec::new();
    for f in features {
        let (mut tags, mut kind, mut geometry) = (Vec::new(), 0u64, Vec::new());
        let mut feature = Pb::new(f);
        while !feature.done() {
            match feature.key()? {
                (2, 2) => tags = feature.packed()?,
                (3, 0) => kind = feature.varint()?,
                (4, 2) => geometry = feature.packed()?,
                (_, wire) => feature.skip(wire)?,
            }
        }
        // Type 3 is a polygon; points and lines (a river's centre line) cover nothing.
        if kind == 3 && tag(&tags, "kind").is_some_and(|k| is_open(k, tag(&tags, "kind_detail"))) {
            open.push(rings(&geometry));
        }
    }
    Some(Some((extent as f64, open)))
}

/// A value's string, or `None` inside for a number or a bool.
fn string_value(buf: &[u8]) -> Option<Option<&str>> {
    let mut v = Pb::new(buf);
    let mut out = None;
    while !v.done() {
        match v.key()? {
            (1, 2) => out = Some(std::str::from_utf8(v.bytes()?).ok()?),
            (_, wire) => v.skip(wire)?,
        }
    }
    Some(out)
}

/// A polygon's rings from its geometry commands.
fn rings(geometry: &[u32]) -> Polygon {
    let zigzag = |n: u32| i64::from((n >> 1) as i32 ^ -((n & 1) as i32));
    let (mut x, mut y) = (0i64, 0i64);
    let mut out = Vec::new();
    let mut ring: Vec<(f64, f64)> = Vec::new();
    let mut i = 0;
    while i < geometry.len() {
        let (cmd, count) = (geometry[i] & 7, (geometry[i] >> 3) as usize);
        i += 1;
        match cmd {
            1 | 2 => {
                for _ in 0..count {
                    let (Some(&dx), Some(&dy)) = (geometry.get(i), geometry.get(i + 1)) else {
                        return out;
                    };
                    i += 2;
                    x += zigzag(dx);
                    y += zigzag(dy);
                    if cmd == 1 && !ring.is_empty() {
                        out.push(std::mem::take(&mut ring));
                    }
                    ring.push((x as f64, y as f64));
                }
            }
            7 => {
                if !ring.is_empty() {
                    out.push(std::mem::take(&mut ring));
                }
            }
            _ => return out,
        }
    }
    if !ring.is_empty() {
        out.push(ring);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn varint(mut v: u64, out: &mut Vec<u8>) {
        while v >= 0x80 {
            out.push((v as u8) | 0x80);
            v >>= 7;
        }
        out.push(v as u8);
    }
    fn field(n: u64, bytes: &[u8], out: &mut Vec<u8>) {
        varint((n << 3) | 2, out);
        varint(bytes.len() as u64, out);
        out.extend_from_slice(bytes);
    }
    fn packed(vals: &[u32]) -> Vec<u8> {
        let mut out = Vec::new();
        for &v in vals {
            varint(u64::from(v), &mut out);
        }
        out
    }
    fn zz(n: i32) -> u32 {
        ((n << 1) ^ (n >> 31)) as u32
    }
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
    fn feature(tags: &[u32], geometry: &[u32]) -> Vec<u8> {
        let mut f = Vec::new();
        field(2, &packed(tags), &mut f);
        varint(3 << 3, &mut f);
        varint(3, &mut f);
        field(4, &packed(geometry), &mut f);
        f
    }
    /// A tile with one `water` layer: a lake with an island, and a pool.
    fn tile() -> Vec<u8> {
        let mut layer = Vec::new();
        field(1, b"water", &mut layer);
        let (outer, at) = ring((0, 0), &square(1000, 3000));
        let (island, _) = ring(at, &square(1800, 2200));
        field(2, &feature(&[0, 0, 1, 1], &[outer, island].concat()), &mut layer);
        let (pool, _) = ring((0, 0), &square(100, 300));
        field(2, &feature(&[0, 2], &pool), &mut layer);
        for k in ["kind", "kind_detail"] {
            field(3, k.as_bytes(), &mut layer);
        }
        for v in ["water", "lake", "swimming_pool"] {
            let mut value = Vec::new();
            field(1, v.as_bytes(), &mut value);
            field(4, &value, &mut layer);
        }
        varint(5 << 3, &mut layer);
        varint(4096, &mut layer);
        let mut t = Vec::new();
        field(3, &layer, &mut t);
        t
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
        let mut other = Vec::new();
        field(1, b"roads", &mut other);
        let mut t = Vec::new();
        field(3, &other, &mut t);
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
}

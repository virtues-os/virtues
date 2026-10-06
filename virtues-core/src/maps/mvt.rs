//! The Mapbox Vector Tile format, read only as far as the box needs on the
//! server: a tile is protobuf layers, a layer names its features' tag keys
//! and values, and a feature's geometry is move / line / close commands on a
//! cursor. Place names read the `pois` and `places` layers' points
//! (`naming`).

use std::collections::HashMap;

/// One layer of a tile.
pub(super) struct Layer<'a> {
    pub extent: f64,
    pub features: Vec<Feature<'a>>,
}

pub(super) struct Feature<'a> {
    /// 1 a point, 2 a line, 3 a polygon.
    pub kind: u64,
    /// The feature's string tags; numbers and booleans are left out.
    pub tags: HashMap<&'a str, &'a str>,
    pub geometry: Vec<u32>,
}

/// The layer named `want`: `Some(None)` when the tile has no such layer,
/// `None` when the tile doesn't parse.
pub(super) fn layer<'a>(mvt: &'a [u8], want: &str) -> Option<Option<Layer<'a>>> {
    let mut tile = Pb::new(mvt);
    while !tile.done() {
        let (field, wire) = tile.key()?;
        if field != 3 || wire != 2 {
            tile.skip(wire)?;
            continue;
        }
        if let Some(found) = read_layer(tile.bytes()?, want)? {
            return Some(Some(found));
        }
    }
    Some(None)
}

/// `Some(None)` for a layer that isn't `want`; `None` when it doesn't parse.
fn read_layer<'a>(buf: &'a [u8], want: &str) -> Option<Option<Layer<'a>>> {
    let (mut name, mut extent) = (None, 4096u64);
    let (mut keys, mut values, mut raw) = (Vec::new(), Vec::new(), Vec::new());
    let mut layer = Pb::new(buf);
    while !layer.done() {
        match layer.key()? {
            (1, 2) => name = Some(std::str::from_utf8(layer.bytes()?).ok()?),
            (2, 2) => raw.push(layer.bytes()?),
            (3, 2) => keys.push(std::str::from_utf8(layer.bytes()?).ok()?),
            (4, 2) => values.push(string_value(layer.bytes()?)?),
            (5, 0) => extent = layer.varint()?,
            (_, wire) => layer.skip(wire)?,
        }
    }
    if name != Some(want) {
        return Some(None);
    }
    let mut features = Vec::with_capacity(raw.len());
    for f in raw {
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
        let tags = tags
            .chunks_exact(2)
            .filter_map(|kv| Some((*keys.get(kv[0] as usize)?, (*values.get(kv[1] as usize)?)?)))
            .collect();
        features.push(Feature { kind, tags, geometry });
    }
    Some(Some(Layer { extent: extent as f64, features }))
}

/// A point feature's points, in tile units.
pub(super) fn points(geometry: &[u32]) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    walk(geometry, |cmd, x, y| {
        if cmd == 1 {
            out.push((x, y));
        }
    });
    out
}

/// Runs the geometry commands, calling `at(cmd, x, y)` for each move (1) and
/// line (2) with the cursor after it, and for each close (7).
fn walk(geometry: &[u32], mut at: impl FnMut(u32, f64, f64)) {
    let zigzag = |n: u32| i64::from((n >> 1) as i32 ^ -((n & 1) as i32));
    let (mut x, mut y) = (0i64, 0i64);
    let mut i = 0;
    while i < geometry.len() {
        let (cmd, count) = (geometry[i] & 7, (geometry[i] >> 3) as usize);
        i += 1;
        match cmd {
            1 | 2 => {
                for _ in 0..count {
                    let (Some(&dx), Some(&dy)) = (geometry.get(i), geometry.get(i + 1)) else {
                        return;
                    };
                    i += 2;
                    x += zigzag(dx);
                    y += zigzag(dy);
                    at(cmd, x as f64, y as f64);
                }
            }
            7 => at(7, x as f64, y as f64),
            _ => return,
        }
    }
}

/// Tile units of tile `z/x/y` as a coordinate, `(lat, lon)`.
pub(super) fn to_lat_lon(z: u8, x: u32, y: u32, extent: f64, px: f64, py: f64) -> (f64, f64) {
    let n = f64::from(1u32 << z);
    let gx = f64::from(x) + px / extent;
    let gy = f64::from(y) + py / extent;
    let lon = gx / n * 360.0 - 180.0;
    let lat = (std::f64::consts::PI * (1.0 - 2.0 * gy / n)).sinh().atan().to_degrees();
    (lat, lon)
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

/// Builders for test tiles, shared by the readers' tests.
#[cfg(test)]
pub(super) mod build {
    pub fn varint(mut v: u64, out: &mut Vec<u8>) {
        while v >= 0x80 {
            out.push((v as u8) | 0x80);
            v >>= 7;
        }
        out.push(v as u8);
    }
    pub fn field(n: u64, bytes: &[u8], out: &mut Vec<u8>) {
        varint((n << 3) | 2, out);
        varint(bytes.len() as u64, out);
        out.extend_from_slice(bytes);
    }
    pub fn packed(vals: &[u32]) -> Vec<u8> {
        let mut out = Vec::new();
        for &v in vals {
            varint(u64::from(v), &mut out);
        }
        out
    }
    pub fn zz(n: i32) -> u32 {
        ((n << 1) ^ (n >> 31)) as u32
    }
    /// A feature of type `kind` with tag indexes and geometry commands.
    pub fn feature(kind: u64, tags: &[u32], geometry: &[u32]) -> Vec<u8> {
        let mut f = Vec::new();
        field(2, &packed(tags), &mut f);
        varint(3 << 3, &mut f);
        varint(kind, &mut f);
        field(4, &packed(geometry), &mut f);
        f
    }
    /// A layer from its name, encoded features, keys and string values.
    pub fn encode_layer(name: &str, features: &[Vec<u8>], keys: &[&str], values: &[&str]) -> Vec<u8> {
        let mut layer = Vec::new();
        field(1, name.as_bytes(), &mut layer);
        for f in features {
            field(2, f, &mut layer);
        }
        for k in keys {
            field(3, k.as_bytes(), &mut layer);
        }
        for v in values {
            let mut value = Vec::new();
            field(1, v.as_bytes(), &mut value);
            field(4, &value, &mut layer);
        }
        varint(5 << 3, &mut layer);
        varint(4096, &mut layer);
        layer
    }
    /// A tile from encoded layers.
    pub fn encode_tile(layers: &[Vec<u8>]) -> Vec<u8> {
        let mut t = Vec::new();
        for l in layers {
            field(3, l, &mut t);
        }
        t
    }
}

#[cfg(test)]
mod tests {
    use super::build::*;
    use super::*;

    #[test]
    fn points_and_their_string_tags_are_read() {
        // Two points; the second has a number value, which is dropped.
        let a = feature(1, &[0, 0, 1, 1], &[9, zz(100), zz(200)]);
        let b = feature(1, &[0, 2, 1, 3], &[9, zz(4000), zz(10)]);
        let mut l = encode_layer("pois", &[a, b], &["name", "kind"], &["Cafe", "cafe", "Park"]);
        let mut number = Vec::new();
        varint(4 << 3, &mut number); // value 3: an int, not a string
        varint(7, &mut number);
        field(4, &number, &mut l);
        let t = encode_tile(&[encode_layer("roads", &[], &[], &[]), l]);
        let found = layer_of(&t, "pois");
        assert_eq!(found.features.len(), 2);
        assert_eq!(found.features[0].tags.get("name"), Some(&"Cafe"));
        assert_eq!(found.features[0].tags.get("kind"), Some(&"cafe"));
        assert_eq!(points(&found.features[0].geometry), vec![(100.0, 200.0)]);
        assert_eq!(points(&found.features[1].geometry), vec![(4000.0, 10.0)]);
        assert_eq!(found.features[1].tags.get("kind"), None, "a number tag is left out");
        assert!(layer(&t, "water").expect("parses").is_none());
        assert!(layer(&[0x1a, 0xff], "pois").is_none(), "a broken tile is unknown");
    }

    #[test]
    fn a_tile_unit_is_a_coordinate() {
        // The tile's top-left corner is its own coordinate, and its far corner
        // is the next tile's.
        let (x, y) = crate::maps::tile_at(41.25, 12.5, 15);
        let (la, lo) = to_lat_lon(15, x, y, 4096.0, 0.0, 0.0);
        assert_eq!(crate::maps::tile_at(la - 1e-9, lo + 1e-9, 15), (x, y));
        let (la2, lo2) = to_lat_lon(15, x, y, 4096.0, 4096.0, 4096.0);
        assert_eq!(crate::maps::tile_at(la2 - 1e-9, lo2 + 1e-9, 15), (x + 1, y + 1));
    }

    fn layer_of<'a>(t: &'a [u8], name: &str) -> Layer<'a> {
        layer(t, name).expect("parses").expect("has the layer")
    }
}

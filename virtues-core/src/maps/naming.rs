//! A place's name from the box's own map (`dayback/resolve.py`): the nearest
//! point of interest a person can stay at, and the city the place lies in.
//!
//! Points of interest are read from the `pois` layer of the z15 tiles a home
//! square holds; a place outside every home square has no street detail to
//! match against. The city is the nearest the map labels as one at z7, which
//! the world overview holds everywhere.
//!
//! Nothing here is stored: a place is matched when it is read. A clear match
//! is shown as the place's name; an unclear one is a question for the person,
//! whose answer goes into `wiki_places` (`api::place_review`).

use std::collections::HashMap;

use serde::Serialize;

use super::mvt;

/// A point of interest must lie this close to the place's centre to be a
/// candidate: the map's pins sit 10-25 m off their building, and a large
/// venue is 30-50 m across (`resolve.py:30`).
const RAD_M: f64 = 60.0;
/// A named place a little further out is still worth offering by name
/// (`resolve.py:31`).
const HINT_M: f64 = 250.0;
/// Named places within this feed the answer field's suggestions
/// (`resolve.py:32-33`).
const NEAR_M: f64 = 300.0;
const NEAR_MAX: usize = 40;
/// An airport's label sits far from any gate: name the whole facility from
/// within this (`resolve.py:34`).
const AERO_M: f64 = 3000.0;
/// An airport feature (a gate, an apron, a runway) this close means the
/// place is on airport ground (`resolve.py:35`).
const INFRA_M: f64 = 250.0;
/// The nearest candidate is clear when the next is this much further, or
/// this many times as far (`resolve.py:243`).
const CLEAR_GAP_M: f64 = 15.0;
const CLEAR_RATIO: f64 = 1.6;
/// With this many candidates within 60 m, the spot is too dense for the
/// nearest to be a clear match, however far the next one is: on a street of
/// bars side by side, a stay covers several of them.
const DENSE: usize = 3;
/// Candidates offered in "Which of these were you at?".
const CANDIDATES_MAX: usize = 8;
/// A city further than this from the place names nothing (`build.py:163`).
const CITY_MAX_M: f64 = 90_000.0;
const POI_ZOOM: u8 = 15;
/// The world overview's deepest zoom, held everywhere, where the map labels
/// cities and the larger towns.
const CITY_ZOOM: u8 = 7;

/// Map features that exist only on airport ground (`resolve.py:40`).
const AIRPORT_INFRA: &[&str] = &["gate", "apron", "runway", "taxiway", "helipad", "hangar", "terminal"];

/// Kinds nobody stays at: infrastructure, transport, street furniture
/// (`resolve.py:43-48`).
const EXCLUDE: &[&str] = &[
    "crossing", "bus_stop", "bus_station", "atm", "vending_machine", "parking_entrance", "parking", "parking_space",
    "payment_terminal", "toilets", "bench", "waste_basket", "drinking_water", "fire_hydrant", "recycling", "post_box",
    "telephone", "clock", "fountain", "picnic_table", "bicycle_parking", "motorcycle_parking", "charging_station",
    "gate", "apron", "runway", "taxiway", "helipad", "hangar", "traffic_signals", "street_lamp", "hunting_stand",
    "tree", "give_way", "stop", "bollard", "turning_circle", "elevator", "entrance", "artwork", "viewpoint",
    "information", "board", "map", "hydrant", "waste_disposal", "grit_bin", "street_cabinet", "surveillance",
    "camera", "tower", "mast", "antenna",
];

/// A point of interest's kind as one of a few stable categories
/// (`resolve.py:54-100`); anything unlisted is `other`.
pub(crate) fn category(kind: &str) -> &'static str {
    match kind {
        "restaurant" | "cafe" | "fast_food" | "food_court" | "bar" | "pub" | "biergarten" | "ice_cream" | "bakery"
        | "deli" | "wine" | "brewery" | "winery" | "distillery" | "cafeteria" => "dining",
        "place_of_worship" | "church" | "cathedral" | "chapel" | "mosque" | "synagogue" | "temple" | "shrine"
        | "monastery" => "worship",
        "fitness_centre" | "gym" | "sports_centre" | "stadium" | "swimming_pool" | "dojo" | "yoga" | "climbing" => {
            "fitness"
        }
        "supermarket" | "convenience" | "greengrocer" | "butcher" | "farm" | "marketplace" | "seafood_shop" => "grocery",
        "clothes" | "gift" | "shoes" | "beauty" | "books" | "florist" | "pet" | "houseware" | "flooring" | "frame"
        | "retail" | "department_store" | "mall" | "jewelry" | "sports" | "electronics" | "furniture" | "hardware"
        | "cosmetics" | "toys" | "mobile_phone" | "bicycle" | "car" => "shopping",
        "office" | "coworking_space" | "company" => "office",
        "park" | "garden" | "nature_reserve" | "forest" | "beach" | "playground" | "dog_park" | "pitch" | "marina"
        | "water" | "pedestrian" | "peak" | "trailhead" | "memorial" | "camp_site" | "grass" | "golf_course" | "golf"
        | "meadow" | "recreation_ground" | "common" => "outdoors",
        "hotel" | "motel" | "hostel" | "guest_house" | "resort" | "chalet" => "lodging",
        "cinema" | "theatre" | "nightclub" | "arts_centre" | "museum" | "gallery" | "art" | "attraction" | "zoo"
        | "aquarium" | "casino" | "bowling_alley" | "community_centre" | "pottery" => "entertainment",
        "hospital" | "clinic" | "doctors" | "dentist" | "pharmacy" | "chemist" | "veterinary" | "optician"
        | "hearing_aids" | "massage" => "health",
        "school" | "university" | "college" | "kindergarten" | "library" | "prep_school" | "training"
        | "public_bookcase" | "childcare" => "education",
        "ferry_terminal" | "aerodrome" | "airport" | "station" | "bicycle_rental" | "taxi" => "transit",
        _ => "other",
    }
}

/// A name as the map may spell it twice: the same venue is often mapped
/// twice, as a building and as a point, with "&" for "and" or a curly
/// apostrophe for a straight one.
fn same_name(name: &str) -> String {
    name.to_lowercase()
        .replace('&', " and ")
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Each name once, at its nearest: `items` sorted nearest first.
fn once<T>(items: Vec<T>, name: impl Fn(&T) -> &str) -> Vec<T> {
    let mut seen = std::collections::HashSet::new();
    items.into_iter().filter(|i| seen.insert(same_name(name(i)))).collect()
}

/// A point of interest or a locality, as the map labels it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Label {
    pub lat: f64,
    pub lon: f64,
    pub kind: String,
    /// The map's finer kind: for a locality, `city`, `town`, `village`...
    pub detail: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Candidate {
    pub meters: f64,
    pub kind: String,
    /// The kind's category, written with the name when the person picks it.
    pub category: &'static str,
    pub name: String,
    /// Where the map puts it, so the map can label it while it's offered.
    pub lat: f64,
    pub lon: f64,
}

/// What the map says a place is.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Naming {
    /// `confident`: one clear match, shown as the place's name; `ambiguous`:
    /// several close (or a dense spot), a question; `none`: nothing within
    /// reach.
    pub tier: &'static str,
    /// The clear match's name and category.
    pub name: Option<String>,
    pub category: Option<&'static str>,
    /// Up to eight candidates within 60 m, nearest first.
    pub candidates: Vec<Candidate>,
    /// With no candidate: the nearest named place within 250 m, to offer by name.
    pub hint: Option<Candidate>,
    /// The nearest hotel (or other lodging) within 250 m: after a night
    /// there, the question is whether it was this hotel.
    pub lodging: Option<Candidate>,
    /// Named places within 300 m, nearest first: the answer field's suggestions.
    pub nearby: Vec<String>,
    /// The city the place lies in: its title until it has a name.
    pub city: Option<String>,
    /// The box holds street detail (z15) here; without it nothing can match.
    pub street_detail: bool,
}

/// The match for a place, from the points of interest in its own z15 tile
/// (`centre`) and in that tile and its eight neighbours (`block`), as
/// `resolve.py:225-248` and `211-223` read them.
pub(crate) fn resolve(lat: f64, lon: f64, centre: &[Label], block: &[Label]) -> Naming {
    let dist = |p: &Label| crate::geo::haversine_distance(lat, lon, p.lat, p.lon);
    let mut cands = Vec::new();
    let mut wide = Vec::new();
    let mut infra = 0;
    for p in centre {
        let d = dist(p);
        if AIRPORT_INFRA.contains(&p.kind.as_str()) && d <= INFRA_M {
            infra += 1;
        }
        if EXCLUDE.contains(&p.kind.as_str()) || p.name.is_empty() {
            continue;
        }
        let c = Candidate {
            meters: (d * 10.0).round() / 10.0,
            kind: p.kind.clone(),
            category: category(&p.kind),
            name: p.name.clone(),
            lat: p.lat,
            lon: p.lon,
        };
        if d <= RAD_M {
            cands.push(c);
        } else if d <= HINT_M {
            wide.push(c);
        }
    }
    let mut out = Naming {
        tier: "none",
        nearby: nearby(lat, lon, block),
        lodging: lodging(lat, lon, block),
        street_detail: true,
        ..Naming::default()
    };
    // On airport ground with no business to name: the whole airport, never "which gate?".
    if infra > 0 {
        let aero = block
            .iter()
            .filter(|p| p.kind == "aerodrome" && !p.name.is_empty())
            .map(|p| (dist(p), p))
            .filter(|(d, _)| *d <= AERO_M)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((d, p)) = aero {
            out.tier = "confident";
            out.name = Some(p.name.clone());
            out.category = Some(category("aerodrome"));
            out.candidates = vec![Candidate {
                meters: (d * 10.0).round() / 10.0,
                kind: "aerodrome".into(),
                category: category("aerodrome"),
                name: p.name.clone(),
                lat: p.lat,
                lon: p.lon,
            }];
            return out;
        }
    }
    let by_distance = |a: &Candidate, b: &Candidate| a.meters.total_cmp(&b.meters);
    cands.sort_by(by_distance);
    wide.sort_by(by_distance);
    let mut cands = once(cands, |c| c.name.as_str());
    if cands.is_empty() {
        out.hint = wide.into_iter().next();
        return out;
    }
    let d0 = cands[0].meters;
    let d1 = cands.get(1).map_or(f64::INFINITY, |c| c.meters);
    let clear = cands.len() < DENSE && (cands.len() == 1 || d1 >= d0 + CLEAR_GAP_M || d1 >= CLEAR_RATIO * d0);
    cands.truncate(CANDIDATES_MAX);
    if clear {
        out.tier = "confident";
        out.name = Some(cands[0].name.clone());
        out.category = Some(category(&cands[0].kind));
    } else {
        out.tier = "ambiguous";
    }
    out.candidates = cands;
    out
}

/// The nearest named lodging within 250 m.
fn lodging(lat: f64, lon: f64, block: &[Label]) -> Option<Candidate> {
    block
        .iter()
        .filter(|p| category(&p.kind) == "lodging" && !p.name.is_empty())
        .map(|p| (crate::geo::haversine_distance(lat, lon, p.lat, p.lon), p))
        .filter(|(d, _)| *d <= HINT_M)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(d, p)| Candidate {
            meters: (d * 10.0).round() / 10.0,
            kind: p.kind.clone(),
            category: "lodging",
            name: p.name.clone(),
            lat: p.lat,
            lon: p.lon,
        })
}

/// Named places a person can stay at within 300 m, nearest first, each name
/// once (`resolve.py:211-223`).
fn nearby(lat: f64, lon: f64, block: &[Label]) -> Vec<String> {
    let mut v: Vec<(&str, f64)> = block
        .iter()
        .filter(|p| !EXCLUDE.contains(&p.kind.as_str()) && !p.name.is_empty())
        .map(|p| (p.name.as_str(), crate::geo::haversine_distance(lat, lon, p.lat, p.lon)))
        .filter(|(_, d)| *d <= NEAR_M)
        .collect();
    v.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(b.0)));
    once(v, |(n, _)| n).into_iter().take(NEAR_MAX).map(|(n, _)| n.to_string()).collect()
}

/// The nearest city within 90 km, else the nearest locality of any size: a
/// suburb reads as the city it belongs to ("Austin", not "Rollingwood"), as
/// the prototype's own list did (`build.py:158-163`). The name alone: a
/// state's label is a point, and the nearest one isn't always the state the
/// place is in.
pub(crate) fn city(lat: f64, lon: f64, localities: &[Label]) -> Option<String> {
    let near = |only_cities: bool| {
        localities
            .iter()
            .filter(|t| !only_cities || t.detail == "city")
            .map(|t| (crate::geo::haversine_distance(lat, lon, t.lat, t.lon), t))
            .filter(|(d, _)| *d <= CITY_MAX_M)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, t)| t.name.clone())
    };
    near(true).or_else(|| near(false))
}

/// The points of a tile's layer, filtered to `keep`, as labels. `None` when
/// the tile can't be read.
async fn labels(z: u8, x: u32, y: u32, layer: &str, keep: fn(&str) -> bool) -> Option<Vec<Label>> {
    let t = super::tile(z, x, y).await?;
    let Some(l) = mvt::layer(&t.mvt, layer)? else { return Some(Vec::new()) };
    Some(
        l.features
            .iter()
            .filter(|f| f.kind == 1 && f.tags.get("kind").is_some_and(|k| keep(k)))
            .flat_map(|f| {
                let tag = |k: &str| f.tags.get(k).copied().unwrap_or_default().to_string();
                let (kind, detail, name) = (tag("kind"), tag("kind_detail"), tag("name"));
                mvt::points(&f.geometry).into_iter().map(move |(px, py)| {
                    let (lat, lon) = mvt::to_lat_lon(t.z, t.x, t.y, l.extent, px, py);
                    Label { lat, lon, kind: kind.clone(), detail: detail.clone(), name: name.clone() }
                })
            })
            .collect(),
    )
}

/// Tiles read once per call, keyed by `(layer, z, x, y)`.
#[derive(Default)]
pub(crate) struct Tiles(HashMap<(&'static str, u8, u32, u32), Option<Vec<Label>>>);

impl Tiles {
    async fn get(&mut self, layer: &'static str, z: u8, x: u32, y: u32) -> Option<&Vec<Label>> {
        if !self.0.contains_key(&(layer, z, x, y)) {
            let found = match layer {
                "pois" => labels(z, x, y, layer, |_| true).await,
                _ => labels(z, x, y, layer, |k| k == "locality").await,
            };
            self.0.insert((layer, z, x, y), found);
        }
        self.0[&(layer, z, x, y)].as_ref()
    }

    /// The tile at a point and its eight neighbours; `None` when the centre
    /// tile can't be read.
    async fn block(&mut self, layer: &'static str, z: u8, lat: f64, lon: f64) -> Option<(Vec<Label>, Vec<Label>)> {
        let (cx, cy) = super::tile_at(lat, lon, z);
        let centre = self.get(layer, z, cx, cy).await?.clone();
        let mut block = Vec::new();
        for dx in -1i64..=1 {
            for dy in -1i64..=1 {
                let (Ok(x), Ok(y)) = (u32::try_from(i64::from(cx) + dx), u32::try_from(i64::from(cy) + dy)) else {
                    continue;
                };
                if let Some(l) = self.get(layer, z, x, y).await {
                    block.extend(l.iter().cloned());
                }
            }
        }
        Some((centre, block))
    }

    /// What the box's map says a place is.
    pub(crate) async fn name(&mut self, lat: f64, lon: f64) -> Naming {
        let mut out = match self.block("pois", POI_ZOOM, lat, lon).await {
            // A z15 tile is only ever in a home square: street detail.
            Some((centre, block)) => resolve(lat, lon, &centre, &block),
            None => Naming { tier: "none", ..Naming::default() },
        };
        if let Some((_, block)) = self.block("places", CITY_ZOOM, lat, lon).await {
            out.city = city(lat, lon, &block);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A label `east_m` metres east of the origin.
    fn at(east_m: f64, kind: &str, name: &str) -> Label {
        let (lat, lon) = (30.0_f64, -97.0_f64);
        let dlon = east_m / (111_320.0 * lat.to_radians().cos());
        Label { lat, lon: lon + dlon, kind: kind.into(), detail: String::new(), name: name.into() }
    }
    fn resolve_here(pois: &[Label]) -> Naming {
        resolve(30.0, -97.0, pois, pois)
    }

    #[test]
    fn one_clear_dwellable_match_is_confident() {
        let n = resolve_here(&[at(5.0, "cafe", "Corner Cafe"), at(12.0, "crossing", "Crosswalk"), at(40.0, "bar", "Blue Door")]);
        assert_eq!(n.tier, "confident", "40 m is 8 times 5 m: clearly nearest");
        assert_eq!(n.name.as_deref(), Some("Corner Cafe"));
        assert_eq!(n.category, Some("dining"));
        assert_eq!(n.candidates.len(), 2, "the crosswalk is no candidate");
    }

    #[test]
    fn two_close_matches_are_a_question() {
        let n = resolve_here(&[at(20.0, "restaurant", "Maple Diner"), at(30.0, "restaurant", "Pine Grill")]);
        assert_eq!(n.tier, "ambiguous", "10 m apart and 1.5 times: neither is clear");
        assert_eq!(n.name, None);
        assert_eq!(n.candidates.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["Maple Diner", "Pine Grill"]);
        // By a gap of 15 m or a ratio of 1.6, either is enough.
        assert_eq!(resolve_here(&[at(2.0, "bar", "A"), at(12.0, "bar", "B")]).tier, "confident");
        assert_eq!(resolve_here(&[at(30.0, "bar", "A"), at(46.0, "bar", "B")]).tier, "confident");
    }

    #[test]
    fn a_dense_spot_is_a_question_however_clear_the_nearest() {
        // A street of bars: 1.8 times as far would make the nearest clear,
        // but three within 60 m means the stay may cover several.
        let n = resolve_here(&[
            at(17.0, "cafe", "Corner Cafe"),
            at(31.0, "bar", "Blue Door"),
            at(50.0, "bar", "Pine Grill"),
        ]);
        assert_eq!(n.tier, "ambiguous");
        assert_eq!(n.name, None);
        assert_eq!(n.candidates.len(), 3, "every one of them is offered");
        let many: Vec<Label> = (0..10).map(|i| at(10.0 + 5.0 * f64::from(i), "bar", &format!("Bar {i}"))).collect();
        assert_eq!(resolve_here(&many).candidates.len(), 8);
    }

    #[test]
    fn a_venue_mapped_twice_is_offered_once_at_its_nearest() {
        let n = resolve_here(&[
            at(27.0, "dog_park", "Maple’s Beer & Garden"),
            at(36.0, "biergarten", "Maple's Beer and Garden"),
            at(38.0, "bar", "Blue Door"),
        ]);
        assert_eq!(n.candidates.iter().map(|c| (c.name.as_str(), c.meters)).collect::<Vec<_>>(), [("Maple’s Beer & Garden", 27.0), ("Blue Door", 38.0)]);
        assert_eq!(n.tier, "ambiguous", "11 m apart and 1.4 times: still a question");
        assert_eq!(n.nearby.len(), 2);
    }

    #[test]
    fn nothing_within_60_m_offers_a_hint_within_250_m() {
        let n = resolve_here(&[at(140.0, "golf_course", "Country Club"), at(400.0, "cafe", "Far")]);
        assert_eq!(n.tier, "none");
        assert_eq!(n.hint.as_ref().map(|h| h.name.as_str()), Some("Country Club"));
        assert_eq!(n.nearby, ["Country Club"], "only names within 300 m are suggested");
        assert_eq!(resolve_here(&[at(400.0, "cafe", "Far")]).hint, None);
    }

    #[test]
    fn the_nearest_hotel_within_250_m_is_kept_for_a_night() {
        let n = resolve_here(&[at(40.0, "cafe", "Corner Cafe"), at(120.0, "hotel", "Hotel Rowan"), at(300.0, "hotel", "Far Inn")]);
        let l = n.lodging.expect("a hotel within 250 m");
        assert_eq!(l.name, "Hotel Rowan", "a nearer cafe doesn't hide it");
        assert!((l.meters - 120.0).abs() < 0.5);
        assert_eq!(resolve_here(&[at(300.0, "hotel", "Far Inn")]).lodging, None);
    }

    #[test]
    fn airport_ground_names_the_whole_airport() {
        let pois = [at(30.0, "gate", "Gate 12"), at(50.0, "fast_food", "Taco Stand"), at(2400.0, "aerodrome", "Regional Airport")];
        let n = resolve_here(&pois);
        assert_eq!(n.tier, "confident");
        assert_eq!(n.name.as_deref(), Some("Regional Airport"), "never which gate, nor the stand by it");
        assert_eq!(n.category, Some("transit"));
    }

    #[test]
    fn unnamed_and_excluded_points_never_match() {
        let n = resolve_here(&[at(3.0, "cafe", ""), at(4.0, "bench", "A bench"), at(6.0, "parking", "Lot B")]);
        assert_eq!(n.tier, "none");
        assert!(n.candidates.is_empty() && n.nearby.is_empty());
    }

    #[test]
    fn the_city_is_the_nearest_city_within_90_km_else_any_locality() {
        let city_at = |east_m: f64, detail: &str, name: &str| Label { detail: detail.into(), ..at(east_m, "locality", name) };
        let around = [city_at(4_000.0, "town", "Riverton"), city_at(21_000.0, "city", "Millbrook")];
        assert_eq!(city(30.0, -97.0, &around).as_deref(), Some("Millbrook"), "a suburb reads as its city");
        assert_eq!(city(30.0, -97.0, &[city_at(4_000.0, "town", "Riverton")]).as_deref(), Some("Riverton"));
        assert_eq!(city(30.0, -97.0, &[city_at(95_000.0, "city", "Far City")]), None);
    }

    #[test]
    fn categories_fold_kinds() {
        assert_eq!(category("place_of_worship"), "worship");
        assert_eq!(category("supermarket"), "grocery");
        assert_eq!(category("hotel"), "lodging");
        assert_eq!(category("residential"), "other");
    }
}

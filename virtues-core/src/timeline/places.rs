//! The Timeline's places: where you stopped, found in the raw GPS rather than
//! taken from the visit clusterer, which mis-centres and mis-merges
//! (`dayback/resolve.py`, `dayback/build.py:165-216`). Visits then supply
//! each stay's timing, and Home and Work fall out of where the time went.

use chrono_tz::Tz;

use super::{meters, Fix, Ms, Visit, MIN};

/// A fix the phone rates worse than this is not used to find a stop.
const STOP_ACCURACY_MAX_M: f64 = 50.0;
/// A fix farther than this from BOTH its time-neighbours is a teleport.
const SPIKE_M: f64 = 300.0;
/// A stop is a run of fixes within this of its first fix...
const STOP_RADIUS_M: f64 = 100.0;
/// ...lasting at least this long...
const STOP_MIN: Ms = 20 * MIN;
/// ...with at least this many fixes.
const STOP_MIN_FIXES: usize = 8;
/// Stops closer than this are the same place.
const SAME_PLACE_M: f64 = 80.0;
/// A visit belongs to the nearest place within this; past it, it is a place of its own.
const VISIT_MATCH_M: f64 = 150.0;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Place {
    pub lat: f64,
    pub lon: f64,
    pub visit_count: i32,
    pub dwell: Ms,
    /// Time spent here across local 01:00-05:00: "you slept here".
    pub overnight: Ms,
    pub is_home: bool,
    pub is_work: bool,
}

/// Fixes fit for finding stops: rated within 50 m by the phone, and not a
/// lone spike. The accuracy flag alone is not trusted (fixes it rated within
/// 20 m were found 1,700 m off), so neighbour consistency goes after it.
pub(crate) fn stop_fixes(fixes: &[Fix]) -> Vec<Fix> {
    let rated: Vec<Fix> = fixes
        .iter()
        .filter(|f| f.accuracy_m.is_some_and(|a| a <= STOP_ACCURACY_MAX_M))
        .copied()
        .collect();
    (0..rated.len())
        .filter(|&i| {
            let spike = i > 0
                && i + 1 < rated.len()
                && meters(&rated[i], &rated[i - 1]) > SPIKE_M
                && meters(&rated[i], &rated[i + 1]) > SPIKE_M;
            !spike
        })
        .map(|i| rated[i])
        .collect()
}

/// The centre of every stop: runs of fixes within 100 m of the run's first
/// fix, at least 20 minutes and 8 fixes long; the centre is the median.
pub(crate) fn stops(fixes: &[Fix]) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let n = fixes.len();
    let mut i = 0;
    while i + 1 < n {
        let mut j = i + 1;
        while j < n && meters(&fixes[i], &fixes[j]) <= STOP_RADIUS_M {
            j += 1;
        }
        let run = &fixes[i..j];
        if run[run.len() - 1].t - run[0].t >= STOP_MIN && run.len() >= STOP_MIN_FIXES {
            out.push((median(run.iter().map(|f| f.lat)), median(run.iter().map(|f| f.lon))));
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

/// Stops within 80 m of a place's first stop are that place, re-centred on the
/// mean of its stops.
pub(crate) fn merge_stops(stops: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut groups: Vec<((f64, f64), Vec<(f64, f64)>)> = Vec::new();
    for &s in stops {
        match groups.iter_mut().find(|(first, _)| meters_ll(s, *first) < SAME_PLACE_M) {
            Some((_, members)) => members.push(s),
            None => groups.push((s, vec![s])),
        }
    }
    groups
        .into_iter()
        .map(|(_, m)| {
            let n = m.len() as f64;
            (m.iter().map(|p| p.0).sum::<f64>() / n, m.iter().map(|p| p.1).sum::<f64>() / n)
        })
        .collect()
}

/// Every visit (in time order) given to the nearest place within 150 m, or to
/// a place of its own; places no visit reached are dropped. Returns the
/// places and each visit's place index. `zone_at` is the local time at a
/// coordinate, for the overnight test.
pub(crate) fn assign(
    visits: &[Visit],
    centres: Vec<(f64, f64)>,
    zone_at: &dyn Fn(f64, f64) -> Tz,
) -> (Vec<Place>, Vec<usize>) {
    let mut places: Vec<Place> = centres
        .into_iter()
        .map(|(lat, lon)| Place { lat, lon, visit_count: 0, dwell: 0, overnight: 0, is_home: false, is_work: false })
        .collect();
    let mut of_visit = Vec::with_capacity(visits.len());
    for v in visits {
        let nearest = places
            .iter()
            .enumerate()
            .map(|(i, p)| (i, meters_ll((v.lat, v.lon), (p.lat, p.lon))))
            .min_by(|a, b| a.1.total_cmp(&b.1));
        let i = match nearest {
            Some((i, d)) if d <= VISIT_MATCH_M => i,
            _ => {
                places.push(Place { lat: v.lat, lon: v.lon, visit_count: 0, dwell: 0, overnight: 0, is_home: false, is_work: false });
                places.len() - 1
            }
        };
        let p = &mut places[i];
        p.visit_count += 1;
        p.dwell += v.e - v.s;
        if super::zone::covers_small_hours(v.s, v.e, zone_at(v.lat, v.lon)) {
            p.overnight += v.e - v.s;
        }
        of_visit.push(i);
    }

    // Keep only places a visit reached, and renumber.
    let mut renumber = vec![usize::MAX; places.len()];
    let mut kept = Vec::new();
    for (i, p) in places.into_iter().enumerate() {
        if p.visit_count > 0 {
            renumber[i] = kept.len();
            kept.push(p);
        }
    }
    let of_visit = of_visit.into_iter().map(|i| renumber[i]).collect();

    // Home: the most time over the local small hours. Work: the most time
    // among the other places visited at least twice. Ties go to the first.
    if let Some(home) = first_max(kept.iter().enumerate().map(|(i, p)| (i, p.overnight))) {
        kept[home].is_home = true;
        let others = kept.iter().enumerate().filter(|(i, p)| *i != home && p.visit_count >= 2).map(|(i, p)| (i, p.dwell));
        if let Some(work) = first_max(others) {
            kept[work].is_work = true;
        }
    }
    (kept, of_visit)
}

/// The index of the first largest value.
fn first_max(items: impl Iterator<Item = (usize, Ms)>) -> Option<usize> {
    let mut best: Option<(usize, Ms)> = None;
    for (i, v) in items {
        if best.is_none_or(|(_, b)| v > b) {
            best = Some((i, v));
        }
    }
    best.map(|(i, _)| i)
}

fn meters_ll(a: (f64, f64), b: (f64, f64)) -> f64 {
    crate::geo::haversine_distance(a.0, a.1, b.0, b.1)
}

/// The median, averaging the middle two of an even count.
fn median(values: impl Iterator<Item = f64>) -> f64 {
    let mut v: Vec<f64> = values.collect();
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ~0.0009° of latitude is 100 m.
    fn fix(minute: i64, lat: f64, lon: f64, accuracy: f64) -> Fix {
        Fix { t: minute * MIN, lat, lon, accuracy_m: Some(accuracy), speed_mps: None }
    }
    fn chicago(_: f64, _: f64) -> Tz {
        chrono_tz::America::Chicago
    }

    #[test]
    fn a_lone_spike_and_a_poorly_rated_fix_are_not_used() {
        let fixes = vec![
            fix(0, 30.0, -97.0, 10.0),
            fix(1, 30.01, -97.0, 10.0), // 1.1 km from both neighbours
            fix(2, 30.0, -97.0, 10.0),
            fix(3, 30.0, -97.0, 80.0), // rated worse than 50 m
            fix(4, 30.0, -97.0, 10.0),
        ];
        let kept = stop_fixes(&fixes);
        assert_eq!(kept.iter().map(|f| f.t / MIN).collect::<Vec<_>>(), vec![0, 2, 4]);
    }

    #[test]
    fn a_stop_needs_twenty_minutes_and_eight_fixes() {
        let long: Vec<Fix> = (0..10).map(|m| fix(m * 3, 30.0, -97.0, 5.0)).collect(); // 27 min, 10 fixes
        assert_eq!(stops(&long).len(), 1);
        let short: Vec<Fix> = (0..10).map(|m| fix(m, 30.0, -97.0, 5.0)).collect(); // 9 min
        assert!(stops(&short).is_empty());
        let sparse: Vec<Fix> = (0..5).map(|m| fix(m * 10, 30.0, -97.0, 5.0)).collect(); // 40 min, 5 fixes
        assert!(stops(&sparse).is_empty());
    }

    #[test]
    fn stops_within_eighty_meters_are_one_place_centred_on_them() {
        let merged = merge_stops(&[(30.0, -97.0), (30.0005, -97.0), (30.01, -97.0)]);
        assert_eq!(merged.len(), 2);
        assert!((merged[0].0 - 30.00025).abs() < 1e-9);
    }

    #[test]
    fn a_visit_far_from_every_stop_is_its_own_place_and_home_is_where_you_slept() {
        let visits = vec![
            // An evening-to-morning stay at the first stop (local 19:00 -> 08:00).
            Visit { s: 0, e: 13 * 60 * MIN, lat: 30.0, lon: -97.0 },
            // Two daytime visits 2 km away, with no stop there.
            Visit { s: 14 * 60 * MIN, e: 20 * 60 * MIN, lat: 30.02, lon: -97.0 },
            Visit { s: 38 * 60 * MIN, e: 44 * 60 * MIN, lat: 30.02, lon: -97.0 },
        ];
        // Visit times are minutes after 2026-07-28 00:00Z = 19:00 CDT.
        let base = chrono::DateTime::parse_from_rfc3339("2026-07-28T00:00:00Z").unwrap().timestamp_millis();
        let visits: Vec<Visit> = visits.into_iter().map(|v| Visit { s: v.s + base, e: v.e + base, ..v }).collect();
        let (places, of_visit) = assign(&visits, vec![(30.0, -97.0), (40.0, -90.0)], &chicago);
        assert_eq!(places.len(), 2, "the unvisited stop is dropped, the far visits make one place");
        assert_eq!(of_visit, vec![0, 1, 1]);
        assert!(places[0].is_home);
        assert!(places[1].is_work);
        assert_eq!(places[1].visit_count, 2);
    }
}

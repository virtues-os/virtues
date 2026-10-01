//! The Timeline's places: where you stopped, found in the raw GPS rather than
//! taken from the visit clusterer, which mis-centres and mis-merges
//! (`dayback/resolve.py`, `dayback/build.py:165-216`). Each stop also times
//! its stay (see `Stop`), and Home and Work fall out of where the time went.

use chrono_tz::Tz;

use super::{meters, Fix, Ms, Stop, MIN};

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

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Place {
    pub lat: f64,
    pub lon: f64,
    pub stop_count: i32,
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

/// Every stop: a run of fixes within 100 m of the run's first fix, at least
/// 20 minutes and 8 fixes long, from its first fix to its last; the centre is
/// the median.
pub(crate) fn stops(fixes: &[Fix]) -> Vec<Stop> {
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
            out.push(Stop {
                s: run[0].t,
                e: run[run.len() - 1].t,
                lat: median(run.iter().map(|f| f.lat)),
                lon: median(run.iter().map(|f| f.lon)),
            });
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

/// Stops within 80 m of a place's first stop are that place. Returns each
/// stop's place, the places numbered in the order they were first stopped at.
pub(crate) fn merge_stops(stops: &[Stop]) -> Vec<usize> {
    let mut firsts: Vec<(f64, f64)> = Vec::new();
    stops
        .iter()
        .map(|s| match firsts.iter().position(|f| meters_ll((s.lat, s.lon), *f) < SAME_PLACE_M) {
            Some(i) => i,
            None => {
                firsts.push((s.lat, s.lon));
                firsts.len() - 1
            }
        })
        .collect()
}

/// The places the stops make: each centred on the mean of its stops, with
/// how often and how long you were there and how much of that covered the
/// local small hours (`build.py:194-206`, with stops in place of visits).
/// `zone_at` is the local time at a coordinate, for that test.
pub(crate) fn places(stops: &[Stop], place_of: &[usize], zone_at: &dyn Fn(f64, f64) -> Tz) -> Vec<Place> {
    let count = place_of.iter().max().map_or(0, |m| m + 1);
    let empty = Place { lat: 0.0, lon: 0.0, stop_count: 0, dwell: 0, overnight: 0, is_home: false, is_work: false };
    let mut places = vec![empty; count];
    for (s, &i) in stops.iter().zip(place_of) {
        let p = &mut places[i];
        // Summed here, averaged below.
        p.lat += s.lat;
        p.lon += s.lon;
        p.stop_count += 1;
        p.dwell += s.e - s.s;
        if super::zone::covers_small_hours(s.s, s.e, zone_at(s.lat, s.lon)) {
            p.overnight += s.e - s.s;
        }
    }
    for p in &mut places {
        let n = f64::from(p.stop_count);
        p.lat /= n;
        p.lon /= n;
    }

    // Home: the most time over the local small hours. Work: the most time
    // among the other places stopped at at least twice. Ties go to the first.
    if let Some(home) = first_max(places.iter().enumerate().map(|(i, p)| (i, p.overnight))) {
        places[home].is_home = true;
        let others = places.iter().enumerate().filter(|(i, p)| *i != home && p.stop_count >= 2).map(|(i, p)| (i, p.dwell));
        if let Some(work) = first_max(others) {
            places[work].is_work = true;
        }
    }
    places
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
    fn a_stop_needs_twenty_minutes_and_eight_fixes_and_runs_first_fix_to_last() {
        let long: Vec<Fix> = (0..10).map(|m| fix(m * 3, 30.0, -97.0, 5.0)).collect(); // 27 min, 10 fixes
        let found = stops(&long);
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].s, found[0].e), (0, 27 * MIN));
        let short: Vec<Fix> = (0..10).map(|m| fix(m, 30.0, -97.0, 5.0)).collect(); // 9 min
        assert!(stops(&short).is_empty());
        let sparse: Vec<Fix> = (0..5).map(|m| fix(m * 10, 30.0, -97.0, 5.0)).collect(); // 40 min, 5 fixes
        assert!(stops(&sparse).is_empty());
    }

    fn stop(s: Ms, e: Ms, lat: f64) -> Stop {
        Stop { s, e, lat, lon: -97.0 }
    }

    #[test]
    fn stops_within_eighty_meters_are_one_place_centred_on_them() {
        let all = [stop(0, MIN, 30.0), stop(2 * MIN, 3 * MIN, 30.0005), stop(4 * MIN, 5 * MIN, 30.01)];
        let place_of = merge_stops(&all);
        assert_eq!(place_of, vec![0, 0, 1]);
        let found = places(&all, &place_of, &chicago);
        assert_eq!(found.len(), 2);
        assert!((found[0].lat - 30.00025).abs() < 1e-9);
        assert_eq!(found[0].stop_count, 2);
    }

    #[test]
    fn home_is_where_you_slept_and_work_where_the_day_went() {
        // Minutes after 2026-06-10 00:00Z = 19:00 CDT.
        let base = chrono::DateTime::parse_from_rfc3339("2026-06-10T00:00:00Z").unwrap().timestamp_millis();
        let all = [
            // An evening-to-morning stay (local 19:00 -> 08:00).
            stop(base, base + 13 * 60 * MIN, 30.0),
            // Two daytime stays 2 km away.
            stop(base + 14 * 60 * MIN, base + 20 * 60 * MIN, 30.02),
            stop(base + 38 * 60 * MIN, base + 44 * 60 * MIN, 30.02),
        ];
        let found = places(&all, &merge_stops(&all), &chicago);
        assert!(found[0].is_home);
        assert!(found[1].is_work);
        assert_eq!(found[1].stop_count, 2);
    }
}

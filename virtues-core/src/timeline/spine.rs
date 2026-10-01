//! The day's spine: stays, drives and signal gaps, built once across the
//! whole record so nothing is cut at midnight (`dayback/build.py:269-363`),
//! then each drive either kept as `transit` or, when the phone recorded
//! nothing that moved, stored as `unknown` (the viewer's "Signal gap",
//! `dayback/src/main.js:2530-2535`).

use super::{meters, Fix, Ms, Stop, HOUR, MIN};

/// Fixes within this of a spot's first fix are one spot (`clean_track`).
const CLEAN_RADIUS_M: f64 = 30.0;
/// The track leaving a place by more than this is a drive, however short.
const AWAY_M: f64 = 300.0;
/// Two stops closer than this are the same place.
const SAME_STOP_M: f64 = 300.0;
/// Movement: faster than this...
const MOVING_MPS: f64 = 1.0;
/// ...with pauses up to this merged...
const PAUSE: Ms = 180_000;
/// ...counts when it covers this much track, this many fixes and this much net ground.
const MOVE_MIN_PATH_M: f64 = 150.0;
const MOVE_MIN_FIXES: usize = 3;
const MOVE_MIN_NET_M: f64 = 100.0;
/// A fix the phone rates at or worse than this is a cell tower's guess.
const GPS_ACCURACY_MAX_M: f64 = 100.0;
/// An awake stretch with no fix longer than this breaks a stay...
const MAX_SILENCE: Ms = 2 * HOUR;
/// ...and no stay bridges more awake time than this.
const BACKSTOP: Ms = 16 * HOUR;
/// A drive longer than this with too few fixes or too little ground is a signal gap.
const GAP_MIN: Ms = 30 * MIN;
const GAP_FIX_EVERY: Ms = 15 * MIN;
const GAP_MIN_TRACK_M: f64 = 600.0;
/// Speeds over this between two fixes are glitches, not travel.
const MAX_KMH: f64 = 2000.0;
/// A trip that never reaches this is not a drive: the rail's own line for
/// "Driving" (`apps/web/src/lib/timeline/rail.ts`, `transitTitle`).
const DRIVE_KMH: f64 = 45.0;

/// A point of the cleaned track: a spot's centre, or a moving fix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Point {
    pub t: Ms,
    pub lat: f64,
    pub lon: f64,
}

/// Consecutive fixes within 30 m of an anchor collapse to their centre; a spot
/// held over a minute gets an arrive and a leave point at that centre. Sitting
/// at home stops scribbling kilometres of path.
pub(crate) fn clean_track(fixes: &[Fix]) -> Vec<Point> {
    let mut out = Vec::new();
    let mut spot: Vec<Fix> = Vec::new();
    let flush = |spot: &[Fix], out: &mut Vec<Point>| {
        if spot.is_empty() {
            return;
        }
        let n = spot.len() as f64;
        let lat = round5(spot.iter().map(|f| f.lat).sum::<f64>() / n);
        let lon = round5(spot.iter().map(|f| f.lon).sum::<f64>() / n);
        out.push(Point { t: spot[0].t, lat, lon });
        if spot.len() > 1 && spot[spot.len() - 1].t - spot[0].t > MIN {
            out.push(Point { t: spot[spot.len() - 1].t, lat, lon });
        }
    };
    for f in fixes {
        match spot.first() {
            Some(anchor) if meters(anchor, f) <= CLEAN_RADIUS_M => spot.push(*f),
            _ => {
                flush(&spot, &mut out);
                spot = vec![*f];
            }
        }
    }
    flush(&spot, &mut out);
    out
}

fn round5(x: f64) -> f64 {
    (x * 1e5).round() / 1e5
}

/// Fixes good enough to judge movement and coverage on.
pub(crate) fn gps(fixes: &[Fix]) -> Vec<Fix> {
    fixes.iter().filter(|f| f.accuracy_m.is_none_or(|a| a < GPS_ACCURACY_MAX_M)).copied().collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Move {
    pub s: Ms,
    pub e: Ms,
    pub path_m: f64,
    pub fixes: usize,
}

/// Movement: Virtues' rule (`day_summary.rs`: over 1.0 m/s, pauses under 180 s
/// merged, 150 m of track) plus two guards against a two-fix glitch - at
/// least 3 fixes and 100 m of net ground.
pub(crate) fn moves(gps: &[Fix]) -> Vec<Move> {
    struct Run {
        s: Ms,
        e: Ms,
        path_m: f64,
        n: usize,
        first: Fix,
        last: Fix,
    }
    let mut runs: Vec<Run> = Vec::new();
    let mut cur: Option<Run> = None;
    for pair in gps.windows(2) {
        let (prev, p) = (&pair[0], &pair[1]);
        let secs = ((p.t - prev.t) as f64 / 1000.0).max(1.0);
        let dist = meters(prev, p);
        let speed = match p.speed_mps {
            Some(s) if s >= 0.0 => s,
            _ => dist / secs,
        };
        if speed > MOVING_MPS {
            match cur.as_mut() {
                Some(c) if p.t - c.e <= PAUSE => {
                    c.e = p.t;
                    c.path_m += dist;
                    c.n += 1;
                    c.last = *p;
                }
                _ => {
                    runs.extend(cur.take());
                    cur = Some(Run { s: prev.t, e: p.t, path_m: dist, n: 2, first: *prev, last: *p });
                }
            }
        }
    }
    runs.extend(cur);
    runs.into_iter()
        .filter(|r| r.path_m >= MOVE_MIN_PATH_M && r.n >= MOVE_MIN_FIXES && meters(&r.first, &r.last) >= MOVE_MIN_NET_M)
        .map(|r| Move { s: r.s, e: r.e, path_m: r.path_m, fixes: r.n })
        .collect()
}

/// A span of time, for nights and silences.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Span {
    pub s: Ms,
    pub e: Ms,
}

/// Whether the track covers a stretch: a stay may only run on across time the
/// phone actually recorded. An awake silence over 2 h breaks it, and no stay
/// bridges more than 16 awake hours. A HealthKit night is exempt - a still
/// phone stops reporting while you sleep - and only HealthKit's: the
/// quiet-hours estimate would stack one inference on another (`build.py:295-333`).
pub(crate) struct Coverage<'a> {
    /// Every usable fix time, sorted.
    pub fix_times: &'a [Ms],
    /// HealthKit's own nights, merged.
    pub nights: &'a [Span],
}

impl Coverage<'_> {
    /// The pieces of `a..b` no night covers.
    fn awake(&self, a: Ms, b: Ms) -> Vec<(Ms, Ms)> {
        let mut pieces = vec![(a, b)];
        for n in self.nights {
            let mut next = Vec::new();
            for (p, q) in pieces {
                if n.e <= p || n.s >= q {
                    next.push((p, q));
                    continue;
                }
                if n.s > p {
                    next.push((p, n.s));
                }
                if n.e < q {
                    next.push((n.e, q));
                }
            }
            pieces = next;
        }
        pieces
    }

    /// The awake silences in `a..b` long enough to break a stay.
    fn holes(&self, a: Ms, b: Ms) -> Vec<(Ms, Ms)> {
        let lo = self.fix_times.partition_point(|&t| t <= a);
        let hi = self.fix_times.partition_point(|&t| t < b);
        let mut ts = Vec::with_capacity(hi.saturating_sub(lo) + 2);
        ts.push(a);
        ts.extend_from_slice(&self.fix_times[lo..hi.max(lo)]);
        ts.push(b);
        let mut out = Vec::new();
        for w in ts.windows(2) {
            if w[1] - w[0] > MAX_SILENCE {
                out.extend(self.awake(w[0], w[1]).into_iter().filter(|(p, q)| q - p > MAX_SILENCE));
            }
        }
        out
    }

    pub fn covered(&self, a: Ms, b: Ms) -> bool {
        self.awake(a, b).iter().map(|(p, q)| q - p).sum::<Ms>() <= BACKSTOP && self.holes(a, b).is_empty()
    }

    /// How far a stay ending at `a` can honestly run toward `b`.
    pub fn forward_to(&self, a: Ms, b: Ms) -> Ms {
        if self.covered(a, b) {
            return b;
        }
        self.holes(a, b).first().map_or(a, |h| h.0)
    }

    /// How far a stay starting at `b` can honestly reach back toward `a`.
    pub fn back_to(&self, a: Ms, b: Ms) -> Ms {
        if self.covered(a, b) {
            return a;
        }
        self.holes(a, b).last().map_or(b, |h| h.1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Kind {
    /// At a place: the index into the places.
    Stay(usize),
    /// Moving, or a stretch the track can't place (classified by `classify`).
    Transit,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Seg {
    pub s: Ms,
    pub e: Ms,
    pub kind: Kind,
}

/// The spine, from the stops (in time order, with their place index) and the
/// track (`build.py:335-363`, stops in place of visits): a stay continues
/// across a break between two stops at one place only where the fixes cover
/// it; a drive is the MOVEMENT inside a gap, not the whole gap; a silence
/// becomes a trackless transit (a signal gap to be).
pub(crate) fn spine(stops: &[Stop], place_of: &[usize], clean: &[Point], moves: &[Move], cov: &Coverage) -> Vec<Seg> {
    let mut segs: Vec<Seg> = Vec::new();
    let mut cur: Option<Ms> = None;
    let mut prev: Option<&Stop> = None;
    let transit = |s, e| Seg { s, e, kind: Kind::Transit };

    for (vi, v) in stops.iter().enumerate() {
        let (mut vs, ve) = (v.s, v.e);
        let mut place = place_of[vi];
        if let (Some(c), Some(pv)) = (cur, prev) {
            if vs > c {
                let same = crate::geo::haversine_distance(pv.lat, pv.lon, v.lat, v.lon) <= SAME_STOP_M;
                let inside: Vec<&Move> = moves.iter().filter(|m| m.e > c && m.s < vs).collect();
                let last_is_stay_ending_here =
                    |segs: &Vec<Seg>| segs.last().is_some_and(|l| l.kind != Kind::Transit && l.e == c);
                let away = went_away(clean, c, vs, pv.lat, pv.lon);
                // A walk from a place back to it: the track left, but the next
                // stop is at the same place, nothing on the way reached
                // driving speed, and the fixes cover the time. The stay runs on
                // through it, and the walk is a moment inside it (`moments`),
                // as the prototype showed one (the owner's call: a loop round
                // the block from home is no trip).
                let walked_back = away
                    && segs.last().is_some_and(|l| l.kind == Kind::Stay(place) && l.e == c)
                    && transit_track(clean, c, vs).peak_kmh < DRIVE_KMH
                    && cov.covered(c, vs);
                if walked_back {
                    vs = c;
                } else if away {
                    if inside.is_empty() {
                        // Movement only in the cleaned track: the whole gap is the drive.
                        segs.push(transit(c, vs));
                    } else {
                        let mut ds = c.max(inside.iter().map(|m| m.s).min().expect("non-empty"));
                        let de = vs.min(inside.iter().map(|m| m.e).max().expect("non-empty"));
                        if ds > c {
                            if last_is_stay_ending_here(&segs) {
                                let held = cov.forward_to(c, ds);
                                segs.last_mut().expect("checked").e = held;
                                if ds > held {
                                    segs.push(transit(held, ds));
                                }
                            } else {
                                ds = c;
                            }
                        }
                        segs.push(transit(ds, de));
                        if de < vs {
                            vs = cov.back_to(de, vs);
                            if vs > de {
                                segs.push(transit(de, vs));
                            }
                        }
                    }
                } else if !same && vs > c + 5 * MIN {
                    // A different place, a long gap, no track: unknown travel.
                    segs.push(transit(c, vs));
                } else if same && last_is_stay_ending_here(&segs) {
                    if cov.covered(c, vs) {
                        // The detector blinked and the track saw you stay: the
                        // last stay runs on into this stop, at its own place,
                        // and the silences inside the stop still cut it below.
                        if let Some(Kind::Stay(p)) = segs.last().map(|l| l.kind) {
                            place = p;
                        }
                        vs = c;
                    } else {
                        let held = cov.forward_to(c, vs);
                        segs.last_mut().expect("checked").e = held;
                        vs = cov.back_to(c, vs);
                        if vs > held {
                            segs.push(transit(held, vs));
                        }
                    }
                }
            }
        }
        if let Some(c) = cur {
            vs = vs.max(c);
        }
        // A stop's fixes need only stay within 100 m of its first, so a stop
        // can run on across hours the phone recorded nothing - which a visit,
        // ended by any 5-minute silence, never did. The same coverage rule
        // as between stops holds inside one: an awake silence over 2 h is a
        // stretch of its own (a signal gap), a HealthKit night is not.
        // Two silences with no stay between them (a lone fix apart) are one
        // gap, as the prototype makes one between two visits.
        let mut from = vs;
        let first_new = segs.len();
        // A stay picks up where the same place's stay ended, or starts anew.
        let stay = |segs: &mut Vec<Seg>, s: Ms, e: Ms| match segs.last_mut() {
            Some(last) if last.kind == Kind::Stay(place) && last.e >= s => last.e = last.e.max(e),
            _ => segs.push(Seg { s, e, kind: Kind::Stay(place) }),
        };
        for (hs, he) in cov.holes(vs, ve) {
            if hs > from {
                stay(&mut segs, from, hs);
            }
            let fresh = segs.len() > first_new;
            match segs.last_mut() {
                Some(last) if fresh && last.kind == Kind::Transit && last.e >= hs.max(from) => last.e = he,
                _ => segs.push(transit(hs.max(from), he)),
            }
            from = he;
        }
        if ve > from {
            stay(&mut segs, from, ve);
        }
        cur = Some(cur.map_or(ve, |c| c.max(ve)));
        prev = Some(v);
    }
    segs
}

/// Did the cleaned track get more than 300 m from a spot between two moments?
fn went_away(clean: &[Point], s: Ms, e: Ms, lat: f64, lon: f64) -> bool {
    clean
        .iter()
        .filter(|p| s <= p.t && p.t < e)
        .any(|p| crate::geo::haversine_distance(lat, lon, p.lat, p.lon) > AWAY_M)
}

/// What the track says about a transit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TransitTrack {
    /// Cleaned-track points in `s..e`.
    pub fixes: usize,
    /// Track length over `s..=e`, in metres.
    pub path_m: f64,
    /// The fastest hop over `s..=e`, in km/h.
    pub peak_kmh: f64,
}

pub(crate) fn transit_track(clean: &[Point], s: Ms, e: Ms) -> TransitTrack {
    let inclusive: Vec<&Point> = clean.iter().filter(|p| s <= p.t && p.t <= e).collect();
    let mut path_m = 0.0;
    let mut peak_kmh: f64 = 0.0;
    for w in inclusive.windows(2) {
        let d = crate::geo::haversine_distance(w[0].lat, w[0].lon, w[1].lat, w[1].lon);
        path_m += d;
        let hours = (w[1].t - w[0].t) as f64 / 3_600_000.0;
        if hours > 0.0 {
            let kmh = d / 1000.0 / hours;
            if kmh < MAX_KMH && kmh > peak_kmh {
                peak_kmh = kmh;
            }
        }
    }
    let fixes = clean.iter().filter(|p| s <= p.t && p.t < e).count();
    TransitTrack { fixes, path_m, peak_kmh }
}

/// A transit that never really moved: over 30 minutes, with fewer than one
/// fix per 15 minutes or under 0.6 km of track. The GPS went silent; it was
/// not a trip.
pub(crate) fn is_signal_gap(s: Ms, e: Ms, track: &TransitTrack) -> bool {
    let span = e - s;
    span > GAP_MIN && ((track.fixes as f64) < span as f64 / GAP_FIX_EVERY as f64 || track.path_m < GAP_MIN_TRACK_M)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fix(minute: i64, lat: f64) -> Fix {
        Fix { t: minute * MIN, lat, lon: -30.0, accuracy_m: Some(5.0), speed_mps: None }
    }

    #[test]
    fn a_held_spot_is_an_arrive_and_a_leave_point() {
        let fixes: Vec<Fix> = (0..5).map(|m| fix(m, 0.0 + m as f64 * 0.00001)).chain([fix(10, 0.01)]).collect();
        let clean = clean_track(&fixes);
        assert_eq!(clean.len(), 3);
        assert_eq!((clean[0].t, clean[1].t, clean[2].t), (0, 4 * MIN, 10 * MIN));
        assert_eq!(clean[0].lat, clean[1].lat);
    }

    #[test]
    fn movement_needs_three_fixes_and_net_ground() {
        // 12 fixes a minute apart, 150 m each: a drive.
        let drive: Vec<Fix> = (0..12).map(|m| fix(m, 0.0 + m as f64 * 0.00135)).collect();
        let found = moves(&drive);
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].s, found[0].e, found[0].fixes), (0, 11 * MIN, 12));
        // Two fixes 500 m apart, then still: a glitch, not a move.
        let glitch = vec![fix(0, 0.0), fix(1, 0.0045), fix(2, 0.0045)];
        assert!(moves(&glitch).is_empty());
    }

    #[test]
    fn a_silence_breaks_a_stay_unless_it_was_a_night() {
        let fix_times = [0, 10 * MIN, 5 * HOUR];
        let none = Coverage { fix_times: &fix_times, nights: &[] };
        assert!(!none.covered(0, 5 * HOUR));
        assert_eq!(none.forward_to(0, 5 * HOUR), 10 * MIN);
        assert_eq!(none.back_to(0, 5 * HOUR), 5 * HOUR);
        let night = [Span { s: 30 * MIN, e: 4 * HOUR + 30 * MIN }];
        let slept = Coverage { fix_times: &fix_times, nights: &night };
        assert!(slept.covered(0, 5 * HOUR));
    }

    #[test]
    fn the_backstop_holds_even_with_fixes_all_the_way() {
        let fix_times: Vec<Ms> = (0..=18).map(|h| h * HOUR).collect();
        let cov = Coverage { fix_times: &fix_times, nights: &[] };
        assert!(cov.covered(0, 16 * HOUR));
        assert!(!cov.covered(0, 17 * HOUR));
    }

    #[test]
    fn a_drive_is_the_movement_inside_the_gap() {
        // Stay at A 0-60 min, drive 70-80 min to B 3 km north, stay at B from 100 min.
        let a = Stop { s: 0, e: 60 * MIN, lat: 0.0, lon: -30.0 };
        let b = Stop { s: 100 * MIN, e: 160 * MIN, lat: 0.027, lon: -30.0 };
        let mut fixes: Vec<Fix> = (0..=69).map(|m| fix(m, 0.0)).collect();
        fixes.extend((70..=80).map(|m| fix(m, 0.0 + (m - 70) as f64 * 0.0027)));
        fixes.extend((81..=160).map(|m| fix(m, 0.027)));
        let clean = clean_track(&fixes);
        let times: Vec<Ms> = fixes.iter().map(|f| f.t).collect();
        let cov = Coverage { fix_times: &times, nights: &[] };
        let segs = spine(&[a, b], &[0, 1], &clean, &moves(&fixes), &cov);
        let shape: Vec<(Ms, Ms, Kind)> = segs.iter().map(|s| (s.s / MIN, s.e / MIN, s.kind)).collect();
        // The stay holds until the movement starts (the fix before the first fast one).
        assert_eq!(shape, vec![(0, 70, Kind::Stay(0)), (70, 80, Kind::Transit), (80, 160, Kind::Stay(1))]);
    }

    #[test]
    fn a_walk_back_to_the_same_place_is_part_of_the_stay_but_a_drive_is_not() {
        // At A 0-60 min, out 500 m and back by 80 min, at A again 80-140 min.
        let a1 = Stop { s: 0, e: 60 * MIN, lat: 0.0, lon: -30.0 };
        let a2 = Stop { s: 80 * MIN, e: 140 * MIN, lat: 0.0, lon: -30.0 };
        let trip = |minutes_out: i64| {
            let mut fixes: Vec<Fix> = (0..=60).map(|m| fix(m, 0.0)).collect();
            let step = 0.0045 / minutes_out as f64; // 500 m out, the same back
            fixes.extend((1..=minutes_out).map(|k| fix(60 + k, k as f64 * step)));
            fixes.extend((1..=minutes_out).map(|k| fix(60 + minutes_out + k, 0.0045 - k as f64 * step)));
            fixes.extend((60 + 2 * minutes_out + 1..=140).map(|m| fix(m, 0.0)));
            fixes
        };
        let run = |fixes: Vec<Fix>| {
            let clean = clean_track(&fixes);
            let times: Vec<Ms> = fixes.iter().map(|f| f.t).collect();
            let cov = Coverage { fix_times: &times, nights: &[] };
            spine(&[a1, a2], &[0, 0], &clean, &moves(&fixes), &cov)
        };
        // Ten minutes out and ten back: walking pace, one stay.
        assert_eq!(run(trip(10)), vec![Seg { s: 0, e: 140 * MIN, kind: Kind::Stay(0) }]);
        // A minute out and a minute back: 30 km/h, still under driving speed.
        assert_eq!(run(trip(1)).len(), 1);
        // The same loop at driving speed is a drive between two stays.
        let fast: Vec<Fix> = trip(10)
            .into_iter()
            .map(|f| if f.t > 60 * MIN && f.t < 80 * MIN { Fix { t: 60 * MIN + (f.t - 60 * MIN) / 20, ..f } } else { f })
            .collect();
        let mut fast = fast;
        fast.sort_by_key(|f| f.t);
        assert_eq!(run(fast).iter().filter(|g| g.kind == Kind::Transit).count(), 1);
    }

    #[test]
    fn a_blink_at_the_same_place_is_one_stay_only_where_the_track_covers_it() {
        let a1 = Stop { s: 0, e: 60 * MIN, lat: 0.0, lon: -30.0 };
        let a2 = Stop { s: 5 * HOUR, e: 6 * HOUR, lat: 0.0, lon: -30.0 };
        let still: Vec<Fix> = (0..=360).step_by(5).map(|m| fix(m, 0.0)).collect();
        let clean = clean_track(&still);
        let times: Vec<Ms> = still.iter().map(|f| f.t).collect();
        let covered = Coverage { fix_times: &times, nights: &[] };
        let one = spine(&[a1, a2], &[0, 0], &clean, &[], &covered);
        assert_eq!(one, vec![Seg { s: 0, e: 6 * HOUR, kind: Kind::Stay(0) }]);

        // The phone silent from 1 h to 5 h: the stay stops at the last fix, a gap, then it resumes.
        let gappy: Vec<Fix> = still.into_iter().filter(|f| f.t <= HOUR || f.t >= 5 * HOUR).collect();
        let clean = clean_track(&gappy);
        let times: Vec<Ms> = gappy.iter().map(|f| f.t).collect();
        let silent = Coverage { fix_times: &times, nights: &[] };
        let split = spine(&[a1, a2], &[0, 0], &clean, &[], &silent);
        assert_eq!(
            split,
            vec![
                Seg { s: 0, e: HOUR, kind: Kind::Stay(0) },
                Seg { s: HOUR, e: 5 * HOUR, kind: Kind::Transit },
                Seg { s: 5 * HOUR, e: 6 * HOUR, kind: Kind::Stay(0) },
            ]
        );
        let track = transit_track(&clean, HOUR, 5 * HOUR);
        assert!(is_signal_gap(HOUR, 5 * HOUR, &track));
    }

    #[test]
    fn a_stop_across_an_awake_silence_is_two_stays_and_a_gap_unless_it_was_a_night() {
        // One stop from 0 to 6 h with no fix between 1 h and 5 h.
        let one = Stop { s: 0, e: 6 * HOUR, lat: 0.0, lon: -30.0 };
        let fixes: Vec<Fix> = (0..=360).step_by(5).filter(|m| *m <= 60 || *m >= 300).map(|m| fix(m, 0.0)).collect();
        let clean = clean_track(&fixes);
        let times: Vec<Ms> = fixes.iter().map(|f| f.t).collect();
        let awake = Coverage { fix_times: &times, nights: &[] };
        assert_eq!(
            spine(&[one], &[0], &clean, &[], &awake),
            vec![
                Seg { s: 0, e: HOUR, kind: Kind::Stay(0) },
                Seg { s: HOUR, e: 5 * HOUR, kind: Kind::Transit },
                Seg { s: 5 * HOUR, e: 6 * HOUR, kind: Kind::Stay(0) },
            ]
        );
        let night = [Span { s: HOUR, e: 5 * HOUR }];
        let slept = Coverage { fix_times: &times, nights: &night };
        assert_eq!(spine(&[one], &[0], &clean, &[], &slept), vec![Seg { s: 0, e: 6 * HOUR, kind: Kind::Stay(0) }]);
    }

    #[test]
    fn a_stay_run_on_into_the_next_stop_is_still_cut_by_a_silence_inside_it() {
        // Two stops at one place a minute apart; the second has no fix from 2 h to 6 h.
        let first = Stop { s: 0, e: 30 * MIN, lat: 0.0, lon: -30.0 };
        let second = Stop { s: 31 * MIN, e: 7 * HOUR, lat: 0.0, lon: -30.0 };
        let fixes: Vec<Fix> =
            (0..=420).step_by(5).filter(|m| *m <= 120 || *m >= 360).map(|m| fix(m, 0.0)).chain([fix(31, 0.0)]).collect();
        let mut fixes = fixes;
        fixes.sort_by_key(|f| f.t);
        let clean = clean_track(&fixes);
        let times: Vec<Ms> = fixes.iter().map(|f| f.t).collect();
        let cov = Coverage { fix_times: &times, nights: &[] };
        assert_eq!(
            spine(&[first, second], &[0, 0], &clean, &[], &cov),
            vec![
                Seg { s: 0, e: 2 * HOUR, kind: Kind::Stay(0) },
                Seg { s: 2 * HOUR, e: 6 * HOUR, kind: Kind::Transit },
                Seg { s: 6 * HOUR, e: 7 * HOUR, kind: Kind::Stay(0) },
            ]
        );
    }

    #[test]
    fn two_silences_a_lone_fix_apart_are_one_gap() {
        // One stop from 0 to 9 h: fixes to 1 h, a lone fix at 4 h, fixes from 7 h.
        let one = Stop { s: 0, e: 9 * HOUR, lat: 0.0, lon: -30.0 };
        let fixes: Vec<Fix> =
            (0..=540).step_by(5).filter(|m| *m <= 60 || *m == 240 || *m >= 420).map(|m| fix(m, 0.0)).collect();
        let clean = clean_track(&fixes);
        let times: Vec<Ms> = fixes.iter().map(|f| f.t).collect();
        let cov = Coverage { fix_times: &times, nights: &[] };
        assert_eq!(
            spine(&[one], &[0], &clean, &[], &cov),
            vec![
                Seg { s: 0, e: HOUR, kind: Kind::Stay(0) },
                Seg { s: HOUR, e: 7 * HOUR, kind: Kind::Transit },
                Seg { s: 7 * HOUR, e: 9 * HOUR, kind: Kind::Stay(0) },
            ]
        );
    }
}

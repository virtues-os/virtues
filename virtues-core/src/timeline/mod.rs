//! The Timeline's derived layer, ported from the Dayback prototype
//! (`dayback/resolve.py` + `dayback/build.py`): its own places, the day's
//! stretches in the day's four kinds (`stay`, `transit`, `sleep`, `unknown`)
//! and the moments inside them.
//!
//! [`derive`] computes all of it from the raw record in memory; `store`
//! reads the record, writes the `wiki_timeline_*` tables whole, and serves a
//! window of them to the view. `maintenance::timeline_builder` runs it on the
//! fast clock. Times inside the module are epoch milliseconds, as in the
//! prototype.

mod moments;
mod nights;
mod places;
mod spine;
mod store;
mod zone;

use chrono::{DateTime, NaiveDate};
use chrono_tz::Tz;
use serde_json::json;

pub use store::{
    day_window, lanes, rebuild, voice, window, CalendarEvent, DayWindow, LaneWindow, RebuildStats, StepBin,
    TimelineMoment, TimelinePlace, TimelineSpan, TimelineWindow, VoiceWindow,
};

pub(crate) type Ms = i64;
pub(crate) const MIN: Ms = 60_000;
pub(crate) const HOUR: Ms = 60 * MIN;
/// Step readings are counted in bins this wide, centred (`build.py:381-383`).
const STEP_BIN: Ms = 10 * MIN;

/// One raw GPS fix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Fix {
    pub t: Ms,
    pub lat: f64,
    pub lon: f64,
    pub accuracy_m: Option<f64>,
    pub speed_mps: Option<f64>,
}

/// A stop found in the raw GPS (`dayback/resolve.py` `detect_dwells`): where
/// you stayed, from its first fix to its last. The prototype took a stay's
/// times from the visits table and only its place from the stop; here both
/// come from the stop (Lemur, 2026-09-30), since Virtues' visit finder ends a
/// visit whenever a still phone goes 5 minutes without reporting, and on a
/// sparse day finds no stay at all where the stop finder does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Stop {
    pub s: Ms,
    pub e: Ms,
    pub lat: f64,
    pub lon: f64,
}

pub(crate) fn meters(a: &Fix, b: &Fix) -> f64 {
    crate::geo::haversine_distance(a.lat, a.lon, b.lat, b.lon)
}

/// The raw record the rules read, each list in time order.
pub(crate) struct Record {
    pub fixes: Vec<Fix>,
    pub windows: Vec<moments::Window>,
    /// HealthKit's sleep rows.
    pub sleep_rows: Vec<spine::Span>,
    /// Step readings with a positive count.
    pub step_times: Vec<Ms>,
    /// The box's home zone: a day with no fix is read in it.
    pub home: Tz,
}

/// A stretch of the day, ready to store.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Stretch {
    pub kind: &'static str,
    pub s: Ms,
    pub e: Ms,
    pub place: Option<usize>,
    pub metadata: serde_json::Value,
}

/// A moment, ready to store.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Moment {
    pub kind: &'static str,
    pub s: Ms,
    pub e: Ms,
    pub title: Option<String>,
    pub metadata: serde_json::Value,
}

pub(crate) struct Derived {
    pub places: Vec<places::Place>,
    pub stretches: Vec<Stretch>,
    pub moments: Vec<Moment>,
}

/// Everything the Timeline shows, from the raw record.
pub(crate) fn derive(r: &Record) -> Derived {
    let zone_at = |lat: f64, lon: f64| zone::at(lat, lon).unwrap_or(r.home);

    let stops = places::stops(&places::stop_fixes(&r.fixes));
    let place_of = places::merge_stops(&stops);
    let places = places::places(&stops, &place_of, &zone_at);

    let clean = spine::clean_track(&r.fixes);
    let gps = spine::gps(&r.fixes);
    let moves = spine::moves(&gps);
    let gps_times: Vec<Ms> = gps.iter().map(|f| f.t).collect();
    let healthkit = nights::healthkit_nights(&r.sleep_rows);
    let coverage = spine::Coverage { fix_times: &gps_times, nights: &healthkit };
    let segs = spine::spine(&stops, &place_of, &clean, &moves, &coverage);

    let talk: Vec<spine::Span> =
        r.windows.iter().filter(|w| w.speakers >= 2).map(|w| spine::Span { s: w.s, e: w.e }).collect();
    let mut step_bins: Vec<Ms> = r.step_times.iter().map(|t| t.div_euclid(STEP_BIN) * STEP_BIN + STEP_BIN / 2).collect();
    step_bins.dedup();

    let mut stretches = Vec::new();
    for g in &segs {
        match g.kind {
            spine::Kind::Stay(i) => {
                stretches.push(Stretch { kind: "stay", s: g.s, e: g.e, place: Some(i), metadata: json!({}) })
            }
            spine::Kind::Transit => {
                let track = spine::transit_track(&clean, g.s, g.e);
                if spine::is_signal_gap(g.s, g.e, &track) {
                    // What the other streams did while location was quiet: the
                    // view's verdict ("Nothing recorded" / "Phone on, mic
                    // active" / "Phone on but idle", `main.js:2536-2544`).
                    let conversations = talk.iter().filter(|c| c.e > g.s && c.s < g.e).count();
                    let step_bins_inside = step_bins.iter().filter(|&&t| g.s <= t && t < g.e).count();
                    stretches.push(Stretch {
                        kind: "unknown",
                        s: g.s,
                        e: g.e,
                        place: None,
                        metadata: json!({
                            "fix_count": track.fixes,
                            "conversation_count": conversations,
                            "step_bin_count": step_bins_inside,
                        }),
                    });
                } else {
                    stretches.push(Stretch {
                        kind: "transit",
                        s: g.s,
                        e: g.e,
                        place: None,
                        metadata: json!({ "path_meters": track.path_m.round(), "peak_kmh": track.peak_kmh.round() }),
                    });
                }
            }
        }
    }

    let fix_coords: Vec<(f64, f64)> = r.fixes.iter().map(|f| (f.lat, f.lon)).collect();
    let fix_times: Vec<Ms> = r.fixes.iter().map(|f| f.t).collect();
    let days = record_days(r)
        .map(|dates| {
            let zones: Vec<Tz> = dates.iter().map(|&d| zone::of_day(d, &fix_times, &fix_coords, r.home)).collect();
            dates
                .iter()
                .enumerate()
                .map(|(i, &d)| {
                    let next = d.succ_opt().expect("a date within range");
                    let next_zone = zones.get(i + 1).copied().unwrap_or(zones[i]);
                    nights::Day { start: zone::midnight(d, zones[i]), end: zone::midnight(next, next_zone) }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for n in nights::in_bed(&days, &healthkit, &talk, &moves, &step_bins) {
        let source = match n.source {
            nights::Source::HealthKit => "healthkit",
            nights::Source::QuietHours => "quiet_hours",
        };
        stretches.push(Stretch { kind: "sleep", s: n.s, e: n.e, place: None, metadata: json!({ "source": source }) });
    }
    stretches.sort_by_key(|s| (s.s, s.e));

    let mut moments: Vec<Moment> = moments::conversations(&r.windows, &segs)
        .into_iter()
        .map(|c| Moment {
            kind: "conversation",
            s: c.s,
            e: c.e,
            title: Some(c.title),
            metadata: json!({ "window_ids": c.window_ids, "speaker_count": c.speakers }),
        })
        .collect();
    moments.extend(moments::walks(&moves, &segs).into_iter().map(|w| Moment {
        kind: "walk",
        s: w.s,
        e: w.e,
        title: None,
        metadata: json!({ "path_meters": w.path_m.round(), "kmh": (w.kmh * 10.0).round() / 10.0 }),
    }));
    moments.sort_by_key(|m| (m.s, m.e));

    Derived { places, stretches, moments }
}

/// Every date from the record's first to its last (UTC dates of its rows,
/// as the prototype's day window, `build.py:442-455`); none when it is empty.
fn record_days(r: &Record) -> Option<Vec<NaiveDate>> {
    let times = r
        .fixes
        .iter()
        .map(|f| f.t)
        .chain(r.windows.iter().map(|w| w.s))
        .chain(r.sleep_rows.iter().map(|s| s.s))
        .chain(r.step_times.iter().copied());
    let (lo, hi) = times.fold(None, |acc: Option<(Ms, Ms)>, t| Some(acc.map_or((t, t), |(a, b)| (a.min(t), b.max(t)))))?;
    let date = |t: Ms| DateTime::from_timestamp_millis(t).expect("a stored instant").date_naive();
    Some(date(lo).iter_days().take_while(|d| *d <= date(hi)).collect())
}

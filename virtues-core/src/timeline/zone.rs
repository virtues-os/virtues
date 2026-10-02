//! Local time for the Timeline's rules, kept to three operations, each tested:
//! the zone at a coordinate, a date's local midnight, and whether a stay
//! covered the local small hours.
//!
//! Only the coordinate lookup is shared (`crate::timezone::coords_to_tz`, an
//! offline tzf-rs finder); the day arithmetic is here, from the prototype's
//! rules (`dayback/build.py:181-267`), with the zone database standing in for
//! its longitude bands. The bands put a place near a zone line in the wrong
//! zone (Phoenix, for one); the tests below pin it.

use chrono::{DateTime, Duration, LocalResult, NaiveDate, NaiveDateTime, Offset, TimeZone};
use chrono_tz::Tz;

use super::{Ms, HOUR};

/// The IANA zone at a coordinate, or `None` where there is none (open sea) or
/// the finder names a zone the zone database doesn't know.
pub(crate) fn at(lat: f64, lon: f64) -> Option<Tz> {
    crate::timezone::coords_to_tz(lat, lon)?.parse().ok()
}

/// The wall-clock time of instant `t` in `tz`.
pub(crate) fn local(t: Ms, tz: Tz) -> NaiveDateTime {
    DateTime::from_timestamp_millis(t)
        .expect("a stored instant is within chrono's range")
        .with_timezone(&tz)
        .naive_local()
}

/// A date's UTC offset, taken at local noon: the prototype's rule
/// (`build.py:261`), so a daylight-saving day keeps one offset all day.
pub(crate) fn noon_offset(date: NaiveDate, tz: Tz) -> Ms {
    let noon = date.and_hms_opt(12, 0, 0).expect("noon exists");
    let offset = match tz.from_local_datetime(&noon) {
        LocalResult::Single(dt) | LocalResult::Ambiguous(dt, _) => dt.offset().fix(),
        // No zone skips noon; if one ever did, read the offset an hour later.
        LocalResult::None => tz.offset_from_utc_datetime(&(noon + Duration::hours(1))).fix(),
    };
    i64::from(offset.local_minus_utc()) * 1000
}

/// Local midnight of `date` in `tz`, as an instant.
pub(crate) fn midnight(date: NaiveDate, tz: Tz) -> Ms {
    let utc_midnight = date.and_hms_opt(0, 0, 0).expect("midnight exists").and_utc().timestamp_millis();
    utc_midnight - noon_offset(date, tz)
}

/// Whether the span `a`..`d` covers any local 01:00-05:00 at its place: "you
/// slept here". The whole span is tested, never the arrival hour, so coming
/// home at 7 PM and sleeping still counts (`build.py:181-193`).
pub(crate) fn covers_small_hours(a: Ms, d: Ms, tz: Tz) -> bool {
    let (la, ld) = (local(a, tz), local(d, tz));
    let mut day = la.date();
    while day <= ld.date() {
        let from = day.and_hms_opt(1, 0, 0).expect("01:00 as wall time");
        let to = day.and_hms_opt(5, 0, 0).expect("05:00 as wall time");
        if la < to && ld > from {
            return true;
        }
        day = day.succ_opt().expect("a date within range");
    }
    false
}

/// The zone a date woke up in: the zone at its first fix, the day taken in
/// `home` to find that fix; `home` when the day has none (`build.py:252-261`).
/// `fix_times` and `fix_coords` are parallel and sorted by time.
pub(crate) fn of_day(date: NaiveDate, fix_times: &[Ms], fix_coords: &[(f64, f64)], home: Tz) -> Tz {
    let start = midnight(date, home);
    let i = fix_times.partition_point(|&t| t < start);
    match fix_times.get(i) {
        Some(&t) if t < start + 24 * HOUR => {
            let (lat, lon) = fix_coords[i];
            at(lat, lon).unwrap_or(home)
        }
        _ => home,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(s: &str) -> Ms {
        DateTime::parse_from_rfc3339(s).unwrap().timestamp_millis()
    }
    fn date(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    #[test]
    fn the_finder_names_the_zone_the_bands_got_wrong() {
        assert_eq!(at(40.71, -74.01), Some(chrono_tz::America::New_York));
        assert_eq!(at(41.88, -87.63), Some(chrono_tz::America::Chicago));
        assert_eq!(at(39.74, -104.99), Some(chrono_tz::America::Denver));
        assert_eq!(at(34.05, -118.24), Some(chrono_tz::America::Los_Angeles));
        // Between -115 and -102, so the prototype's bands said Mountain time with
        // daylight saving: Phoenix keeps standard time all year.
        assert_eq!(at(33.45, -112.07), Some(chrono_tz::America::Phoenix));
    }

    #[test]
    fn a_summer_midnight_is_daylight_time() {
        let chicago = chrono_tz::America::Chicago;
        assert_eq!(midnight(date("2026-06-10"), chicago), ms("2026-06-10T05:00:00Z"));
        assert_eq!(midnight(date("2026-01-15"), chicago), ms("2026-01-15T06:00:00Z"));
    }

    #[test]
    fn a_daylight_saving_day_keeps_its_noon_offset() {
        // 2026-03-08: clocks spring forward at 02:00 in Chicago. Noon is CDT (-5).
        let chicago = chrono_tz::America::Chicago;
        assert_eq!(midnight(date("2026-03-08"), chicago), ms("2026-03-08T05:00:00Z"));
    }

    #[test]
    fn small_hours_are_read_on_the_whole_span_in_local_time() {
        let chicago = chrono_tz::America::Chicago;
        // Home at 19:00, left 08:00: covers 01:00-05:00 local.
        assert!(covers_small_hours(ms("2026-06-10T00:00:00Z"), ms("2026-06-10T13:00:00Z"), chicago));
        // 22:00-00:30 local: evening only.
        assert!(!covers_small_hours(ms("2026-06-10T03:00:00Z"), ms("2026-06-10T05:30:00Z"), chicago));
        // The same UTC span is 01:00-03:30 in Lisbon: covered.
        assert!(covers_small_hours(ms("2026-06-10T00:00:00Z"), ms("2026-06-10T02:30:00Z"), chrono_tz::Europe::Lisbon));
        // Across the spring-forward night, 00:30-06:00 local still counts.
        assert!(covers_small_hours(ms("2026-03-08T06:30:00Z"), ms("2026-03-08T11:00:00Z"), chicago));
    }

    #[test]
    fn a_day_wakes_up_in_the_zone_of_its_first_fix() {
        let chicago = chrono_tz::America::Chicago;
        let times = vec![ms("2026-06-01T09:00:00Z"), ms("2026-06-02T15:00:00Z")];
        let coords = vec![(41.88, -87.63), (34.05, -118.24)];
        assert_eq!(of_day(date("2026-06-02"), &times, &coords, chicago), chrono_tz::America::Los_Angeles);
        assert_eq!(of_day(date("2026-06-04"), &times, &coords, chicago), chicago);
    }
}

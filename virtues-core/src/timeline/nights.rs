//! In Bed: one night per date - HealthKit's own when it has one, else the
//! quiet hours with no conversation (`dayback/build.py:306-311,464-491`).
//! Shown as "In Bed", never "Asleep": the phone knows you were in bed, and
//! the quiet hours only that no one was talking.

use super::spine::{Move, Span};
use super::{Ms, HOUR, MIN};

/// HealthKit rows closer than this are one night...
const NIGHT_JOIN: Ms = 30 * MIN;
/// ...and a night is at least this long.
const NIGHT_MIN: Ms = 2 * HOUR;
/// A quiet-hours night is at least this long.
const QUIET_MIN: Ms = 150 * MIN;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Source {
    HealthKit,
    QuietHours,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Night {
    pub s: Ms,
    pub e: Ms,
    pub source: Source,
}

/// HealthKit's sleep rows (in start order) merged into nights. Never a single
/// row: that truncates most nights to their first stretch.
pub(crate) fn healthkit_nights(rows: &[Span]) -> Vec<Span> {
    let mut merged: Vec<Span> = Vec::new();
    for r in rows {
        match merged.last_mut() {
            Some(last) if r.s - last.e < NIGHT_JOIN => last.e = last.e.max(r.e),
            _ => merged.push(*r),
        }
    }
    merged.retain(|n| n.e - n.s >= NIGHT_MIN);
    merged
}

/// One day of the record, as the night rules see it.
pub(crate) struct Day {
    /// Local midnight, in the zone the day woke up in.
    pub start: Ms,
    /// The next day's local midnight.
    pub end: Ms,
}

/// A night per day: the longest HealthKit night ending that day; else the
/// quiet stretch between two conversations that holds 03:00, from no earlier
/// than 23:00 the evening before to the first activity after 04:00 (a
/// conversation, a movement, a step reading), no later than 09:00, if it runs
/// 2.5 hours. `talk` is every conversation window with two or more speakers,
/// in start order; `step_bins` the centre of every 10-minute bin holding
/// steps, sorted.
pub(crate) fn in_bed(days: &[Day], healthkit: &[Span], talk: &[Span], moves: &[Move], step_bins: &[Ms]) -> Vec<Night> {
    let mut out = Vec::new();
    for day in days {
        let md = day.start;
        let ending_today = healthkit.iter().filter(|n| md <= n.e && n.e < md + 24 * HOUR);
        let mut longest: Option<&Span> = None;
        for n in ending_today {
            if longest.is_none_or(|l| n.e - n.s > l.e - l.s) {
                longest = Some(n);
            }
        }
        if let Some(n) = longest {
            out.push(Night { s: n.s, e: n.e, source: Source::HealthKit });
            continue;
        }
        let three_am = md + 3 * HOUR;
        let Some(gap) = talk.windows(2).find(|w| w[0].e <= three_am && three_am <= w[1].s) else {
            continue;
        };
        let start = gap[0].e.max(md - HOUR);
        let after = md + 4 * HOUR;
        let first_activity = talk
            .iter()
            .map(|c| c.s)
            .chain(moves.iter().map(|m| m.s))
            .filter(|&t| t >= after)
            .chain(step_bins.iter().copied().filter(|&t| t >= after && t >= day.start && t < day.end))
            .min();
        let end = gap[1].s.min(md + 9 * HOUR).min(first_activity.unwrap_or(md + 9 * HOUR));
        if end - start >= QUIET_MIN {
            out.push(Night { s: start, e: end, source: Source::QuietHours });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(h: f64) -> Ms {
        (h * HOUR as f64) as Ms
    }

    #[test]
    fn healthkit_rows_merge_into_nights_of_two_hours_or_more() {
        let rows = [
            Span { s: at(23.0), e: at(25.0) },
            Span { s: at(25.25), e: at(30.0) }, // 15 min later: the same night
            Span { s: at(40.0), e: at(41.0) },  // an hour's nap: not a night
        ];
        assert_eq!(healthkit_nights(&rows), vec![Span { s: at(23.0), e: at(30.0) }]);
    }

    #[test]
    fn a_healthkit_night_wins_and_quiet_hours_fill_the_rest() {
        // Day 1 starts at 24 h (midnight), day 2 at 48 h.
        let days = [Day { start: at(24.0), end: at(48.0) }, Day { start: at(48.0), end: at(72.0) }];
        let healthkit = [Span { s: at(23.0), e: at(31.0) }];
        // Talk until 23:30 on day 1's evening, then from 08:00 on day 2.
        let talk = [Span { s: at(47.0), e: at(47.5) }, Span { s: at(56.0), e: at(56.5) }];
        let steps = [at(54.5)]; // 06:30 on day 2
        let nights = in_bed(&days, &healthkit, &talk, &[], &steps);
        assert_eq!(
            nights,
            vec![
                Night { s: at(23.0), e: at(31.0), source: Source::HealthKit },
                Night { s: at(47.5), e: at(54.5), source: Source::QuietHours },
            ]
        );
    }
}

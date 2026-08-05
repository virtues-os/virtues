//! Slot bookkeeping — what the scheduler owes, and what it missed.
//!
//! `tokio-cron-scheduler` answers exactly one question: *is it time yet?* It
//! holds no memory of a slot that passed while the process was down, and it
//! cannot tell a job that never fired from a job that fired and did nothing.
//! Everything in this module exists to answer the two questions it cannot:
//!
//! - **Did we miss one?** → catch-up (anacron semantics: at most one, and only
//!   inside the same period; a box off for a week does not replay seven
//!   examens on Monday morning).
//! - **Should something have happened by now?** → overdue, which is the whole
//!   substance of "expected but didn't run" in the needs-attention strip.
//!
//! Both are read off two columns, `last_slot_at` and `next_due_at`, which are
//! *derived state*: recomputed from `cron_schedule` on every sync pass, never
//! authoritative. A row that predates them, or whose schedule was just edited,
//! heals on the next tick. Nothing here should ever be the only copy of a fact.
//!
//! The cron dialect is the scheduler's own — 6 fields, seconds first,
//! interpreted in the box owner's local timezone — so this parses with
//! `croner`, the same crate `tokio-cron-scheduler` uses underneath. Using a
//! second parser with different edge cases would put the slot we *predict*
//! quietly out of step with the slot that actually fires.

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use chrono_tz::Tz;
use croner::Cron;
use sqlx::PgPool;

use crate::error::Result;

/// A schedule at or below this frequency catches up after a missed slot;
/// anything more frequent does not, because the next tick is along shortly and
/// covers it. The plan's rule ("daily-or-less-frequent → true") with a margin
/// for a daily cron whose two occurrences straddle a DST boundary and so sit
/// 23 hours apart rather than 24.
const CATCH_UP_MIN_PERIOD: ChronoDuration = ChronoDuration::hours(23);

/// How late a slot must be before it reads as missed rather than merely
/// in-flight. Two periods, so a single skipped tick is not an alarm, clamped
/// to a window that stays useful at both ends: a 2-minute poll should not be
/// flagged for being 4 minutes late, and a daily applet should not need to be
/// 48 hours late before anyone hears about it.
const GRACE_MIN: ChronoDuration = ChronoDuration::minutes(5);
const GRACE_MAX: ChronoDuration = ChronoDuration::hours(1);

/// Parse a schedule in the scheduler's dialect. `None` when it will not parse
/// — the same expressions `sync_jobs` logs and skips.
pub fn parse(expr: &str) -> Option<Cron> {
    Cron::new(expr).with_seconds_required().parse().ok()
}

/// First occurrence strictly after `after`, in the box's timezone.
pub fn next_after(cron: &Cron, tz: Tz, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let local = after.with_timezone(&tz);
    cron.find_next_occurrence(&local, false)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

/// Most recent occurrence at or before `before`. Used to name the slot a
/// firing belongs to: the job wakes a moment *after* its slot, and recording
/// the wake time instead would drift `next_due_at` forward by that moment on
/// every single firing.
///
/// `croner` only walks forward, so this starts a few periods back and steps up
/// to `before`. Bounded rather than looped-until-done: an expression whose
/// occurrences are far more irregular than its measured period would otherwise
/// spin here, and returning `None` costs only a fallback to the wake time.
pub fn previous_at_or_before(
    cron: &Cron,
    tz: Tz,
    before: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let p = period(cron, tz, before)?;
    let mut cursor = before.checked_sub_signed(p * 3)?;
    let mut last = None;
    for _ in 0..32 {
        let Some(n) = next_after(cron, tz, cursor) else {
            break;
        };
        if n > before {
            break;
        }
        last = Some(n);
        cursor = n;
    }
    last
}

/// The gap between the next two occurrences after `from` — the schedule's
/// period. `None` if the expression yields fewer than two future occurrences.
pub fn period(cron: &Cron, tz: Tz, from: DateTime<Utc>) -> Option<ChronoDuration> {
    let a = next_after(cron, tz, from)?;
    let b = next_after(cron, tz, a)?;
    Some(b - a)
}

/// Whether a missed slot on this schedule is worth catching up.
///
/// Frequency is the whole test, and deliberately so: an examen or a weekly
/// review must happen, while a 15-minute sync that missed a tick is served
/// better by the next tick than by a burst of backfill on boot.
pub fn catches_up(period: ChronoDuration) -> bool {
    period >= CATCH_UP_MIN_PERIOD
}

/// How overdue a slot must be before it counts as missed.
pub fn grace(period: ChronoDuration) -> ChronoDuration {
    (period * 2).clamp(GRACE_MIN, GRACE_MAX)
}

/// Record that `slot` was handled and `next` is now owed.
///
/// "Handled" covers both firing it and consciously declining to catch it up —
/// the decline has to advance the pointer too, or an applet that missed one
/// slot would be reported overdue forever.
pub async fn mark_slot_handled(
    db: &PgPool,
    applet_id: &str,
    slot: DateTime<Utc>,
    next: Option<DateTime<Utc>>,
) -> Result<()> {
    sqlx::query(
        "UPDATE app_applets SET last_slot_at = $2, next_due_at = $3 WHERE id = $1",
    )
    .bind(applet_id)
    .bind(slot)
    .bind(next)
    .execute(db)
    .await?;
    Ok(())
}

/// Set the owed slot without claiming any slot was handled. Used when a
/// schedule is first seen or has just been edited: there is no history to
/// honour, so the applet starts owing its next future occurrence and nothing
/// before it. Notably this means **a newly scheduled applet never catches
/// up** — it has not missed anything, it simply did not exist yet.
pub async fn seed_next_due(
    db: &PgPool,
    applet_id: &str,
    next: Option<DateTime<Utc>>,
) -> Result<()> {
    sqlx::query("UPDATE app_applets SET next_due_at = $2 WHERE id = $1")
        .bind(applet_id)
        .bind(next)
        .execute(db)
        .await?;
    Ok(())
}

/// What the scheduler should do about one applet's slot pointer, decided from
/// the pointer and the clock alone so it can be unit-tested without a database
/// or a running scheduler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotAction {
    /// Pointer is absent or unusable — seed it to this occurrence.
    Seed(Option<DateTime<Utc>>),
    /// Nothing owed yet; the slot is still in the future or inside its grace.
    Wait,
    /// A slot was missed and this schedule catches up: run once now, then
    /// treat `slot` as handled.
    CatchUp { slot: DateTime<Utc> },
    /// A slot was missed and this schedule does not catch up: advance past it
    /// without running. The gap is visible in the run history by its absence.
    Skip { slot: DateTime<Utc> },
}

/// Decide what to do with an applet's slot pointer.
///
/// `now` is a parameter rather than read here so the whole decision table is
/// testable — the catch-up rules are exactly the kind of thing that is easy to
/// write plausibly and get wrong at the edges.
pub fn decide(
    next_due_at: Option<DateTime<Utc>>,
    period: ChronoDuration,
    now: DateTime<Utc>,
    next_from_now: Option<DateTime<Utc>>,
) -> SlotAction {
    let Some(due) = next_due_at else {
        return SlotAction::Seed(next_from_now);
    };

    // Not yet late enough to mean anything.
    if now <= due + grace(period) {
        return SlotAction::Wait;
    }

    // Anacron's rule, and the one that keeps a box that was off for a week
    // from stampeding on boot: catch up at most one slot, and only while we
    // are still inside the period it belonged to. Past that, the moment has
    // gone — yesterday's examen is not written at noon today.
    let same_period = now - due < period;
    if catches_up(period) && same_period {
        SlotAction::CatchUp { slot: due }
    } else {
        SlotAction::Skip { slot: due }
    }
}

/// An enabled cron applet whose owed slot is far enough in the past to say so.
#[derive(Debug, Clone, serde::Serialize)]
pub struct OverdueApplet {
    pub id: String,
    pub name: String,
    pub next_due_at: DateTime<Utc>,
}

/// Applets that should have run and did not.
///
/// The grace here is the flat maximum rather than the per-schedule figure: it
/// is one query over the table, and being an hour late is the point at which a
/// person wants to know regardless of how often the thing normally runs. The
/// scheduler's own pass, which knows each period, is what actually advances
/// pointers; this is the read side.
pub async fn overdue(db: &PgPool) -> Result<Vec<OverdueApplet>> {
    let rows: Vec<(String, String, DateTime<Utc>)> = sqlx::query_as(
        r#"SELECT id, name, next_due_at
             FROM app_applets
            WHERE enabled = TRUE
              AND archived_at IS NULL
              AND next_due_at IS NOT NULL
              AND next_due_at < now() - make_interval(mins => $1)
            ORDER BY next_due_at"#,
    )
    .bind(GRACE_MAX.num_minutes() as i32)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, name, next_due_at)| OverdueApplet {
            id,
            name,
            next_due_at,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn parses_the_schedulers_own_six_field_dialect() {
        assert!(parse("0 0 7 * * *").is_some(), "daily 7am");
        assert!(parse("0 */15 * * * *").is_some(), "every 15 minutes");
        // Five fields are the other common dialect and are NOT what the
        // scheduler registers, so predicting from them would drift.
        assert!(parse("0 7 * * *").is_none(), "5-field rejected");
        assert!(parse("nonsense").is_none());
    }

    #[test]
    fn period_reads_the_gap_between_occurrences() {
        let daily = parse("0 0 7 * * *").unwrap();
        let p = period(&daily, Tz::UTC, utc("2026-08-05T00:00:00Z")).unwrap();
        assert_eq!(p, ChronoDuration::hours(24));

        let quarter = parse("0 */15 * * * *").unwrap();
        let p = period(&quarter, Tz::UTC, utc("2026-08-05T00:00:00Z")).unwrap();
        assert_eq!(p, ChronoDuration::minutes(15));
    }

    #[test]
    fn a_slot_is_named_by_its_occurrence_not_the_wake_moment() {
        let daily = parse("0 0 7 * * *").unwrap();
        // The job wakes a few hundred ms after 07:00; the slot is still 07:00.
        let woke = utc("2026-08-05T07:00:00.412Z");
        assert_eq!(
            previous_at_or_before(&daily, Tz::UTC, woke).unwrap(),
            utc("2026-08-05T07:00:00Z")
        );
    }

    #[test]
    fn schedules_are_local_not_utc() {
        let daily = parse("0 0 7 * * *").unwrap();
        let ny: Tz = "America/New_York".parse().unwrap();
        let next = next_after(&daily, ny, utc("2026-08-05T00:00:00Z")).unwrap();
        // 7am in New York on 2026-08-05 is 11:00 UTC (EDT, UTC-4).
        assert_eq!(next, utc("2026-08-05T11:00:00Z"));
    }

    #[test]
    fn only_infrequent_schedules_catch_up() {
        assert!(catches_up(ChronoDuration::hours(24)), "daily");
        assert!(catches_up(ChronoDuration::days(7)), "weekly");
        assert!(!catches_up(ChronoDuration::hours(1)), "hourly");
        assert!(!catches_up(ChronoDuration::minutes(15)), "quarter-hourly");
    }

    #[test]
    fn grace_stays_useful_at_both_ends() {
        // A 2-minute poll is not an alarm for being 4 minutes late.
        assert_eq!(grace(ChronoDuration::minutes(2)), GRACE_MIN);
        // A daily applet does not get to be 48 hours late unnoticed.
        assert_eq!(grace(ChronoDuration::hours(24)), GRACE_MAX);
        assert_eq!(grace(ChronoDuration::minutes(15)), ChronoDuration::minutes(30));
    }

    #[test]
    fn an_unseen_schedule_is_seeded_and_never_catches_up() {
        let next = Some(utc("2026-08-06T07:00:00Z"));
        assert_eq!(
            decide(None, ChronoDuration::hours(24), utc("2026-08-05T09:00:00Z"), next),
            SlotAction::Seed(next),
            "a newly scheduled applet has missed nothing"
        );
    }

    #[test]
    fn a_slot_inside_its_grace_is_not_late() {
        let due = utc("2026-08-05T07:00:00Z");
        // Daily → one hour of grace; half an hour past is still in flight.
        assert_eq!(
            decide(Some(due), ChronoDuration::hours(24), utc("2026-08-05T07:30:00Z"), None),
            SlotAction::Wait
        );
    }

    #[test]
    fn a_box_asleep_at_seven_catches_up_when_it_wakes() {
        let due = utc("2026-08-05T07:00:00Z");
        assert_eq!(
            decide(Some(due), ChronoDuration::hours(24), utc("2026-08-05T09:30:00Z"), None),
            SlotAction::CatchUp { slot: due },
        );
    }

    #[test]
    fn a_box_off_for_a_week_does_not_stampede() {
        let due = utc("2026-08-05T07:00:00Z");
        // Eight days later: outside the period the slot belonged to. One slot
        // is advanced past per pass, never seven runs at once.
        assert_eq!(
            decide(Some(due), ChronoDuration::hours(24), utc("2026-08-13T09:00:00Z"), None),
            SlotAction::Skip { slot: due },
        );
    }

    #[test]
    fn a_frequent_schedule_never_catches_up_it_just_advances() {
        let due = utc("2026-08-05T07:00:00Z");
        assert_eq!(
            decide(Some(due), ChronoDuration::minutes(15), utc("2026-08-05T08:00:00Z"), None),
            SlotAction::Skip { slot: due },
            "the next tick covers it; a backfill burst would not"
        );
    }

    #[test]
    fn dst_does_not_turn_a_daily_schedule_into_a_catch_up_refusal() {
        // US DST ends 2026-11-01: the local day is 25 hours, and the spring
        // transition makes one 23. Both must still read as "daily".
        let daily = parse("0 0 7 * * *").unwrap();
        let ny: Tz = "America/New_York".parse().unwrap();
        let spring = ny.with_ymd_and_hms(2026, 3, 7, 12, 0, 0).unwrap().with_timezone(&Utc);
        let p = period(&daily, ny, spring).unwrap();
        assert!(catches_up(p), "a 23-hour DST day is still daily (got {p})");
    }
}

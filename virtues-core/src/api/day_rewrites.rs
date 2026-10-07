//! Where each day's Rewrite this page stands, for the page that asked.
//!
//! A rewrite is three paid model calls and can take minutes, so it runs in a
//! task of its own and the day page asks here how it is going. The day's
//! advisory lock (`day_summary::try_lock_day`) is what keeps two writers off
//! one day; this map only reports what happened.
//!
//! In memory, one entry per day rewritten since the server started, kept
//! after the rewrite ends so the page can read how it ended. A restart
//! forgets them all, and a rewrite the restart interrupted is not resumed:
//! resuming would pay for the model calls again, so the owner decides.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use chrono::{DateTime, NaiveDate, Utc};

use crate::api::day_summary::RewriteOutcome;

/// One day's latest rewrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RewriteStatus {
    Running {
        started_at: DateTime<Utc>,
    },
    Done {
        started_at: DateTime<Utc>,
        finished_at: DateTime<Utc>,
        /// The version holding the page as it was before.
        before_version: i64,
        /// The version the rewrite cut, when cutting it worked.
        after_version: Option<i64>,
    },
    Failed {
        started_at: DateTime<Utc>,
        finished_at: DateTime<Utc>,
        /// A `RewriteOutcome::code`, or `interrupted` for a task that ended
        /// without finishing.
        code: &'static str,
        message: Option<String>,
    },
}

impl RewriteStatus {
    pub fn state(&self) -> &'static str {
        match self {
            Self::Running { .. } => "running",
            Self::Done { .. } => "done",
            Self::Failed { .. } => "failed",
        }
    }

    pub fn started_at(&self) -> DateTime<Utc> {
        match self {
            Self::Running { started_at }
            | Self::Done { started_at, .. }
            | Self::Failed { started_at, .. } => *started_at,
        }
    }
}

/// Every day's latest rewrite, by date.
#[derive(Clone, Default)]
pub struct DayRewrites {
    inner: Arc<Mutex<HashMap<NaiveDate, RewriteStatus>>>,
}

impl DayRewrites {
    pub fn new() -> Self {
        Self::default()
    }

    fn map(&self) -> MutexGuard<'_, HashMap<NaiveDate, RewriteStatus>> {
        // Every critical section is one insert or one lookup, so a panic
        // elsewhere cannot leave the map half-written.
        self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Mark `date` running, or `None` when it already is. The check and the
    /// mark are one step under one lock, so two callers cannot both begin.
    /// A finished rewrite's entry is replaced.
    pub fn try_begin(&self, date: NaiveDate) -> Option<RewriteGuard> {
        let mut map = self.map();
        if matches!(map.get(&date), Some(RewriteStatus::Running { .. })) {
            return None;
        }
        let started_at = Utc::now();
        map.insert(date, RewriteStatus::Running { started_at });
        Some(RewriteGuard {
            rewrites: self.clone(),
            date,
            started_at,
            finished: false,
        })
    }

    /// The day's latest rewrite, or `None` when it has had none since the
    /// server started.
    pub fn status(&self, date: NaiveDate) -> Option<RewriteStatus> {
        self.map().get(&date).cloned()
    }

    /// The refusal for a request that found `date`'s lock taken.
    /// `rewrite_in_progress` only when this map says a rewrite of the day is
    /// running, since the page then watches it here. Any other holder (the
    /// nightly narration, the CLI) is `busy`: this map knows nothing of it,
    /// so a page watching here would read the day's last rewrite, or none,
    /// as the outcome of a request that never started.
    pub fn locked_out(&self, date: NaiveDate) -> &'static str {
        match self.status(date) {
            Some(RewriteStatus::Running { .. }) => "rewrite_in_progress",
            _ => "busy",
        }
    }
}

/// A running rewrite's entry. Finishing it records how the rewrite ended;
/// dropping it unfinished (a panic, a cancelled task) records `interrupted`,
/// so no entry says running for a rewrite that is not.
pub struct RewriteGuard {
    rewrites: DayRewrites,
    date: NaiveDate,
    started_at: DateTime<Utc>,
    finished: bool,
}

impl RewriteGuard {
    pub fn started_at(&self) -> DateTime<Utc> {
        self.started_at
    }

    pub fn finish(mut self, outcome: &RewriteOutcome) {
        let finished_at = Utc::now();
        let status = match outcome {
            RewriteOutcome::Rewritten {
                before_version,
                after_version,
            } => RewriteStatus::Done {
                started_at: self.started_at,
                finished_at,
                before_version: *before_version,
                after_version: *after_version,
            },
            other => RewriteStatus::Failed {
                started_at: self.started_at,
                finished_at,
                code: other.code().unwrap_or("failed"),
                message: match other {
                    RewriteOutcome::Failed(message) => Some(message.clone()),
                    _ => None,
                },
            },
        };
        self.rewrites.map().insert(self.date, status);
        self.finished = true;
    }
}

impl Drop for RewriteGuard {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        self.rewrites.map().insert(
            self.date,
            RewriteStatus::Failed {
                started_at: self.started_at,
                finished_at: Utc::now(),
                code: "interrupted",
                message: None,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 14).unwrap()
    }

    #[test]
    fn a_second_begin_is_refused_while_one_runs() {
        let rewrites = DayRewrites::new();
        let guard = rewrites.try_begin(day()).expect("the first begins");
        assert!(rewrites.try_begin(day()).is_none(), "one rewrite of a day at a time");
        assert_eq!(rewrites.status(day()).unwrap().state(), "running");
        // Another day is its own.
        assert!(rewrites.try_begin(day().succ_opt().unwrap()).is_some());
        guard.finish(&RewriteOutcome::NotEnough);
        assert!(rewrites.try_begin(day()).is_some(), "a finished rewrite can be asked for again");
    }

    #[test]
    fn a_rewrite_that_landed_is_done_with_its_versions() {
        let rewrites = DayRewrites::new();
        let guard = rewrites.try_begin(day()).unwrap();
        guard.finish(&RewriteOutcome::Rewritten {
            before_version: 4,
            after_version: Some(5),
        });
        match rewrites.status(day()).unwrap() {
            RewriteStatus::Done {
                before_version,
                after_version,
                finished_at,
                started_at,
            } => {
                assert_eq!((before_version, after_version), (4, Some(5)));
                assert!(finished_at >= started_at);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_refusal_is_recorded_with_its_code() {
        let rewrites = DayRewrites::new();
        rewrites
            .try_begin(day())
            .unwrap()
            .finish(&RewriteOutcome::EditedWhileWriting);
        match rewrites.status(day()).unwrap() {
            RewriteStatus::Failed { code, message, .. } => {
                assert_eq!(code, "edited_while_writing");
                assert_eq!(message, None);
            }
            other => panic!("{other:?}"),
        }

        rewrites
            .try_begin(day())
            .unwrap()
            .finish(&RewriteOutcome::Failed("scout came back empty".into()));
        match rewrites.status(day()).unwrap() {
            RewriteStatus::Failed { code, message, .. } => {
                assert_eq!(code, "failed");
                assert_eq!(message.as_deref(), Some("scout came back empty"));
            }
            other => panic!("{other:?}"),
        }
    }

    /// A day locked by its running rewrite is one the page can watch; a day
    /// locked by anything else is busy, whatever the map last recorded.
    #[test]
    fn a_locked_day_is_in_progress_only_while_its_rewrite_runs() {
        let rewrites = DayRewrites::new();
        assert_eq!(rewrites.locked_out(day()), "busy", "no rewrite since the server started");

        let guard = rewrites.try_begin(day()).unwrap();
        assert_eq!(rewrites.locked_out(day()), "rewrite_in_progress");

        guard.finish(&RewriteOutcome::Rewritten {
            before_version: 4,
            after_version: Some(5),
        });
        assert_eq!(rewrites.locked_out(day()), "busy", "an earlier rewrite is not this one");

        rewrites.try_begin(day()).unwrap().finish(&RewriteOutcome::NotEnough);
        assert_eq!(rewrites.locked_out(day()), "busy");
    }

    #[test]
    fn a_rewrite_dropped_unfinished_reads_interrupted() {
        let rewrites = DayRewrites::new();
        drop(rewrites.try_begin(day()).unwrap());
        match rewrites.status(day()).unwrap() {
            RewriteStatus::Failed { code, .. } => assert_eq!(code, "interrupted"),
            other => panic!("{other:?}"),
        }
        assert!(rewrites.try_begin(day()).is_some(), "and it does not hold the day");
    }

    #[test]
    fn a_task_that_panics_does_not_leave_the_day_running() {
        let rewrites = DayRewrites::new();
        let guard = rewrites.try_begin(day()).unwrap();
        let joined = std::thread::spawn(move || {
            let _guard = guard;
            panic!("the rewrite task fell over");
        })
        .join();
        assert!(joined.is_err());
        assert_eq!(rewrites.status(day()).unwrap().state(), "failed");
    }
}

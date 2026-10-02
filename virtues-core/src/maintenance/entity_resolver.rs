//! Periodic entity resolution.
//!
//! Raw ingest writes primitives to the `data_*` lake (location points,
//! financial transactions, calendar events). Turning those into ontology
//! surfaces — `data_location_visit` + `wiki_places`, merchant `wiki_orgs`,
//! attendee `wiki_people`, all linked via `wiki_refs` — is the job of
//! `entity_resolution::resolve_entities`. Historically that only ran from the
//! `virtues resolve-entities` CLI, so on a normal box it NEVER ran: location
//! never clustered into visits, merchants/people never resolved, and the day
//! page / timeline had nothing to show even though the lake was filling up.
//!
//! This task closes that gap the same way `sweeper` and `pair_rotator` do — a
//! single tokio interval loop owned by `server::run`. It re-resolves a rolling
//! lookback window so the in-progress day stays current; resolution is
//! idempotent (a re-clustered stay merges into its stored visit by time and
//! distance, entity refs dedup on a unique key, merchant
//! lookups skip already-linked rows), so overlapping runs are safe.

use std::sync::Arc;
use std::time::Duration;

use tokio::time::{interval, MissedTickBehavior};

use crate::database::Database;
use crate::entity_resolution::{resolve_entities, TimeWindow};

/// How often to resolve. Matches the original "~30-minute cron" intent closely
/// enough while keeping the in-progress day fresh on the timeline.
const TICK: Duration = Duration::from_secs(900); // 15 minutes

/// Rolling window each run re-resolves. Wide enough to cover the whole local
/// day (so "today" fills in) plus slack for late-arriving samples; resolution
/// is idempotent so re-processing the overlap is cheap and safe.
const LOOKBACK_HOURS: i64 = 30;

/// How often the box checks its own stays against the fixes behind them.
const COVERAGE_EVERY: Duration = Duration::from_secs(24 * 3600);

/// Warn when stays account for under half the week's tracked time.
///
/// The fixes keep arriving when the stay detector breaks, so no freshness
/// check sees it; the stays just thin out. A healthy week sits above 80%. A
/// week with under a day of tracked time says nothing either way.
async fn log_stay_coverage(db: &Database) {
    let since = chrono::Utc::now() - chrono::Duration::days(7);
    match crate::entity_resolution::places::stay_coverage(db.pool(), since).await {
        Ok(c) if c.tracked_hours >= 24.0 && c.stay_hours < c.tracked_hours / 2.0 => {
            tracing::warn!(
                stay_hours = c.stay_hours,
                tracked_hours = c.tracked_hours,
                "stays cover under half of the week's tracked location time"
            );
        }
        Ok(c) => tracing::debug!(stay_hours = c.stay_hours, tracked_hours = c.tracked_hours, "stay coverage"),
        Err(e) => tracing::warn!(error = %e, "stay coverage check failed"),
    }
}

/// Spawn the entity resolver as a background tokio task. Logs resolution counts
/// when work was done; errors are logged and the loop continues — a transient
/// DB error must not take the daemon down.
pub fn spawn(db: Arc<Database>) {
    tokio::spawn(async move {
        // Before the first pass, and in this task so it never races the
        // writer. A no-op unless visits were written by an older detector.
        if let Err(e) = crate::entity_resolution::places::rebuild_visits(&db).await {
            tracing::warn!(error = %e, "visit rebuild failed (will retry next start)");
        }

        // `Skip` so a slow resolution pass (large backlog) doesn't queue
        // catch-up ticks.
        let mut ticker = interval(TICK);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        // First tick fires immediately — resolve once at startup so a box that
        // was offline catches up without waiting a full interval.
        let mut coverage_checked: Option<std::time::Instant> = None;
        loop {
            ticker.tick().await;
            if coverage_checked.is_none_or(|t| t.elapsed() >= COVERAGE_EVERY) {
                coverage_checked = Some(std::time::Instant::now());
                log_stay_coverage(&db).await;
            }
            let window = TimeWindow::from_lookback_hours(LOOKBACK_HOURS);
            match resolve_entities(&db, window).await {
                Ok(stats) => {
                    if stats.places_resolved > 0 || stats.people_resolved > 0 {
                        tracing::info!(
                            places_resolved = stats.places_resolved,
                            people_resolved = stats.people_resolved,
                            duration_ms = stats.duration_ms,
                            "entity resolution pass complete"
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "entity resolution pass failed (will retry next tick)");
                }
            }
        }
    });
}

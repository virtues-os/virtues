//! Cron scheduler for app_applets.
//!
//! Reads every enabled action with a cron schedule from `app_applets` and
//! registers a job with `tokio-cron-scheduler`. Each firing calls
//! [`crate::applet_runner::run_applet`] with `trigger = "cron"`; the unified
//! runner handles triggers validation, condition evaluation, concurrency, and
//! dispatch to subprocess / LLM agent.
//!
//! ## Cron Expression Format
//!
//! 6-field (tokio-cron-scheduler): `sec min hour day month day_of_week`
//!
//! - `0 0 */6 * * *`  every 6 hours
//! - `0 */15 * * * *` every 15 minutes
//! - `0 0 0 * * *`    daily at midnight (the box's local time — see below)
//! - `0 0 9 * * 1`    every Monday at 9:00 AM
//!
//! ## Timezone
//!
//! Schedules are interpreted in the box owner's local timezone (from the
//! profile), not UTC — so "9am daily" means 9am where the user is. This is a
//! single-tenant appliance, so there is one timezone per box. If no timezone is
//! set we fall back to UTC. (Note: the offset is captured when jobs register, so
//! a DST change is picked up on the next restart/reschedule.)

pub mod applets;
pub mod slots;

use chrono_tz::Tz;
use sqlx::PgPool;
use std::collections::HashMap;
use std::time::Duration;
use tokio_cron_scheduler::{Job, JobScheduler};
use uuid::Uuid;

use crate::applet_runner::RunnerDeps;
use crate::error::{Error, Result};
use crate::server::yjs::YjsState;
use crate::types::Timestamp;

/// How often the registered job set is re-derived from `app_applets`.
///
/// Cron granularity is one minute, so a minute of latency between a row
/// appearing and its job existing costs nothing in practice.
const REFRESH_INTERVAL: Duration = Duration::from_secs(60);

pub struct Scheduler {
    db: PgPool,
    yjs_state: YjsState,
    scheduler: JobScheduler,
    /// `applet_id -> (cron_expr, job id)` for everything currently registered.
    /// The cron expression is kept so an *edited* schedule reads as a change
    /// rather than a no-op — otherwise a user retiming an applet in the UI
    /// would keep firing on the old schedule until the next restart.
    registered: HashMap<String, (String, Uuid)>,
}

impl Scheduler {
    pub async fn new(db: PgPool, yjs_state: YjsState) -> Result<Self> {
        let scheduler = JobScheduler::new()
            .await
            .map_err(|e| Error::Other(format!("Failed to create scheduler: {e}")))?;

        Ok(Self {
            db,
            yjs_state,
            scheduler,
            registered: HashMap::new(),
        })
    }

    /// Re-derive the registered job set from the database: add rows that have
    /// appeared, drop rows that are gone or disabled, and re-register any whose
    /// cron expression changed. Returns the number of jobs added.
    ///
    /// Called once at startup and then on a timer. It used to be a one-shot at
    /// boot, which meant a source connected while the box was running had its
    /// applet rows created by reconcile and then *never scheduled* — the row
    /// looked healthy in the DB and simply never fired, with no error anywhere.
    /// Deriving the set from the DB (rather than being notified of changes)
    /// keeps this self-healing: no write path has to remember to tell us.
    pub async fn sync_jobs(&mut self) -> Result<usize> {
        // Face-only applets (no command, no agent) never run server-side —
        // exclude from cron scheduling so they don't tick into a no-op skip
        // every minute. Derived from field presence, not the legacy
        // `runtime` taxonomy.
        let rows: Vec<(String, String, String, Option<chrono::DateTime<chrono::Utc>>)> =
            sqlx::query_as(
                r#"SELECT id, name, cron_schedule, next_due_at
               FROM app_applets
               WHERE enabled = TRUE
                 AND archived_at IS NULL
                 AND cron_schedule IS NOT NULL
                 AND triggers @> '["cron"]'::jsonb
                 AND (command IS NOT NULL OR (agent IS NOT NULL AND btrim(agent) <> ''))"#,
            )
            .fetch_all(&self.db)
            .await?;

        let mut desired: HashMap<String, (String, String, Option<chrono::DateTime<chrono::Utc>>)> =
            rows.into_iter()
                .map(|(id, name, cron, due)| (id, (name, cron, due)))
                .collect();

        // Drop anything that vanished, was disabled, or was retimed. Collect
        // first so we aren't mutating `registered` while reading it.
        let stale: Vec<String> = self
            .registered
            .iter()
            .filter(|(id, (cron, _))| {
                desired
                    .get(*id)
                    .map(|(_, want, _)| want != cron)
                    .unwrap_or(true)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for applet_id in stale {
            if let Some((_, job_id)) = self.registered.remove(&applet_id) {
                if let Err(e) = self.scheduler.remove(&job_id).await {
                    tracing::warn!(applet_id, error = %e, "failed to unregister cron job");
                } else {
                    tracing::info!(applet_id, "unregistered cron action");
                }
            }
            // The owed slot belonged to the old schedule. Clearing it makes
            // the pass below reseed from the new one — otherwise retiming an
            // applet from hourly to daily leaves it reading as an hour overdue
            // forever, and could fire a catch-up for a slot that no longer
            // exists on its schedule.
            //
            // Cleared in `desired` as well as in the row: `desired` was read
            // before this loop ran, so it still holds the old schedule's slot,
            // and the reconcile pass below reads from it. Writing only the row
            // leaves the pass looking at a value that is already gone.
            if let Err(e) = slots::seed_next_due(&self.db, &applet_id, None).await {
                tracing::warn!(applet_id, error = %e, "failed to clear stale slot pointer");
            }
            if let Some(entry) = desired.get_mut(&applet_id) {
                entry.2 = None;
            }
        }

        // Single-tenant box → one timezone for every schedule. Resolved per
        // sync so a profile timezone change is picked up too.
        let tz = resolve_schedule_tz(&self.db).await;
        let mut added = 0usize;

        for (applet_id, (name, cron_expr, next_due_at)) in desired {
            // Slot bookkeeping runs on EVERY pass, not just for newly
            // registered jobs — a missed slot is precisely the case where the
            // job was already registered and simply did not fire (box asleep,
            // process down, machine off).
            self.reconcile_slot(&applet_id, &cron_expr, next_due_at, tz)
                .await;

            if self.registered.contains_key(&applet_id) {
                continue;
            }
            let db = self.db.clone();
            let yjs = self.yjs_state.clone();
            let action_id_for_job = applet_id.clone();
            let name_for_log = name.clone();
            let cron_for_log = cron_expr.clone();
            let cron_for_job = cron_expr.clone();

            let job = Job::new_async_tz(cron_expr.as_str(), tz, move |_uuid, _lock| {
                let deps = RunnerDeps {
                    db: db.clone(),
                    yjs: yjs.clone(),
                };
                let applet_id = action_id_for_job.clone();
                let cron_expr = cron_for_job.clone();
                Box::pin(async move {
                    // Advance the pointer BEFORE running. A long run (an
                    // embedding drain is hours) would otherwise leave the slot
                    // unadvanced for its whole duration, and every sync pass
                    // in that window would read the applet as overdue.
                    stamp_fired_slot(&deps.db, &applet_id, &cron_expr, tz).await;

                    if let Err(e) =
                        crate::applet_runner::run_applet(&deps, &applet_id, "cron", None).await
                    {
                        tracing::error!(applet_id, error = %e, "scheduled cron run failed");
                    }
                })
            })
            .map_err(|e| {
                Error::Other(format!(
                    "Failed to create cron job '{}' ({}): {}. Expected 6-field format \
                     (sec min hour day month dow). Example: '0 0 */6 * * *'",
                    name_for_log, cron_for_log, e
                ))
            });

            // One malformed cron expression must not abort the whole sync and
            // strand every other applet unscheduled — log it and keep going.
            let job = match job {
                Ok(j) => j,
                Err(e) => {
                    tracing::error!(applet_id, error = %e, "skipping unschedulable cron action");
                    continue;
                }
            };

            match self.scheduler.add(job).await {
                Ok(job_id) => {
                    self.registered.insert(applet_id.clone(), (cron_expr, job_id));
                    added += 1;
                    tracing::debug!(applet_id = %applet_id, name = %name, "registered cron action");
                }
                Err(e) => {
                    tracing::error!(applet_id, error = %e, "failed to register cron job");
                }
            }
        }

        Ok(added)
    }

    /// Bring one applet's slot pointer up to date, and fire a catch-up run if
    /// the schedule is the kind that owes one.
    ///
    /// This is the half `tokio-cron-scheduler` structurally cannot do: it only
    /// knows whether a moment has arrived while it was watching. A slot that
    /// passed while this process was not running leaves no trace in it at all.
    async fn reconcile_slot(
        &self,
        applet_id: &str,
        cron_expr: &str,
        next_due_at: Option<chrono::DateTime<chrono::Utc>>,
        tz: Tz,
    ) {
        let Some(cron) = slots::parse(cron_expr) else {
            // Unschedulable — already logged where the job is registered.
            return;
        };
        let now = chrono::Utc::now();
        let Some(period) = slots::period(&cron, tz, now) else {
            return;
        };
        // Whatever we do with the missed slot, what we owe next is the first
        // occurrence after *now* — not after the missed slot. Advancing one
        // slot per pass would leave a 15-minute applet that fell two hours
        // behind needing eight sync passes to become current again, reading as
        // overdue the whole way.
        let next_from_now = slots::next_after(&cron, tz, now);

        match slots::decide(next_due_at, period, now, next_from_now) {
            slots::SlotAction::Wait => {}
            slots::SlotAction::Seed(next) => {
                if let Err(e) = slots::seed_next_due(&self.db, applet_id, next).await {
                    tracing::warn!(applet_id, error = %e, "failed to seed slot pointer");
                }
            }
            slots::SlotAction::Skip { slot } => {
                tracing::info!(
                    applet_id,
                    missed_slot = %slot,
                    "slot missed; not catching up on this cadence"
                );
                if let Err(e) =
                    slots::mark_slot_handled(&self.db, applet_id, slot, next_from_now).await
                {
                    tracing::warn!(applet_id, error = %e, "failed to advance slot pointer");
                }
            }
            slots::SlotAction::CatchUp { slot } => {
                // Advance first, then run. If the catch-up run itself fails,
                // the slot still counts as handled — anacron semantics are
                // "at most once", and a failing applet must not retry on
                // every sync pass for the rest of the period.
                if let Err(e) =
                    slots::mark_slot_handled(&self.db, applet_id, slot, next_from_now).await
                {
                    tracing::warn!(applet_id, error = %e, "failed to advance slot pointer");
                    return;
                }
                tracing::info!(applet_id, missed_slot = %slot, "catching up a missed slot");

                let deps = RunnerDeps {
                    db: self.db.clone(),
                    yjs: self.yjs_state.clone(),
                };
                let id = applet_id.to_string();
                // Detached: a catch-up must not hold up the rest of the sync
                // pass, and on a box that was off for a while there may be
                // several. The runner's own singleton gate keeps them honest.
                tokio::spawn(async move {
                    if let Err(e) = crate::applet_runner::run_applet(&deps, &id, "cron", None).await
                    {
                        tracing::error!(applet_id = %id, error = %e, "catch-up run failed");
                    }
                });
            }
        }
    }

    /// Keep the job set current for the life of the process.
    ///
    /// Also the owner of the `Scheduler` value: `tokio-cron-scheduler` runs its
    /// jobs on their own tasks, but the `JobScheduler` must stay in scope, so
    /// this never returns.
    pub async fn run_refresh_loop(mut self) {
        loop {
            tokio::time::sleep(REFRESH_INTERVAL).await;
            match self.sync_jobs().await {
                Ok(0) => {}
                Ok(n) => tracing::info!("scheduler picked up {n} new cron action(s)"),
                Err(e) => tracing::warn!(error = %e, "scheduler refresh failed; will retry"),
            }
        }
    }

    /// Start the underlying tokio-cron-scheduler.
    pub async fn start(&self) -> Result<()> {
        self.scheduler
            .start()
            .await
            .map_err(|e| Error::Other(format!("Failed to start scheduler: {e}")))?;

        tracing::info!("Scheduler started");
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        self.scheduler
            .shutdown()
            .await
            .map_err(|e| Error::Other(format!("Failed to stop scheduler: {e}")))?;

        tracing::info!("Scheduler stopped");
        Ok(())
    }

    /// Simple enumeration of cron-scheduled actions for display.
    pub async fn list_scheduled(&self) -> Result<Vec<ScheduledApplet>> {
        let rows = sqlx::query_as::<_, (String, String, String, Option<Timestamp>)>(
            r#"SELECT a.id, a.name, a.cron_schedule, r.started_at
               FROM app_applets a
               LEFT JOIN app_applet_runs r ON r.id = (
                   SELECT id FROM app_applet_runs
                   WHERE applet_id = a.id AND status = 'success'
                   ORDER BY started_at DESC LIMIT 1
               )
               WHERE a.enabled = TRUE
                 AND a.archived_at IS NULL
                 AND a.cron_schedule IS NOT NULL
                 AND a.triggers @> '["cron"]'::jsonb
               ORDER BY a.name"#,
        )
        .fetch_all(&self.db)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, name, cron_schedule, last_success_at)| ScheduledApplet {
                id,
                name,
                cron_schedule,
                last_success_at,
            })
            .collect())
    }
}

#[derive(Debug)]
pub struct ScheduledApplet {
    pub id: String,
    pub name: String,
    pub cron_schedule: String,
    pub last_success_at: Option<Timestamp>,
}

/// Record that the slot this firing belongs to has been handled.
///
/// The slot is the scheduled occurrence, not the wake moment: a job wakes some
/// milliseconds after its slot, and stamping the wake time would walk
/// `next_due_at` forward by that drift on every single firing. Falls back to
/// the wake moment only if the expression will not yield an occurrence, which
/// keeps a strange schedule from freezing the pointer entirely.
async fn stamp_fired_slot(db: &PgPool, applet_id: &str, cron_expr: &str, tz: Tz) {
    let now = chrono::Utc::now();
    let Some(cron) = slots::parse(cron_expr) else {
        return;
    };
    let slot = slots::previous_at_or_before(&cron, tz, now).unwrap_or(now);
    let next = slots::next_after(&cron, tz, now);
    if let Err(e) = slots::mark_slot_handled(db, applet_id, slot, next).await {
        tracing::warn!(applet_id, error = %e, "failed to stamp fired slot");
    }
}

/// Resolve the timezone cron schedules are interpreted in: the box owner's
/// profile timezone, falling back to UTC if unset or unrecognized.
async fn resolve_schedule_tz(db: &PgPool) -> Tz {
    match crate::api::profile::get_timezone(db).await {
        Ok(Some(name)) => match name.parse::<Tz>() {
            Ok(tz) => tz,
            Err(_) => {
                tracing::warn!(timezone = %name, "unrecognized profile timezone; scheduling in UTC");
                Tz::UTC
            }
        },
        Ok(None) => Tz::UTC,
        Err(e) => {
            tracing::warn!(error = %e, "failed to read profile timezone; scheduling in UTC");
            Tz::UTC
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Requires a live Postgres: `#[sqlx::test]` provisions a scratch DB and
    // applies `core/migrations` automatically. Set DATABASE_URL when running.
    #[sqlx::test]
    async fn test_scheduler_creation(pool: PgPool) {
        let yjs_state = YjsState::new(pool.clone());
        let result = Scheduler::new(pool, yjs_state).await;
        assert!(result.is_ok());
    }

    async fn insert_applet(pool: &PgPool, id: &str, cron: &str) {
        sqlx::query(
            "INSERT INTO app_applets (id, name, owner, cron_schedule, command)
             VALUES ($1, $1, 'system', $2, 'echo')",
        )
        .bind(id)
        .bind(cron)
        .execute(pool)
        .await
        .expect("insert applet");
    }

    async fn scheduler(pool: &PgPool) -> Scheduler {
        let yjs = YjsState::new(pool.clone());
        Scheduler::new(pool.clone(), yjs).await.expect("scheduler")
    }

    /// The regression this whole refresh loop exists for: a source connected
    /// while the box is running has its applet rows written by reconcile, and
    /// the scheduler must pick them up without a restart. Before this, jobs
    /// were registered once at boot and a later row simply never fired.
    #[sqlx::test]
    async fn picks_up_applets_created_after_the_first_sync(pool: PgPool) {
        let mut sched = scheduler(&pool).await;
        assert_eq!(sched.sync_jobs().await.unwrap(), 0, "empty catalog");

        insert_applet(&pool, "applet_plaid_transactions_sync_cred_x", "0 */30 * * * *").await;

        assert_eq!(sched.sync_jobs().await.unwrap(), 1, "new row scheduled");
        assert_eq!(sched.sync_jobs().await.unwrap(), 0, "already registered");
    }

    /// Retiming an applet in the UI has to take effect, so a changed cron must
    /// read as a change and not as "already registered".
    #[sqlx::test]
    async fn reregisters_an_applet_whose_cron_changed(pool: PgPool) {
        let mut sched = scheduler(&pool).await;
        insert_applet(&pool, "applet_a", "0 0 * * * *").await;
        assert_eq!(sched.sync_jobs().await.unwrap(), 1);

        sqlx::query("UPDATE app_applets SET cron_schedule = '0 */5 * * * *' WHERE id = 'applet_a'")
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(sched.sync_jobs().await.unwrap(), 1, "retimed → re-registered");
        assert_eq!(
            sched.registered.get("applet_a").map(|(c, _)| c.as_str()),
            Some("0 */5 * * * *")
        );
    }

    type Utc0 = chrono::DateTime<chrono::Utc>;

    async fn pointer(pool: &PgPool, id: &str) -> (Option<Utc0>, Option<Utc0>) {
        sqlx::query_as("SELECT last_slot_at, next_due_at FROM app_applets WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .expect("pointer")
    }

    async fn set_due(pool: &PgPool, id: &str, due: chrono::DateTime<chrono::Utc>) {
        sqlx::query("UPDATE app_applets SET next_due_at = $2 WHERE id = $1")
            .bind(id)
            .bind(due)
            .execute(pool)
            .await
            .expect("set due");
    }

    /// Before this, nothing anywhere recorded when an applet was *supposed* to
    /// run — which is why a box that was off could not know it had missed
    /// anything, and why "expected but didn't run" was uncomputable.
    #[sqlx::test]
    async fn sync_seeds_the_slot_pointer(pool: PgPool) {
        let mut sched = scheduler(&pool).await;
        insert_applet(&pool, "applet_a", "0 0 7 * * *").await;
        sched.sync_jobs().await.unwrap();

        let (last, next) = pointer(&pool, "applet_a").await;
        assert!(last.is_none(), "nothing has been handled yet");
        let next = next.expect("next_due_at seeded");
        assert!(next > chrono::Utc::now(), "the owed slot is in the future");
    }

    /// A newly scheduled applet has missed nothing — it did not exist yet.
    /// Seeding must never look like a missed slot, or every applet would fire
    /// once the moment it was created.
    #[sqlx::test]
    async fn a_freshly_seeded_applet_does_not_catch_up(pool: PgPool) {
        let mut sched = scheduler(&pool).await;
        insert_applet(&pool, "applet_a", "0 0 7 * * *").await;
        sched.sync_jobs().await.unwrap();
        sched.sync_jobs().await.unwrap();

        let (last, _) = pointer(&pool, "applet_a").await;
        assert!(last.is_none(), "no slot was handled, so none was missed");
    }

    /// The box was asleep at 7am. On waking, the daily slot is owed and inside
    /// its period, so it is caught up once and the pointer advances.
    #[sqlx::test]
    async fn a_missed_daily_slot_is_caught_up_once(pool: PgPool) {
        let mut sched = scheduler(&pool).await;
        insert_applet(&pool, "applet_a", "0 0 7 * * *").await;
        sched.sync_jobs().await.unwrap();

        let missed = chrono::Utc::now() - chrono::Duration::hours(3);
        set_due(&pool, "applet_a", missed).await;
        sched.sync_jobs().await.unwrap();

        let (last, next) = pointer(&pool, "applet_a").await;
        assert_eq!(last, Some(missed), "the missed slot is the one handled");
        assert!(
            next.expect("advanced") > chrono::Utc::now(),
            "pointer moved past the missed slot rather than sticking on it"
        );
    }

    /// A 15-minute sync that missed a tick is served by the next tick, not by
    /// a backfill. The pointer still advances — otherwise it would read as
    /// overdue forever and nag from the needs-attention strip.
    #[sqlx::test]
    async fn a_missed_frequent_slot_advances_without_catching_up(pool: PgPool) {
        let mut sched = scheduler(&pool).await;
        insert_applet(&pool, "applet_a", "0 */15 * * * *").await;
        sched.sync_jobs().await.unwrap();

        let missed = chrono::Utc::now() - chrono::Duration::hours(2);
        set_due(&pool, "applet_a", missed).await;
        sched.sync_jobs().await.unwrap();

        let (last, next) = pointer(&pool, "applet_a").await;
        assert_eq!(last, Some(missed));
        assert!(next.expect("advanced") > chrono::Utc::now());
    }

    /// Retiming must not leave the old schedule's owed slot behind: it would
    /// read as permanently overdue, and could fire a catch-up for a slot the
    /// new schedule does not have.
    #[sqlx::test]
    async fn retiming_reseeds_the_pointer(pool: PgPool) {
        let mut sched = scheduler(&pool).await;
        insert_applet(&pool, "applet_a", "0 0 7 * * *").await;
        sched.sync_jobs().await.unwrap();
        let (_, before) = pointer(&pool, "applet_a").await;

        sqlx::query("UPDATE app_applets SET cron_schedule = '0 0 22 * * *' WHERE id = 'applet_a'")
            .execute(&pool)
            .await
            .unwrap();
        sched.sync_jobs().await.unwrap();

        let (_, after) = pointer(&pool, "applet_a").await;
        assert!(after.is_some(), "reseeded from the new schedule");
        assert_ne!(before, after, "the old schedule's slot did not survive");
    }

    /// A finished applet must not be scheduled. Archiving sets enabled = FALSE
    /// so this was covered in practice, but the query never said so — and
    /// `slots::overdue` DID filter archived rows, so the two disagreed about
    /// what counts as live.
    #[sqlx::test]
    async fn a_finished_applet_is_never_scheduled(pool: PgPool) {
        let mut sched = scheduler(&pool).await;
        insert_applet(&pool, "applet_a", "0 0 7 * * *").await;
        assert_eq!(sched.sync_jobs().await.unwrap(), 1);

        sqlx::query(
            "UPDATE app_applets SET archived_at = now(), enabled = FALSE WHERE id = 'applet_a'",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(sched.sync_jobs().await.unwrap(), 0);
        assert!(sched.registered.is_empty(), "unregistered once finished");

        // Enabled again but still archived — the belt-and-braces case, and the
        // one the enabled-only query would have re-scheduled.
        sqlx::query("UPDATE app_applets SET enabled = TRUE WHERE id = 'applet_a'")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(sched.sync_jobs().await.unwrap(), 0, "still finished");
    }

    /// Disabling or deleting an applet must unregister it, or a disconnected
    /// source would keep firing until the next restart.
    #[sqlx::test]
    async fn drops_disabled_and_deleted_applets(pool: PgPool) {
        let mut sched = scheduler(&pool).await;
        insert_applet(&pool, "applet_a", "0 0 * * * *").await;
        insert_applet(&pool, "applet_b", "0 0 * * * *").await;
        assert_eq!(sched.sync_jobs().await.unwrap(), 2);

        sqlx::query("UPDATE app_applets SET enabled = FALSE WHERE id = 'applet_a'")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM app_applets WHERE id = 'applet_b'")
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(sched.sync_jobs().await.unwrap(), 0, "nothing new to add");
        assert!(sched.registered.is_empty(), "both unregistered");
    }
}

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
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            r#"SELECT id, name, cron_schedule
               FROM app_applets
               WHERE enabled = TRUE
                 AND cron_schedule IS NOT NULL
                 AND triggers @> '["cron"]'::jsonb
                 AND (command IS NOT NULL OR (agent IS NOT NULL AND btrim(agent) <> ''))"#,
        )
        .fetch_all(&self.db)
        .await?;

        let desired: HashMap<String, (String, String)> = rows
            .into_iter()
            .map(|(id, name, cron)| (id, (name, cron)))
            .collect();

        // Drop anything that vanished, was disabled, or was retimed. Collect
        // first so we aren't mutating `registered` while reading it.
        let stale: Vec<String> = self
            .registered
            .iter()
            .filter(|(id, (cron, _))| {
                desired.get(*id).map(|(_, want)| want != cron).unwrap_or(true)
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
        }

        // Single-tenant box → one timezone for every schedule. Resolved per
        // sync so a profile timezone change is picked up too.
        let tz = resolve_schedule_tz(&self.db).await;
        let mut added = 0usize;

        for (applet_id, (name, cron_expr)) in desired {
            if self.registered.contains_key(&applet_id) {
                continue;
            }
            let db = self.db.clone();
            let yjs = self.yjs_state.clone();
            let action_id_for_job = applet_id.clone();
            let name_for_log = name.clone();
            let cron_for_log = cron_expr.clone();

            let job = Job::new_async_tz(cron_expr.as_str(), tz, move |_uuid, _lock| {
                let deps = RunnerDeps {
                    db: db.clone(),
                    yjs: yjs.clone(),
                };
                let applet_id = action_id_for_job.clone();
                Box::pin(async move {
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

        sqlx::query("UPDATE app_applets SET cron_schedule = '0 */5 * * * *' WHERE id = 'action_a'")
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(sched.sync_jobs().await.unwrap(), 1, "retimed → re-registered");
        assert_eq!(
            sched.registered.get("applet_a").map(|(c, _)| c.as_str()),
            Some("0 */5 * * * *")
        );
    }

    /// Disabling or deleting an applet must unregister it, or a disconnected
    /// source would keep firing until the next restart.
    #[sqlx::test]
    async fn drops_disabled_and_deleted_applets(pool: PgPool) {
        let mut sched = scheduler(&pool).await;
        insert_applet(&pool, "applet_a", "0 0 * * * *").await;
        insert_applet(&pool, "applet_b", "0 0 * * * *").await;
        assert_eq!(sched.sync_jobs().await.unwrap(), 2);

        sqlx::query("UPDATE app_applets SET enabled = FALSE WHERE id = 'action_a'")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM app_applets WHERE id = 'action_b'")
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(sched.sync_jobs().await.unwrap(), 0, "nothing new to add");
        assert!(sched.registered.is_empty(), "both unregistered");
    }
}

//! Cron scheduler for app_applets.
//!
//! Reads every enabled action with a cron schedule from `app_applets` and
//! registers a job with `tokio-cron-scheduler`. Each firing calls
//! [`crate::action_runner::run_action`] with `trigger = "cron"`; the unified
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

pub mod actions;

use chrono_tz::Tz;
use sqlx::PgPool;
use tokio_cron_scheduler::{Job, JobScheduler};

use crate::action_runner::RunnerDeps;
use crate::error::{Error, Result};
use crate::server::yjs::YjsState;
use crate::types::Timestamp;

pub struct Scheduler {
    db: PgPool,
    yjs_state: YjsState,
    scheduler: JobScheduler,
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
        })
    }

    /// Read all enabled cron-triggered actions and register them as jobs.
    ///
    /// Templates.toml reconciliation must have already run so the rows exist.
    pub async fn schedule_all(&self) -> Result<()> {
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

        if rows.is_empty() {
            tracing::info!("No cron actions to schedule");
            return Ok(());
        }

        // Single-tenant box → one timezone for every schedule. Resolve it once
        // so "9am daily" fires at 9am local rather than 9am UTC.
        let tz = resolve_schedule_tz(&self.db).await;
        tracing::info!("Scheduling {} cron actions in {}", rows.len(), tz);

        for (action_id, name, cron_expr) in rows {
            let db = self.db.clone();
            let yjs = self.yjs_state.clone();
            let action_id_for_job = action_id.clone();
            let name_for_log = name.clone();
            let cron_for_log = cron_expr.clone();

            let job = Job::new_async_tz(cron_expr.as_str(), tz, move |_uuid, _lock| {
                let deps = RunnerDeps {
                    db: db.clone(),
                    yjs: yjs.clone(),
                };
                let action_id = action_id_for_job.clone();
                Box::pin(async move {
                    if let Err(e) =
                        crate::action_runner::run_action(&deps, &action_id, "cron", None).await
                    {
                        tracing::error!(action_id, error = %e, "scheduled cron run failed");
                    }
                })
            })
            .map_err(|e| {
                Error::Other(format!(
                    "Failed to create cron job '{}' ({}): {}. Expected 6-field format \
                     (sec min hour day month dow). Example: '0 0 */6 * * *'",
                    name_for_log, cron_for_log, e
                ))
            })?;

            self.scheduler
                .add(job)
                .await
                .map_err(|e| Error::Other(format!("Failed to add cron job: {e}")))?;

            tracing::debug!(action_id = %action_id, name = %name, "registered cron action");
        }

        Ok(())
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
    pub async fn list_scheduled(&self) -> Result<Vec<ScheduledAction>> {
        let rows = sqlx::query_as::<_, (String, String, String, Option<Timestamp>)>(
            r#"SELECT a.id, a.name, a.cron_schedule, r.started_at
               FROM app_applets a
               LEFT JOIN app_applet_runs r ON r.id = (
                   SELECT id FROM app_applet_runs
                   WHERE action_id = a.id AND status = 'success'
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
            .map(|(id, name, cron_schedule, last_success_at)| ScheduledAction {
                id,
                name,
                cron_schedule,
                last_success_at,
            })
            .collect())
    }
}

#[derive(Debug)]
pub struct ScheduledAction {
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
}

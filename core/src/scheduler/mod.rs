//! Built-in task scheduler for periodic stream syncs
//!
//! This scheduler reads enabled streams from the `streams` table and
//! schedules them based on their `cron_schedule` field.
//!
//! ## Cron Expression Format
//!
//! The scheduler uses tokio-cron-scheduler which requires 6-field cron expressions:
//! ```text
//! sec   min   hour   day   month   day_of_week
//! *     *     *      *     *       *
//! ```
//!
//! ### Examples:
//! - `0 0 */6 * * *` - Every 6 hours
//! - `0 */15 * * * *` - Every 15 minutes
//! - `0 0 0 * * *` - Daily at midnight
//! - `0 0 9 * * 1` - Every Monday at 9:00 AM

pub mod actions;

use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_cron_scheduler::{Job, JobScheduler};

use crate::{
    error::{Error, Result},
    server::yjs::YjsState,
    storage::{stream_writer::StreamWriter, Storage},
    types::Timestamp,
};

/// Simplified scheduler using StreamFactory
pub struct Scheduler {
    db: SqlitePool,
    storage: Storage,
    drive_config: crate::api::DriveConfig,
    stream_writer: Arc<Mutex<StreamWriter>>,
    yjs_state: YjsState,
    scheduler: JobScheduler,
}

impl Scheduler {
    /// Create a new scheduler
    pub async fn new(
        db: SqlitePool,
        storage: Storage,
        stream_writer: Arc<Mutex<StreamWriter>>,
        yjs_state: YjsState,
    ) -> Result<Self> {
        let scheduler = JobScheduler::new()
            .await
            .map_err(|e| Error::Other(format!("Failed to create scheduler: {e}")))?;

        // Create drive config from storage
        let drive_config = crate::api::DriveConfig::new(std::sync::Arc::new(storage.clone()));

        Ok(Self {
            db,
            storage,
            drive_config,
            stream_writer,
            yjs_state,
            scheduler,
        })
    }

    /// Start the scheduler
    ///
    /// Loads all enabled streams with cron schedules from the database
    /// and creates jobs for each.
    ///
    /// Note: Only pull streams (Google, Notion) are scheduled. Push streams
    /// (Mac, iOS) are not scheduled since they're initiated by client devices.
    pub async fn start(&self) -> Result<()> {
        // Load enabled sync tasks from the unified app_actions table
        // (cron_schedule moved from elt_stream_connections to app_actions in migration 032)
        let streams = sqlx::query_as::<_, (String, String, String, String, Option<String>)>(
            r#"
            SELECT
                json_extract(t.config, '$.source_connection_id') as source_id,
                s.name as source_name,
                s.source,
                json_extract(t.config, '$.stream_name') as stream_name,
                t.cron_schedule
            FROM app_actions t
            JOIN elt_source_connections s
                ON s.id = json_extract(t.config, '$.source_connection_id')
            WHERE t.action_type = 'sync'
              AND t.enabled = 1
              AND t.cron_schedule IS NOT NULL
              AND s.is_active = true
              AND s.source NOT IN ('mac', 'ios')
            "#,
        )
        .fetch_all(&self.db)
        .await?;

        tracing::info!("Loading {} scheduled sync tasks", streams.len());

        // Schedule each stream
        for (source_id, source_name, provider, stream_name, cron_schedule) in streams {
            let cron = cron_schedule.expect("cron_schedule is NOT NULL per WHERE clause");

            let db = self.db.clone();
            let storage = self.storage.clone();
            let stream_writer = self.stream_writer.clone();

            tracing::debug!(
                "Scheduling {}/{} ({}) with cron: {}",
                provider,
                stream_name,
                source_name,
                cron
            );

            // Clone values for error message before they're moved into closure
            let provider_for_error = provider.clone();
            let stream_name_for_error = stream_name.clone();
            let source_name_for_error = source_name.clone();

            let job = Job::new_async(cron.as_str(), move |_uuid, _lock| {
                let db = db.clone();
                let storage = storage.clone();
                let stream_writer = stream_writer.clone();
                let source_id_str = source_id.clone();
                let stream_name = stream_name.clone();
                let source_name = source_name.clone();
                let stream_name_str = stream_name.clone();

                Box::pin(async move {
                    tracing::info!(
                        "Running scheduled sync: {} ({})",
                        stream_name_str,
                        source_name
                    );

                    // Use the task-based API with String source_id
                    match crate::api::actions::trigger_stream_sync(
                        &db,
                        &storage,
                        stream_writer,
                        source_id_str.clone(),
                        &stream_name,
                        None,
                    )
                    .await
                    {
                        Ok(response) => {
                            tracing::info!(
                                "Scheduled sync run created: {} - run_id={}, status={}",
                                stream_name_str,
                                response.run_id,
                                response.status
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                "Failed to create scheduled sync run for {}: {}",
                                stream_name_str,
                                e
                            );
                        }
                    }
                })
            })
            .map_err(|e| {
                Error::Other(format!(
                    "Failed to create job for {}/{} ({}): {}. \
                    Note: Cron expressions must be in 6-field format (sec min hour day month dow). \
                    Example: '0 0 */6 * * *' for every 6 hours.",
                    provider_for_error, stream_name_for_error, source_name_for_error, e
                ))
            })?;

            self.scheduler
                .add(job)
                .await
                .map_err(|e| Error::Other(format!("Failed to add job: {e}")))?;
        }

        // Start the scheduler
        self.scheduler
            .start()
            .await
            .map_err(|e| Error::Other(format!("Failed to start scheduler: {e}")))?;

        tracing::info!("Scheduler started successfully");
        Ok(())
    }

    /// Load and schedule all enabled action tasks from the database.
    ///
    /// Each action task has a cron_schedule and an optional activation_code.
    /// On trigger: create a ActionRun, run the activation gate, execute via run_action().
    pub async fn schedule_action_tasks(&self) -> Result<()> {
        let rows = sqlx::query(
            r#"
            SELECT *
            FROM app_actions
            WHERE action_type = 'agent'
              AND enabled = 1
              AND cron_schedule IS NOT NULL
            "#,
        )
        .fetch_all(&self.db)
        .await?;

        if rows.is_empty() {
            tracing::debug!("No action tasks to schedule");
            return Ok(());
        }

        let action_list: Vec<actions::Action> = rows
            .iter()
            .filter_map(|r| actions::action_from_row(r).ok())
            .collect();

        tracing::info!("Loading {} scheduled action tasks", action_list.len());

        for action in action_list {
            let cron_schedule = match &action.cron_schedule {
                Some(c) => c.clone(),
                None => continue,
            };

            let db = self.db.clone();
            let yjs = self.yjs_state.clone();
            let name_for_log = action.name.clone();
            let action_id_for_log = action.id.clone();
            let action_id_captured = action.id.clone();

            let job = Job::new_async(cron_schedule.as_str(), move |_uuid, _lock| {
                let db = db.clone();
                let yjs = yjs.clone();
                let action_id = action_id_captured.clone();

                Box::pin(async move {
                    // Re-fetch the action from DB to get fresh memory/instruction
                    let action = match actions::get_action(&db, &action_id).await {
                        Ok(a) => a,
                        Err(e) => {
                            tracing::error!(action_id, error = %e, "Failed to load action for cron run");
                            return;
                        }
                    };

                    // Skip if disabled (may have been disabled since scheduler started)
                    if !action.enabled {
                        return;
                    }

                    // Check for overlapping runs (unless parallel mode)
                    if action.concurrency_mode != "parallel" {
                        if let Ok(true) = actions::has_active_run(&db, &action.id).await {
                            tracing::debug!(action_id = action.id, "Skipping action: previous run still active");
                            return;
                        }
                    }

                    // Create ActionRun for audit trail
                    let run = match actions::create_run(&db, Some(&action.id), "cron").await {
                        Ok(r) => r,
                        Err(e) => {
                            tracing::error!(action_id = action.id, error = %e, "Failed to create action run");
                            return;
                        }
                    };

                    // Build dynamic context for system actions
                    let context = match action.id.as_str() {
                        "action_agent_dayline_hourly" => {
                            let now = chrono::Utc::now();
                            let window_start = now - chrono::Duration::hours(1);
                            let ctx = crate::dayline::context::build_hourly_context(
                                &db, window_start, now,
                            ).await;
                            if ctx.is_empty() { None } else { Some(ctx) }
                        }
                        "action_agent_dayline_eod" => {
                            match resolve_user_yesterday(&db).await {
                                Some(yesterday) => {
                                    Some(crate::dayline::context::build_eod_context(&db, yesterday).await)
                                }
                                None => None,
                            }
                        }
                        _ => None,
                    };

                    // Execute the action
                    let result = crate::agent::action_runner::run_action(
                        &db,
                        &yjs,
                        &action,
                        false, // force_run
                        None,  // broadcast
                        context.as_deref(),
                    )
                    .await;

                    // Complete the ActionRun
                    match result {
                        Ok(action_result) => {
                            let status = match &action_result.status {
                                crate::agent::action_runner::ActionRunStatus::Completed => "success",
                                crate::agent::action_runner::ActionRunStatus::ActivationSkipped => "skipped",
                                crate::agent::action_runner::ActionRunStatus::ActivationError(_) => "error",
                                crate::agent::action_runner::ActionRunStatus::Error(_) => "error",
                            };
                            let error_msg = match &action_result.status {
                                crate::agent::action_runner::ActionRunStatus::ActivationError(e) => Some(e.as_str()),
                                crate::agent::action_runner::ActionRunStatus::Error(e) => Some(e.as_str()),
                                _ => None,
                            };
                            let _ = actions::complete_run(
                                &db,
                                &run.id,
                                status,
                                action_result.steps as i64,
                                error_msg,
                            )
                            .await;
                        }
                        Err(e) => {
                            tracing::error!(action_id = action.id, error = %e, "Action execution failed");
                            let _ = actions::complete_run(
                                &db,
                                &run.id,
                                "error",
                                0,
                                Some(&e.to_string()),
                            )
                            .await;
                        }
                    }
                })
            })
            .map_err(|e| {
                Error::Other(format!(
                    "Failed to create action job '{}': {}. Cron must be 6-field format.",
                    name_for_log, e
                ))
            })?;

            self.scheduler
                .add(job)
                .await
                .map_err(|e| Error::Other(format!("Failed to add action job: {e}")))?;

            tracing::info!(action_id = action_id_for_log, name = name_for_log, cron = cron_schedule, "Scheduled action");
        }

        Ok(())
    }

    // =========================================================================
    // System action seeding
    // =========================================================================

    /// Ensure the dayline hourly action exists. Idempotent — skips if already created.
    pub async fn ensure_dayline_hourly_action(&self) -> Result<()> {
        let action_id = "action_agent_dayline_hourly";

        // Check if already exists
        if sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM app_actions WHERE id = ?)",
        )
        .bind(action_id)
        .fetch_one(&self.db)
        .await
        .unwrap_or(false)
        {
            return Ok(());
        }

        // Build activation code from registry — each ontology has its own timestamp column
        let active_ontologies: Vec<(&str, &str)> = virtues_registry::ontologies::registered_ontologies()
            .iter()
            .filter(|o| o.is_activation_signal)
            .map(|o| (o.table_name, o.timestamp_column))
            .collect();

        // Build Python dict of table_name -> timestamp_column
        let table_entries: Vec<String> = active_ontologies
            .iter()
            .map(|(table, col)| format!("    \"{}\": \"{}\"", table, col))
            .collect();

        let activation_code = format!(
            r#"import sqlite3, os
db = sqlite3.connect(os.environ.get('DB_PATH', 'virtues.db'))
tables = {{
{}
}}
for t, col in tables.items():
    try:
        count = db.execute(f"SELECT COUNT(*) FROM {{t}} WHERE {{col}} > datetime('now', '-1 hour')").fetchone()[0]
        if count > 0:
            print(f"active: {{t}} ({{count}} records)")
            break
    except:
        pass
else:
    print("")
"#,
            table_entries.join(",\n")
        );

        // Create the action (no chat needed — system actions are stateless)
        sqlx::query(
            r#"INSERT INTO app_actions (id, action_type, owner, name, instruction, cron_schedule, enabled, config, activation_code)
               VALUES (?, 'agent', 'system', ?, ?, '0 * * * * *', 1, '{}', ?)"#,
        )
        .bind(action_id)
        .bind("Dayline Hourly")
        .bind(HOURLY_ACTION_INSTRUCTION)
        .bind(&activation_code)
        .execute(&self.db)
        .await?;

        tracing::info!("Seeded dayline hourly action");
        Ok(())
    }

    /// Ensure the dayline end-of-day action exists. Idempotent.
    pub async fn ensure_dayline_eod_action(&self) -> Result<()> {
        let action_id = "action_agent_dayline_eod";

        if sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM app_actions WHERE id = ?)",
        )
        .bind(action_id)
        .fetch_one(&self.db)
        .await
        .unwrap_or(false)
        {
            return Ok(());
        }

        // EOD runs hourly and checks if it's the user's maintenance hour in their timezone.
        // Activation code gates execution to the correct local hour + checks for events.
        let activation_code = r#"import sqlite3, os, datetime
db = sqlite3.connect(os.environ.get('DB_PATH', 'virtues.db'))
row = db.execute("SELECT update_check_hour, timezone FROM app_user_profile LIMIT 1").fetchone()
if not row:
    print("")
else:
    maint_hour, tz_name = row[0] or 8, row[1]
    import zoneinfo
    try:
        tz = zoneinfo.ZoneInfo(tz_name) if tz_name else datetime.timezone.utc
    except Exception:
        tz = datetime.timezone.utc
    local_now = datetime.datetime.now(tz)
    if local_now.hour != maint_hour:
        print("")
    else:
        yesterday = (local_now - datetime.timedelta(days=1)).strftime("%Y-%m-%d")
        count = db.execute("SELECT COUNT(*) FROM wiki_events e JOIN wiki_days d ON e.day_id = d.id WHERE d.date = ?", (yesterday,)).fetchone()[0]
        if count > 0:
            print(f"eod: {yesterday} ({count} events)")
        else:
            print("")
"#;

        // Create the action (no chat needed — system actions are stateless)
        sqlx::query(
            r#"INSERT INTO app_actions (id, action_type, owner, name, instruction, cron_schedule, enabled, config, activation_code)
               VALUES (?, 'agent', 'system', ?, ?, '0 0 * * * *', 1, '{}', ?)"#,
        )
        .bind(action_id)
        .bind("Dayline End of Day")
        .bind(EOD_ACTION_INSTRUCTION)
        .bind(activation_code)
        .execute(&self.db)
        .await?;

        tracing::info!("Seeded dayline EOD action");
        Ok(())
    }

    /// Ensure the nightly day illustration system action exists. Idempotent.
    ///
    /// Runs hourly, gated by `schedule_system_actions` dispatch — the function
    /// itself checks for eligible days (has autobiography + epigraph, no cover
    /// image). Runs a single day per tick to keep image-gen cost predictable.
    pub async fn ensure_dayline_illustration_action(&self) -> Result<()> {
        let action_id = "action_system_dayline_illustration";

        if sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM app_actions WHERE id = ?)",
        )
        .bind(action_id)
        .fetch_one(&self.db)
        .await
        .unwrap_or(false)
        {
            return Ok(());
        }

        // Hourly; the dispatched function checks for eligible days internally.
        // Runs one illustration per tick, walking back up to 7 days.
        let config = serde_json::json!({ "function_name": "day_illustration" });
        sqlx::query(
            r#"INSERT INTO app_actions (id, action_type, owner, name, cron_schedule, enabled, config)
               VALUES (?, 'system', 'system', ?, '0 0 * * * *', 1, ?)"#,
        )
        .bind(action_id)
        .bind("Dayline Illustration")
        .bind(config)
        .execute(&self.db)
        .await?;

        tracing::info!("Seeded dayline illustration action");
        Ok(())
    }

    /// Ensure the Morning Examen template action exists. Idempotent.
    /// Created as a user-owned action (not system) so users can customize it.
    /// Disabled by default — user activates it when ready.
    pub async fn ensure_morning_examen_action(&self) -> Result<()> {
        let action_id = "action_agent_morning_examen";

        if sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM app_actions WHERE id = ?)",
        )
        .bind(action_id)
        .fetch_one(&self.db)
        .await
        .unwrap_or(false)
        {
            return Ok(());
        }

        let template = virtues_registry::get_action_template("morning_examen")
            .expect("morning_examen template must exist in registry");

        sqlx::query(
            r#"INSERT INTO app_actions (id, action_type, owner, name, instruction, cron_schedule, enabled, config, activation_code)
               VALUES (?, 'agent', 'user', ?, ?, ?, 0, '{}', ?)"#,
        )
        .bind(action_id)
        .bind(template.name)
        .bind(template.instruction)
        .bind(template.default_schedule)
        .bind(template.activation_code)
        .execute(&self.db)
        .await?;

        tracing::info!("Seeded morning examen action (disabled by default)");
        Ok(())
    }

    /// Schedule all action_type='system' actions from app_actions.
    ///
    /// System actions are hardcoded Rust jobs (embedding indexer, trash purge)
    /// dispatched by `config.function_name`. Reads enabled rows from app_actions
    /// and registers each with the cron scheduler.
    pub async fn schedule_system_actions(&self) -> Result<()> {
        let rows = sqlx::query_as::<_, (String, String, Option<String>, serde_json::Value)>(
            r#"
            SELECT id, name, cron_schedule, config
            FROM app_actions
            WHERE action_type = 'system'
              AND enabled = 1
              AND cron_schedule IS NOT NULL
            "#,
        )
        .fetch_all(&self.db)
        .await?;

        for (action_id, name, cron_schedule, config) in rows {
            let cron = match cron_schedule {
                Some(c) => c,
                None => continue,
            };
            let function_name = config
                .get("function_name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let Some(function_name) = function_name else {
                tracing::warn!(action_id = %action_id, "System action missing config.function_name, skipping");
                continue;
            };

            let db = self.db.clone();
            let drive_config = self.drive_config.clone();
            let fn_name = function_name.clone();
            let action_label = name.clone();

            let job = Job::new_async(cron.as_str(), move |_uuid, _lock| {
                let db = db.clone();
                let drive_config = drive_config.clone();
                let fn_name = fn_name.clone();
                let action_label = action_label.clone();

                Box::pin(async move {
                    match fn_name.as_str() {
                        "embedding_index" => {
                            if let Err(e) = crate::search::run_embedding_job(&db).await {
                                tracing::error!(action = %action_label, error = %e, "system action failed");
                            }
                        }
                        "trash_purge" => {
                            match crate::api::purge_old_drive_trash(&db, &drive_config).await {
                                Ok(count) if count > 0 => tracing::info!(
                                    action = %action_label,
                                    "purged {} files", count
                                ),
                                Ok(_) => tracing::debug!(action = %action_label, "nothing to purge"),
                                Err(e) => tracing::error!(action = %action_label, error = %e, "purge failed"),
                            }
                        }
                        "day_illustration" => {
                            if let Err(e) = crate::api::day_illustration::run_illustration_job(&db).await {
                                tracing::error!(action = %action_label, error = %e, "illustration job failed");
                            }
                        }
                        other => {
                            tracing::warn!(function_name = %other, "unknown system action function");
                        }
                    }
                })
            })
            .map_err(|e| Error::Other(format!("Failed to create system action job '{}': {}", name, e)))?;

            self.scheduler
                .add(job)
                .await
                .map_err(|e| Error::Other(format!("Failed to add system action job '{}': {}", name, e)))?;

            tracing::info!(
                action_id = %action_id,
                name = %name,
                function_name = %function_name,
                cron = %cron,
                "Scheduled system action"
            );
        }

        Ok(())
    }

    /// Stop the scheduler
    pub async fn stop(&mut self) -> Result<()> {
        self.scheduler
            .shutdown()
            .await
            .map_err(|e| Error::Other(format!("Failed to stop scheduler: {e}")))?;

        tracing::info!("Scheduler stopped");
        Ok(())
    }

    /// Get list of scheduled streams
    pub async fn list_scheduled(&self) -> Result<Vec<ScheduledStream>> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                String,
                Option<Timestamp>,
            ),
        >(
            r#"
            SELECT
                json_extract(t.config, '$.source_connection_id') as source_id,
                s.name as source_name,
                json_extract(t.config, '$.stream_name') as stream_name,
                t.cron_schedule,
                st.last_sync_at
            FROM app_actions t
            JOIN elt_source_connections s
                ON s.id = json_extract(t.config, '$.source_connection_id')
            LEFT JOIN elt_stream_connections st
                ON st.source_connection_id = json_extract(t.config, '$.source_connection_id')
                AND st.stream_name = json_extract(t.config, '$.stream_name')
            WHERE t.action_type = 'sync'
              AND t.enabled = 1
              AND t.cron_schedule IS NOT NULL
              AND s.source NOT IN ('mac', 'ios')
            ORDER BY s.name, json_extract(t.config, '$.stream_name')
            "#,
        )
        .fetch_all(&self.db)
        .await?;

        let streams = rows
            .into_iter()
            .map(
                |(source_id, source_name, stream_name, cron_schedule, last_sync_at)| {
                    ScheduledStream {
                        source_id,
                        source_name,
                        stream_name,
                        cron_schedule,
                        last_sync_at,
                    }
                },
            )
            .collect();

        Ok(streams)
    }
}

/// Information about a scheduled stream
#[derive(Debug)]
pub struct ScheduledStream {
    pub source_id: String,
    pub source_name: String,
    pub stream_name: String,
    pub cron_schedule: String,
    pub last_sync_at: Option<Timestamp>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_scheduler_creation() {
        let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        let storage = Storage::local("./test_data".to_string()).unwrap();
        let stream_writer = Arc::new(Mutex::new(StreamWriter::new()));
        let yjs_state = YjsState::new(pool.clone());
        let result = Scheduler::new(pool, storage, stream_writer, yjs_state).await;
        assert!(result.is_ok());
    }
}

/// Compute "yesterday" in the user's configured timezone.
/// Returns None if no user profile exists.
async fn resolve_user_yesterday(db: &SqlitePool) -> Option<chrono::NaiveDate> {
    let profile: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT timezone FROM app_user_profile LIMIT 1",
    )
    .fetch_optional(db)
    .await
    .ok()?;

    let (timezone,) = profile?;
    let now_utc = chrono::Utc::now();

    let local_today = if let Some(ref tz_str) = timezone {
        if let Ok(tz) = tz_str.parse::<chrono_tz::Tz>() {
            now_utc.with_timezone(&tz).date_naive()
        } else {
            now_utc.date_naive()
        }
    } else {
        now_utc.date_naive()
    };

    Some(local_today - chrono::Duration::days(1))
}

// ============================================================================
// Action instruction constants
// ============================================================================

/// Instruction for the dayline hourly action.
/// The action reads the current hour's ontology data (injected as dynamic context)
/// and produces structured events with summaries, topics, and temporal boundaries.
const HOURLY_ACTION_INSTRUCTION: &str = r#"You are the Dayline hourly event agent. Your job is to process the current hour's data into structured timeline events.

You will receive the current hour's ontology data in the <context> block. Use the dayline_event tool to create or update events.

Guidelines for event summaries (critical for embedding quality):
- Be factual and specific, not literary
- Include ALL data sources active during the event, even minor ones (a 5-min chess game matters)
- Name people, places, projects, apps — specificity creates embedding distance
- Don't editorialize ("meeting with 5 teams about 5 topics" not "stressful meeting")
- Consistent structure: what happened, who was involved, what tools/apps were used, how long

Guidelines for topics (critical for fragmentation signal):
- Topic count reflects attentional complexity — it IS the visual signal for how fragmented or focused an event was
- A single-focus event (commute, run, shower, deep focus) gets 1-2 topics
- A meeting spanning multiple subjects gets one topic per distinct thread discussed (3-6)
- A context-switching block (Slack catchup across channels, email triage) gets more topics (4-8)
- Never pad with generic topics ("work", "morning") to fill space — fewer is better when focus was narrow
- Use specific vocabulary consistently: "figma" not "design tool", "standup" not "meeting", "mueller-trails" not "running path"

For each hour, decide one action:
- NEW: Create a new event from this hour's data
- CONTINUE: This hour continues the previous event (extend its end_time)
- REVISE: Merge or split previous events based on new context
- NO_DATA: Insufficient signal to characterize this hour"#;

/// Instruction for the dayline end-of-day action.
/// Runs once at the user's maintenance hour with full-day context.
const EOD_ACTION_INSTRUCTION: &str = r#"You are the Dayline end-of-day agent. Your job is to polish today's timeline events and generate the day's autobiography.

Review all events created by the hourly agent today. You may:
- Polish event boundaries (merge short fragments, split overly long blocks)
- Refine event summaries with full-day context
- Adjust topic counts to reflect true attentional complexity (a single-focus event should have 1-2 topics, a fragmented multi-thread block should have 4-8)
- Generate the day's autobiography (2-5 sentences capturing what mattered)

Do NOT create events from scratch — the hourly agent handles that. Focus on cleanup and narrative.

Save the autobiography using the edit_page tool on the day's wiki page."#;

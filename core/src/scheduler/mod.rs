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

use chrono::Timelike;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_cron_scheduler::{Job, JobScheduler};

use crate::{
    error::{Error, Result},
    storage::{stream_writer::StreamWriter, Storage},
    types::Timestamp,
};

/// Simplified scheduler using StreamFactory
pub struct Scheduler {
    db: SqlitePool,
    storage: Storage,
    drive_config: crate::api::DriveConfig,
    stream_writer: Arc<Mutex<StreamWriter>>,
    scheduler: JobScheduler,
}

impl Scheduler {
    /// Create a new scheduler
    pub async fn new(
        db: SqlitePool,
        storage: Storage,
        stream_writer: Arc<Mutex<StreamWriter>>,
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
        // First, check for enabled streams WITHOUT cron schedules and log warnings
        // Note: UUIDs stored as TEXT in SQLite
        let streams_without_schedule = sqlx::query_as::<_, (String, String, String, String)>(
            r#"
            SELECT
                st.id as stream_connection_id,
                s.source,
                s.name as source_name,
                st.stream_name
            FROM elt_stream_connections st
            JOIN elt_source_connections s ON st.source_connection_id = s.id
            WHERE st.is_enabled = true
              AND st.cron_schedule IS NULL
              AND s.is_active = true
              AND s.source NOT IN ('mac', 'ios')  -- Exclude push-only sources
            "#,
        )
        .fetch_all(&self.db)
        .await?;

        // Check if any enabled streams are missing schedules
        for (stream_id, source, source_name, stream_name) in &streams_without_schedule {
            // Look up registry default
            if let Some(registered_stream) = crate::registry::get_stream(source, stream_name) {
                if let Some(default_cron) = registered_stream.descriptor.default_cron_schedule {
                    tracing::warn!(
                        "Stream {}/{} ({}) is enabled but has no cron_schedule. \
                        Registry default is '{}'. Consider updating the database or seed config.",
                        source,
                        stream_name,
                        source_name,
                        default_cron
                    );

                    // Apply the default schedule to the database
                    tracing::info!(
                        "Applying registry default cron schedule '{}' to {}/{}",
                        default_cron,
                        source,
                        stream_name
                    );

                    sqlx::query!(
                        r#"
                        UPDATE elt_stream_connections
                        SET cron_schedule = $1, updated_at = datetime('now')
                        WHERE id = $2
                        "#,
                        default_cron,
                        stream_id
                    )
                    .execute(&self.db)
                    .await?;
                } else {
                    tracing::debug!(
                        "Stream {}/{} ({}) is enabled but has no cron_schedule and no registry default. \
                        This stream may need manual scheduling or is not meant to be scheduled.",
                        source, stream_name, source_name
                    );
                }
            } else {
                tracing::warn!(
                    "Stream {}/{} ({}) not found in registry. Cannot apply default schedule.",
                    source,
                    stream_name,
                    source_name
                );
            }
        }

        // Load enabled streams from database
        // Filter to only pull streams (not 'mac' or 'ios' which are push-only)
        let streams = sqlx::query_as::<_, (String, String, String, String, Option<String>)>(
            r#"
            SELECT
                s.id as source_id,
                s.name as source_name,
                s.source,
                st.stream_name,
                st.cron_schedule
            FROM elt_stream_connections st
            JOIN elt_source_connections s ON st.source_connection_id = s.id
            WHERE st.is_enabled = true
              AND st.cron_schedule IS NOT NULL
              AND s.is_active = true
              AND s.source NOT IN ('mac', 'ios')  -- Exclude push-only sources
            "#,
        )
        .fetch_all(&self.db)
        .await?;

        tracing::info!("Loading {} scheduled streams", streams.len());

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

                    // Use the job-based API with String source_id
                    match crate::api::jobs::trigger_stream_sync(
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
                                "Scheduled sync job created: {} - job_id={}, status={}",
                                stream_name_str,
                                response.job_id,
                                response.status
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                "Failed to create scheduled sync job for {}: {}",
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



    /// Schedule the drive trash purge job (daily at 3am)
    ///
    /// Permanently deletes files that have been in trash for more than 30 days.
    pub async fn schedule_drive_trash_purge_job(&self) -> Result<()> {
        let db = self.db.clone();
        let drive_config = self.drive_config.clone();

        // Daily at 3am
        let cron_expr = "0 0 3 * * *";

        tracing::info!("Scheduling DriveTrashPurgeJob daily at 3am");

        let job = Job::new_async(cron_expr, move |_uuid, _lock| {
            let db = db.clone();
            let config = drive_config.clone();

            Box::pin(async move {
                tracing::info!("Running DriveTrashPurgeJob");

                match crate::api::purge_old_drive_trash(&db, &config).await {
                    Ok(count) => {
                        if count > 0 {
                            tracing::info!(
                                "DriveTrashPurgeJob completed: {} files permanently deleted",
                                count
                            );
                        } else {
                            tracing::debug!("DriveTrashPurgeJob: no files to purge");
                        }
                    }
                    Err(e) => {
                        tracing::error!("DriveTrashPurgeJob failed: {}", e);
                    }
                }
            })
        })
        .map_err(|e| Error::Other(format!("Failed to create DriveTrashPurgeJob: {}", e)))?;

        self.scheduler
            .add(job)
            .await
            .map_err(|e| Error::Other(format!("Failed to add DriveTrashPurgeJob: {}", e)))?;

        tracing::info!("DriveTrashPurgeJob scheduled daily at 3am");
        Ok(())
    }

    /// Schedule the daily summary job (hourly check, runs at user's update_check_hour)
    ///
    /// Checks every hour whether it's the user's configured maintenance hour
    /// (update_check_hour) in their timezone. If so, generates yesterday's
    /// daily summary (autobiography + context vector + chaos scoring).
    pub async fn schedule_daily_summary_job(&self) -> Result<()> {
        let db = self.db.clone();

        // Every hour at :00
        let cron_expr = "0 0 * * * *";

        tracing::info!("Scheduling DailySummaryJob (hourly check)");

        let job = Job::new_async(cron_expr, move |_uuid, _lock| {
            let db = db.clone();

            Box::pin(async move {
                // 1. Read user's maintenance hour and timezone
                let profile: Option<(Option<i32>, Option<String>)> = sqlx::query_as(
                    "SELECT update_check_hour, timezone FROM app_user_profile LIMIT 1",
                )
                .fetch_optional(&db)
                .await
                .unwrap_or(None);

                let (maintenance_hour, timezone) = match profile {
                    Some((hour, tz)) => (hour.unwrap_or(8), tz), // 8 UTC ≈ midnight PST / 3am EST
                    None => return, // No profile yet
                };

                // 2. Compute current hour in the user's timezone (or UTC if unset)
                let now_utc = chrono::Utc::now();
                let current_hour = if let Some(ref tz_str) = timezone {
                    if let Ok(tz) = tz_str.parse::<chrono_tz::Tz>() {
                        now_utc.with_timezone(&tz).hour()
                    } else {
                        now_utc.hour()
                    }
                } else {
                    now_utc.hour()
                };

                // 3. Only proceed if it's the maintenance hour
                if current_hour != maintenance_hour as u32 {
                    return;
                }

                tracing::info!("Running DailySummaryJob (maintenance hour {} in user's timezone)", maintenance_hour);

                // 4. Compute "yesterday" in the user's timezone
                let yesterday = if let Some(ref tz_str) = timezone {
                    if let Ok(tz) = tz_str.parse::<chrono_tz::Tz>() {
                        let local_now = now_utc.with_timezone(&tz);
                        local_now.date_naive() - chrono::Duration::days(1)
                    } else {
                        now_utc.date_naive() - chrono::Duration::days(1)
                    }
                } else {
                    now_utc.date_naive() - chrono::Duration::days(1)
                };

                // 5. Check if autobiography already exists (idempotent)
                let existing = crate::api::wiki::get_or_create_day(&db, yesterday).await;
                match existing {
                    Ok(day) if day.autobiography.is_some() => {
                        tracing::debug!(
                            "DailySummaryJob: summary already exists for {}, skipping",
                            yesterday
                        );
                        return;
                    }
                    Err(e) => {
                        tracing::error!("DailySummaryJob: failed to check day {}: {}", yesterday, e);
                        return;
                    }
                    _ => {}
                }

                // 6. Generate the summary
                match crate::api::day_summary::generate_day_summary(&db, yesterday).await {
                    Ok(day) => {
                        tracing::info!(
                            "DailySummaryJob: generated summary for {} (chaos_score={:?})",
                            yesterday,
                            day.chaos_score
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            "DailySummaryJob: failed to generate summary for {}: {}",
                            yesterday,
                            e
                        );
                    }
                }
            })
        })
        .map_err(|e| Error::Other(format!("Failed to create DailySummaryJob: {}", e)))?;

        self.scheduler
            .add(job)
            .await
            .map_err(|e| Error::Other(format!("Failed to add DailySummaryJob: {}", e)))?;

        tracing::info!("DailySummaryJob scheduled (checks every hour)");
        Ok(())
    }

    /// Schedule the embedding indexer job (every 15 minutes)
    ///
    /// Processes new records from searchable ontologies and generates
    /// embeddings via the local fastembed model.
    pub async fn schedule_embedding_job(&self) -> Result<()> {
        let db = self.db.clone();

        // Every 15 minutes
        let cron_expr = "0 */15 * * * *";

        tracing::info!("Scheduling EmbeddingIndexerJob every 15 minutes");

        let job = Job::new_async(cron_expr, move |_uuid, _lock| {
            let db = db.clone();

            Box::pin(async move {
                match crate::search::run_embedding_job(&db).await {
                    Ok(()) => {}
                    Err(e) => {
                        tracing::error!("EmbeddingIndexerJob failed: {}", e);
                    }
                }
            })
        })
        .map_err(|e| Error::Other(format!("Failed to create EmbeddingIndexerJob: {}", e)))?;

        self.scheduler
            .add(job)
            .await
            .map_err(|e| Error::Other(format!("Failed to add EmbeddingIndexerJob: {}", e)))?;

        tracing::info!("EmbeddingIndexerJob scheduled every 15 minutes");
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
                s.id as source_id,
                s.name as source_name,
                st.stream_name,
                st.cron_schedule,
                st.last_sync_at
            FROM elt_stream_connections st
            JOIN elt_source_connections s ON st.source_connection_id = s.id
            WHERE st.is_enabled = true
              AND st.cron_schedule IS NOT NULL
              AND s.source NOT IN ('mac', 'ios')  -- Exclude push-only sources
            ORDER BY s.name, st.stream_name
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
        let result = Scheduler::new(pool, storage, stream_writer).await;
        assert!(result.is_ok());
    }
}

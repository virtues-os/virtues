//! Job executor for running async jobs in background tasks

use crate::error::Result;
use crate::jobs::models::{JobStatus, JobType};
use crate::jobs::sync_job::execute_sync_job;
use crate::jobs::transform_context::TransformContext;
use crate::jobs::transform_job::execute_transform_job;
use crate::observability::JobTimer;
use sqlx::SqlitePool;
use std::sync::Arc;

/// Job executor that spawns background tasks for async job execution
#[derive(Clone)]
pub struct JobExecutor {
    db: SqlitePool,
    context: Arc<TransformContext>,
}

impl JobExecutor {
    /// Create a new job executor
    ///
    /// # Arguments
    /// * `db` - Database connection pool
    /// * `context` - Transform context with storage and API keys
    pub fn new(db: SqlitePool, context: TransformContext) -> Self {
        Self {
            db,
            context: Arc::new(context),
        }
    }

    /// Execute a job asynchronously in a background task
    ///
    /// This spawns a Tokio task to run the job and returns immediately.
    /// The job status will be updated in the database as it progresses.
    pub fn execute_async(&self, job_id: String) {
        let db = self.db.clone();
        let context = self.context.clone();

        tokio::spawn(async move {
            if let Err(e) = Self::run_job(&db, &context, &job_id).await {
                tracing::error!(
                    job_id = %job_id,
                    error = %e,
                    "Job execution failed"
                );
            }
        });
    }

    /// Internal method to run a job
    async fn run_job(db: &SqlitePool, context: &Arc<TransformContext>, job_id: &str) -> Result<()> {
        // Fetch the job
        let job = super::get_job(db, job_id).await?;

        // Check if job is in pending state
        if job.status != JobStatus::Pending {
            tracing::warn!(
                job_id = %job_id,
                status = %job.status,
                "Job is not in pending state, skipping execution"
            );
            return Ok(());
        }

        // Update job status to running
        super::update_job_status(db, &job.id, JobStatus::Running, None).await?;

        // Start metrics timer
        let job_type_str = job.job_type.to_string();
        let timer = JobTimer::start(&job_type_str);

        tracing::info!(
            job_id = %job_id,
            job_type = %job.job_type,
            "Starting job execution"
        );

        // Create executor for job chaining (with same context)
        let executor = JobExecutor {
            db: db.clone(),
            context: context.clone(),
        };

        // Execute the job based on type
        let result = match job.job_type {
            JobType::Sync => execute_sync_job(db, &executor, context, &job).await,
            JobType::Transform => execute_transform_job(db, &executor, context, &job).await,
            JobType::Archive => {
                // Archive jobs are typically executed directly from sync_job with records
                // This path is for retries or manual triggers where records are in S3
                tracing::warn!(
                    job_id = %job_id,
                    "Archive job execution via executor not yet implemented (records not available)"
                );
                Err(crate::error::Error::Other(
                    "Archive job retry logic not yet implemented".to_string(),
                ))
            }
        };

        // Record metrics and log result
        match &result {
            Ok(_) => {
                timer.success();
            }
            Err(e) => {
                timer.failure(&e.to_string());
                // Ensure job is marked as failed even if the job handler didn't do it
                // (e.g., early metadata validation errors in transform_job)
                let _ = super::update_job_status(
                    db,
                    &job.id,
                    JobStatus::Failed,
                    Some(e.to_string()),
                )
                .await;
            }
        }

        result
    }
}

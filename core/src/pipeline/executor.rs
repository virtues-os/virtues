//! Pipeline executor for running syncs and transforms
//!
//! Holds shared context (DB + storage + API keys) and provides
//! async execution methods for sync and transform operations.

use crate::pipeline::context::TransformContext;
use crate::pipeline::transform::TransformConfig;
use crate::scheduler::actions;
use sqlx::SqlitePool;
use std::sync::Arc;

/// Executor that spawns background tasks for pipeline operations.
#[derive(Clone)]
pub struct PipelineExecutor {
    pub db: SqlitePool,
    pub context: Arc<TransformContext>,
}

impl PipelineExecutor {
    pub fn new(db: SqlitePool, context: TransformContext) -> Self {
        Self {
            db,
            context: Arc::new(context),
        }
    }

    /// Execute a transform in the background with explicit config.
    ///
    /// Used for chained transforms (sync → transform, transform → entity resolution).
    /// The run should already be created in app_action_runs before calling this.
    pub fn execute_transform_with_config_async(&self, run_id: String, config: TransformConfig) {
        let db = self.db.clone();
        let context = self.context.clone();
        let executor = self.clone();

        tokio::spawn(async move {
            let result = super::transform::execute_transform_with_config(
                &db,
                &executor,
                &context,
                &run_id,
                &config,
            )
            .await;

            // Note: execute_transform_with_config already calls complete_run internally,
            // so we only need to handle the case where the function itself panics/errors
            // before reaching that point.
            if let Err(e) = result {
                tracing::error!(run_id = %run_id, error = %e, "Transform execution failed");
                let _ = actions::complete_run(&db, &run_id, "error", 0, Some(&e.to_string())).await;
            }
        });
    }
}

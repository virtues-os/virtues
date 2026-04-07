//! Transform execution logic
//!
//! Runs a transformation that converts raw stream data into normalized ontology tables.
//! Config is passed as parameters, not stored on the run.

use sqlx::SqlitePool;
use std::sync::Arc;

use crate::error::Result;
use crate::pipeline::context::TransformContext;
use crate::pipeline::executor::PipelineExecutor;
use crate::scheduler::actions;

/// Config for a transform execution, passed by caller.
#[derive(Debug, Clone)]
pub struct TransformConfig {
    pub source_table: String,
    pub target_table: String,
    pub source_id: String,
}

/// Execute a transform given its config and run_id.
#[tracing::instrument(skip(db, _executor, _context), fields(run_id = %run_id))]
pub async fn execute_transform(
    db: &SqlitePool,
    _executor: &PipelineExecutor,
    _context: &Arc<TransformContext>,
    run_id: &str,
) -> Result<()> {
    // Read the run to get the transform_stage and parent info
    let run = actions::get_run(db, run_id).await?;

    let transform_stage = run.transform_stage.as_deref().unwrap_or("transform");

    // For chained transforms, we need to reconstruct what to transform.
    // The parent run's transform_stage tells us what we are.
    // We look up the source_table and target_table from the registry based on the run's parent chain.
    // For now, we fall back to reading from the old elt_jobs table if it still exists,
    // or we use the info passed through execute_transform_with_config.

    tracing::info!(
        run_id = %run_id,
        transform_stage = %transform_stage,
        "Transform execution delegated to execute_transform_with_config"
    );

    // This path shouldn't be hit directly — callers should use execute_transform_with_config
    Err(crate::Error::Other(
        "execute_transform called without config — use execute_transform_with_config instead".into()
    ))
}

/// Execute a transform with explicit config (the primary entry point).
#[tracing::instrument(skip(db, executor, context, config), fields(run_id = %run_id, source_table = %config.source_table, target_table = %config.target_table))]
pub async fn execute_transform_with_config(
    db: &SqlitePool,
    executor: &PipelineExecutor,
    context: &Arc<TransformContext>,
    run_id: &str,
    config: &TransformConfig,
) -> Result<()> {
    tracing::info!(
        source_table = %config.source_table,
        target_table = %config.target_table,
        source_id = %config.source_id,
        "Starting transform execution"
    );

    // Look up transformer from registry (no factory indirection)
    let transformer = crate::registry::find_transform(&config.source_table, &config.target_table, context)?;

    // Create database wrapper from pool
    let db_wrapper = crate::database::Database::from_pool(db.clone());

    // Execute transformation
    let result = transformer.transform(&db_wrapper, context, config.source_id.clone()).await;

    match result {
        Ok(transform_result) => {
            // Update run with record count
            let _ = actions::complete_run(
                db,
                run_id,
                "success",
                transform_result.records_written as i64,
                None,
            )
            .await;

            tracing::info!(
                run_id = %run_id,
                records_read = transform_result.records_read,
                records_written = transform_result.records_written,
                records_failed = transform_result.records_failed,
                "Transform completed"
            );

            // Create and execute chained transform runs
            for chained in &transform_result.chained_transforms {
                let child_run = actions::create_child_run(
                    db,
                    run_id,
                    &chained.transform_stage,
                    "cron", // inherited from parent
                )
                .await;

                match child_run {
                    Ok(child) => {
                        tracing::info!(
                            parent_run_id = %run_id,
                            child_run_id = %child.id,
                            transform_stage = %chained.transform_stage,
                            "Created chained transform run"
                        );

                        // Build config for the child transform
                        let child_config = TransformConfig {
                            source_table: chained.source_table.clone(),
                            target_table: chained.target_tables.first()
                                .cloned()
                                .unwrap_or_default(),
                            source_id: chained.source_record_id.clone(),
                        };

                        executor.execute_transform_with_config_async(
                            child.id,
                            child_config,
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            parent_run_id = %run_id,
                            error = %e,
                            "Failed to create chained transform run"
                        );
                    }
                }
            }

            Ok(())
        }
        Err(e) => {
            let _ = actions::complete_run(
                db,
                run_id,
                "error",
                0,
                Some(&e.to_string()),
            )
            .await;

            tracing::error!(
                run_id = %run_id,
                error = %e,
                "Transform failed"
            );

            // Return Ok — error is already recorded via complete_run.
            // Returning Err would cause executor.rs to call complete_run again.
            Ok(())
        }
    }
}

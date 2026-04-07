//! Shared transform triggering logic for both cloud syncs and device ingest
//!
//! Creates and executes transform jobs after records have been collected,
//! whether from cloud API syncs or device ingest batches.

use crate::error::{Error, Result};
use crate::scheduler::actions;
use crate::pipeline::{PipelineExecutor, TransformContext};
use crate::pipeline::transform::TransformConfig;
use crate::registry;
use crate::sources::base::MemoryDataSource;
use sqlx::SqlitePool;
use std::sync::Arc;

/// Create and execute transform jobs for a stream.
///
/// For each target ontology table defined in the registry, creates a task_run
/// and dispatches the transform asynchronously.
///
/// Hot path (records provided): uses MemoryDataSource for direct in-memory transform.
/// Cold path (no records): reads from storage.
pub async fn create_transform_job_for_stream(
    db: &SqlitePool,
    executor: &PipelineExecutor,
    context: &Arc<TransformContext>,
    source_id: String,
    stream_name: &str,
    records: Option<Vec<serde_json::Value>>,
    parent_run_id: Option<&str>,
) -> Result<String> {
    let table_name = registry::normalize_stream_name(stream_name);

    let (_source_name, stream) =
        registry::get_stream_by_table_name(&table_name).ok_or_else(|| {
            let err = Error::InvalidInput(format!(
                "Unknown stream for transform: '{}'. Check registry for valid streams.",
                table_name
            ));
            tracing::error!(
                error = %err,
                stream_name = %stream_name,
                normalized_table_name = %table_name,
                source_id = %source_id,
                "Transform route not found"
            );
            err
        })?;

    let target_ontologies = &stream.descriptor.target_ontologies;
    let mut first_run_id: Option<String> = None;

    for target_ontology in target_ontologies {
        // Create the task run
        let run = if let Some(parent_id) = parent_run_id {
            actions::create_child_run(db, parent_id, "transform", "cron").await?
        } else {
            actions::create_run(db, None, "push").await?
        };

        if first_run_id.is_none() {
            first_run_id = Some(run.id.clone());
        }

        let config = TransformConfig {
            source_table: stream.descriptor.table_name.to_string(),
            target_table: target_ontology.to_string(),
            source_id: source_id.clone(),
        };

        if let Some(ref records) = records {
            tracing::info!(
                run_id = %run.id,
                source_id = %source_id,
                stream_name,
                record_count = records.len(),
                source_table = %config.source_table,
                target_table = %config.target_table,
                "Transform job created (HOT PATH)"
            );

            // Create MemoryDataSource with records
            let memory_source = MemoryDataSource::new(
                records.clone(),
                source_id.clone(),
                stream_name.to_string(),
                None,
                None,
                db.clone(),
            );

            // Create a new context with memory data source
            let transform_context_with_memory = TransformContext::with_data_source(
                Arc::clone(&context.storage),
                context.stream_writer.clone(),
                Arc::new(memory_source),
                context.api_keys.clone(),
            );

            // Create a new executor with the memory-enabled context
            let memory_executor = PipelineExecutor::new(db.clone(), transform_context_with_memory);
            memory_executor.execute_transform_with_config_async(run.id, config);
        } else {
            tracing::info!(
                run_id = %run.id,
                source_id = %source_id,
                stream_name,
                source_table = %config.source_table,
                target_table = %config.target_table,
                "Transform job created (COLD PATH)"
            );

            executor.execute_transform_with_config_async(run.id, config);
        }
    }

    Ok(first_run_id.unwrap_or_default())
}

//! Actions API — run tracking and manual triggering
//!
//! Queries app_action_runs.

use crate::error::{Error, Result};
use crate::pipeline::{ApiKeys, PipelineExecutor, TransformContext};
use crate::scheduler::actions::{self, Action, ActionRun};
use crate::storage::{stream_writer::StreamWriter, Storage};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Response when a sync is triggered
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerSyncResponse {
    pub run_id: String,
    pub status: String,
    pub started_at: String,
}

/// Trigger a sync for a specific stream.
///
/// Creates an action_run and starts the sync in the background.
pub async fn trigger_stream_sync(
    db: &SqlitePool,
    storage: &Storage,
    stream_writer: Arc<Mutex<StreamWriter>>,
    source_id: String,
    stream_name: &str,
    sync_mode: Option<crate::sources::base::SyncMode>,
) -> Result<TriggerSyncResponse> {
    // Find the sync action for this stream (if one exists)
    let action = actions::find_action_by_config(db, "sync", "source_connection_id", &source_id)
        .await
        .ok()
        .flatten();

    // Check for active run (if we have an action)
    if let Some(ref a) = action {
        if actions::has_active_run(db, &a.id).await? {
            return Err(Error::InvalidInput(format!(
                "Stream '{}' already has an active sync run",
                stream_name
            )));
        }
    }

    // Create an action_run
    let action_id = action.as_ref().map(|a| a.id.as_str());
    let run = actions::create_run(db, action_id, "manual").await?;

    let response = TriggerSyncResponse {
        run_id: run.id.clone(),
        status: run.status.clone(),
        started_at: run.started_at.clone(),
    };

    // Spawn background sync execution
    let db = db.clone();
    let storage = storage.clone();
    let run_id = run.id.clone();
    let source_id_owned = source_id.clone();
    let stream_name_owned = stream_name.to_string();

    tokio::spawn(async move {
        let api_keys = ApiKeys::from_env();
        let context = Arc::new(TransformContext::new(
            Arc::new(storage.clone()),
            stream_writer,
            api_keys,
        ));
        let executor = PipelineExecutor::new(db.clone(), (*context).clone());

        let result = crate::pipeline::sync::execute_sync(
            &db,
            &executor,
            &context,
            &run_id,
            &source_id_owned,
            &stream_name_owned,
            sync_mode,
        )
        .await;

        match result {
            Ok(()) => {
                let _ = actions::complete_run(&db, &run_id, "success", 0, None).await;
            }
            Err(e) => {
                tracing::error!(run_id = %run_id, error = %e, "Sync execution failed");
                let _ = actions::complete_run(&db, &run_id, "error", 0, Some(&e.to_string())).await;
            }
        }
    });

    Ok(response)
}

/// Get run by ID
pub async fn get_run(db: &SqlitePool, run_id: &str) -> Result<ActionRun> {
    actions::get_run(db, run_id).await
}

/// Query runs with filters
#[derive(Debug, Clone, Deserialize)]
pub struct QueryRunsRequest {
    pub action_id: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
}

pub async fn query_runs(db: &SqlitePool, request: QueryRunsRequest) -> Result<Vec<ActionRun>> {
    actions::query_runs(
        db,
        request.action_id.as_deref(),
        request.status.as_deref(),
        request.limit.unwrap_or(16),
    )
    .await
}

/// Cancel a running run
pub async fn cancel_run(db: &SqlitePool, run_id: &str) -> Result<()> {
    actions::cancel_run(db, run_id).await
}

/// Get run history for a specific stream
pub async fn get_run_history(
    db: &SqlitePool,
    source_id: &str,
    _stream_name: &str,
    limit: i64,
) -> Result<Vec<ActionRun>> {
    // Find the sync action for this stream, then query its runs
    let action = actions::find_action_by_config(db, "sync", "source_connection_id", source_id)
        .await
        .ok()
        .flatten();

    match action {
        Some(a) => actions::query_runs(db, Some(&a.id), None, limit).await,
        None => Ok(vec![]), // No action = no history
    }
}

/// List all actions
pub async fn list_actions(db: &SqlitePool) -> Result<Vec<Action>> {
    actions::get_all_actions(db).await
}

/// Toggle an action's enabled state
pub async fn toggle_action(db: &SqlitePool, action_id: &str, enabled: bool) -> Result<()> {
    actions::toggle_action(db, action_id, enabled).await
}

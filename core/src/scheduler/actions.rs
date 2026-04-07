//! Action + ActionRun models and CRUD operations
//!
//! Two tables, two structs, clean semantics:
//! - Action: what runs and when (pure config, no state)
//! - ActionRun: one execution of an action (all history)
//!
//! action_type subtypes:
//! - 'sync'   = data pipeline (fetch → transform → write), no LLM
//! - 'agent'  = LLM agent loop with chat, instruction, optional activation gate
//! - 'system' = hardcoded Rust job (embedding indexer, trash purge)

use crate::error::{Error, Result};
use crate::ids::generate_id;
use sqlx::{Row, SqlitePool};

// ID prefixes
const RUN_PREFIX: &str = "run";

/// A scheduled action — pure configuration, no runtime state.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Action {
    pub id: String,
    pub action_type: String,
    pub owner: String,
    pub name: String,
    pub instruction: Option<String>,
    pub cron_schedule: Option<String>,
    pub enabled: bool,
    pub config: serde_json::Value,
    pub activation_code: Option<String>,
    pub concurrency_mode: String,
    pub memory: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// One execution of an action.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActionRun {
    pub id: String,
    pub action_id: Option<String>,
    pub status: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub records_processed: i64,
    pub error: Option<String>,
    pub trigger: String,
    pub parent_run_id: Option<String>,
    pub transform_stage: Option<String>,
    pub result_summary: Option<String>,
    pub created_at: String,
}

// ============================================================================
// Action CRUD
// ============================================================================

/// Get all enabled actions.
pub async fn get_enabled_actions(db: &SqlitePool) -> Result<Vec<Action>> {
    let rows = sqlx::query(
        "SELECT * FROM app_actions WHERE enabled = 1 ORDER BY action_type, name",
    )
    .fetch_all(db)
    .await?;

    rows.iter().map(action_from_row).collect()
}

/// Get all actions (for API listing).
pub async fn get_all_actions(db: &SqlitePool) -> Result<Vec<Action>> {
    let rows = sqlx::query("SELECT * FROM app_actions ORDER BY action_type, name")
        .fetch_all(db)
        .await?;

    rows.iter().map(action_from_row).collect()
}

/// Get an action by ID.
pub async fn get_action(db: &SqlitePool, action_id: &str) -> Result<Action> {
    let row = sqlx::query("SELECT * FROM app_actions WHERE id = ?")
        .bind(action_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Action not found: {}", action_id)))?;

    action_from_row(&row)
}

/// Find an action by config field (e.g., find sync action by source_connection_id + stream_name).
pub async fn find_action_by_config(
    db: &SqlitePool,
    action_type: &str,
    key: &str,
    value: &str,
) -> Result<Option<Action>> {
    let row = sqlx::query(
        "SELECT * FROM app_actions WHERE action_type = ? AND json_extract(config, ?) = ?",
    )
    .bind(action_type)
    .bind(format!("$.{}", key))
    .bind(value)
    .fetch_optional(db)
    .await?;

    row.as_ref().map(action_from_row).transpose()
}

/// Create a new action.
pub async fn create_action(
    db: &SqlitePool,
    id: &str,
    action_type: &str,
    name: &str,
    cron_schedule: Option<&str>,
    config: &serde_json::Value,
    activation_code: Option<&str>,
) -> Result<Action> {
    let row = sqlx::query(
        r#"INSERT INTO app_actions (id, action_type, name, cron_schedule, config, activation_code)
           VALUES (?, ?, ?, ?, ?, ?)
           RETURNING *"#,
    )
    .bind(id)
    .bind(action_type)
    .bind(name)
    .bind(cron_schedule)
    .bind(config)
    .bind(activation_code)
    .fetch_one(db)
    .await?;

    action_from_row(&row)
}

/// Toggle an action's enabled state.
pub async fn toggle_action(db: &SqlitePool, action_id: &str, enabled: bool) -> Result<()> {
    let affected = sqlx::query("UPDATE app_actions SET enabled = ? WHERE id = ?")
        .bind(enabled)
        .bind(action_id)
        .execute(db)
        .await?
        .rows_affected();

    if affected == 0 {
        return Err(Error::NotFound(format!("Action not found: {}", action_id)));
    }
    Ok(())
}

/// Update an action's persistent memory (markdown scratchpad across runs).
pub async fn update_memory(db: &SqlitePool, action_id: &str, memory: &str) -> Result<()> {
    let affected = sqlx::query("UPDATE app_actions SET memory = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(memory)
        .bind(action_id)
        .execute(db)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(Error::NotFound(format!("Action not found: {}", action_id)));
    }
    Ok(())
}

/// Delete an action. Nullifies action_id on existing runs first (FK safety).
pub async fn delete_action(db: &SqlitePool, action_id: &str) -> Result<()> {
    // Guard: system actions cannot be deleted
    let owner: Option<String> = sqlx::query_scalar("SELECT owner FROM app_actions WHERE id = ?")
        .bind(action_id)
        .fetch_optional(db)
        .await?;
    if owner.as_deref() == Some("system") {
        return Err(crate::Error::InvalidInput("Cannot delete system action".into()).into());
    }
    sqlx::query("UPDATE app_action_runs SET action_id = NULL WHERE action_id = ?")
        .bind(action_id)
        .execute(db)
        .await?;
    sqlx::query("DELETE FROM app_actions WHERE id = ?")
        .bind(action_id)
        .execute(db)
        .await?;
    Ok(())
}

// ============================================================================
// ActionRun CRUD
// ============================================================================

/// Create a new run for an action.
pub async fn create_run(
    db: &SqlitePool,
    action_id: Option<&str>,
    trigger: &str,
) -> Result<ActionRun> {
    let run_id = generate_id(RUN_PREFIX, &[
        action_id.unwrap_or("adhoc"),
        trigger,
        &chrono::Utc::now().to_rfc3339(),
    ]);

    let row = sqlx::query(
        r#"INSERT INTO app_action_runs (id, action_id, trigger)
           VALUES (?, ?, ?)
           RETURNING *"#,
    )
    .bind(&run_id)
    .bind(action_id)
    .bind(trigger)
    .fetch_one(db)
    .await?;

    run_from_row(&row)
}

/// Create a child run (for transform chaining).
pub async fn create_child_run(
    db: &SqlitePool,
    parent_run_id: &str,
    transform_stage: &str,
    trigger: &str,
) -> Result<ActionRun> {
    let run_id = generate_id(RUN_PREFIX, &[
        parent_run_id,
        transform_stage,
        &chrono::Utc::now().to_rfc3339(),
    ]);

    let row = sqlx::query(
        r#"INSERT INTO app_action_runs (id, parent_run_id, transform_stage, trigger)
           VALUES (?, ?, ?, ?)
           RETURNING *"#,
    )
    .bind(&run_id)
    .bind(parent_run_id)
    .bind(transform_stage)
    .bind(trigger)
    .fetch_one(db)
    .await?;

    run_from_row(&row)
}

/// Complete a run (success or error).
pub async fn complete_run(
    db: &SqlitePool,
    run_id: &str,
    status: &str,
    records_processed: i64,
    error: Option<&str>,
) -> Result<()> {
    sqlx::query(
        r#"UPDATE app_action_runs
           SET status = ?, completed_at = datetime('now'), records_processed = ?, error = ?
           WHERE id = ?"#,
    )
    .bind(status)
    .bind(records_processed)
    .bind(error)
    .bind(run_id)
    .execute(db)
    .await?;

    Ok(())
}

/// Check if an action has an active (running) run.
pub async fn has_active_run(db: &SqlitePool, action_id: &str) -> Result<bool> {
    let result = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM app_action_runs WHERE action_id = ? AND status = 'running')",
    )
    .bind(action_id)
    .fetch_one(db)
    .await?;

    Ok(result)
}

/// Get the most recent run for an action.
pub async fn last_run(db: &SqlitePool, action_id: &str) -> Result<Option<ActionRun>> {
    let row = sqlx::query(
        "SELECT * FROM app_action_runs WHERE action_id = ? ORDER BY created_at DESC LIMIT 1",
    )
    .bind(action_id)
    .fetch_optional(db)
    .await?;

    row.as_ref().map(run_from_row).transpose()
}

/// Get a run by ID.
pub async fn get_run(db: &SqlitePool, run_id: &str) -> Result<ActionRun> {
    let row = sqlx::query("SELECT * FROM app_action_runs WHERE id = ?")
        .bind(run_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Run not found: {}", run_id)))?;

    run_from_row(&row)
}

/// Query runs with filters.
pub async fn query_runs(
    db: &SqlitePool,
    action_id: Option<&str>,
    status: Option<&str>,
    limit: i64,
) -> Result<Vec<ActionRun>> {
    let rows = sqlx::query(
        r#"SELECT * FROM app_action_runs
           WHERE (? IS NULL OR action_id = ?)
             AND (? IS NULL OR status = ?)
           ORDER BY created_at DESC
           LIMIT ?"#,
    )
    .bind(action_id)
    .bind(action_id)
    .bind(status)
    .bind(status)
    .bind(limit)
    .fetch_all(db)
    .await?;

    rows.iter().map(run_from_row).collect()
}

/// Cancel a running run.
pub async fn cancel_run(db: &SqlitePool, run_id: &str) -> Result<()> {
    let affected = sqlx::query(
        r#"UPDATE app_action_runs
           SET status = 'cancelled', completed_at = datetime('now')
           WHERE id = ? AND status = 'running'"#,
    )
    .bind(run_id)
    .execute(db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(Error::InvalidInput(
            "Run cannot be cancelled (not found or already completed)".to_string(),
        ));
    }
    Ok(())
}

/// Mark all stale running runs as error (called on startup).
pub async fn cleanup_stale_runs(db: &SqlitePool) -> Result<u64> {
    let affected = sqlx::query(
        r#"UPDATE app_action_runs
           SET status = 'error', error = 'interrupted by restart', completed_at = datetime('now')
           WHERE status = 'running'"#,
    )
    .execute(db)
    .await?
    .rows_affected();

    Ok(affected)
}

/// Get child runs for a parent run.
pub async fn get_child_runs(db: &SqlitePool, parent_run_id: &str) -> Result<Vec<ActionRun>> {
    let rows = sqlx::query(
        "SELECT * FROM app_action_runs WHERE parent_run_id = ? ORDER BY created_at ASC",
    )
    .bind(parent_run_id)
    .fetch_all(db)
    .await?;

    rows.iter().map(run_from_row).collect()
}

// ============================================================================
// Row mapping helpers
// ============================================================================

pub fn action_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Action> {
    Ok(Action {
        id: row.try_get("id")?,
        action_type: row.try_get("action_type")?,
        owner: row.try_get("owner")?,
        name: row.try_get("name")?,
        instruction: row.try_get("instruction")?,
        cron_schedule: row.try_get("cron_schedule")?,
        enabled: row.try_get::<bool, _>("enabled")?,
        config: row.try_get("config")?,
        activation_code: row.try_get("activation_code")?,
        concurrency_mode: row.try_get("concurrency_mode")?,
        memory: row.try_get("memory")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn run_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<ActionRun> {
    Ok(ActionRun {
        id: row.try_get("id")?,
        action_id: row.try_get("action_id")?,
        status: row.try_get("status")?,
        started_at: row.try_get("started_at")?,
        completed_at: row.try_get("completed_at")?,
        records_processed: row.try_get("records_processed")?,
        error: row.try_get("error")?,
        trigger: row.try_get("trigger")?,
        parent_run_id: row.try_get("parent_run_id")?,
        transform_stage: row.try_get("transform_stage")?,
        result_summary: row.try_get("result_summary")?,
        created_at: row.try_get("created_at")?,
    })
}

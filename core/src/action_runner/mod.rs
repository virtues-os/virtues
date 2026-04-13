//! Action runner: spawn action binaries as subprocesses and route stdin/stdout JSON.
//!
//! This is the new execution layer for actions migrated to the actions/ crate.
//! It coexists with the legacy scheduler dispatch in `scheduler/mod.rs` — only
//! actions with `function_name` set use this path.
//!
//! ## Contract
//!
//! Subprocesses receive a single JSON object on stdin:
//! ```json
//! { "config": { ... }, "credentials": { ... } | null, "payload": [...] | null }
//! ```
//!
//! And write a single JSON object to stdout:
//! ```json
//! { "result": "summary string", "config": { ... } }
//! ```
//!
//! - Exit 0 → success. Runner saves returned config back to `app_actions.config`
//!   and records a `success` row in `app_action_runs`.
//! - Exit non-zero → failure. Runner records a `failed` row with stderr as the
//!   error message. Config is NOT updated.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::error::{Error, Result};

/// JSON object piped to the subprocess via stdin.
#[derive(Debug, Serialize)]
pub struct ActionInput<'a> {
    pub config: &'a serde_json::Value,
    pub credentials: Option<serde_json::Value>,
    pub payload: Option<&'a serde_json::Value>,
}

/// JSON object received from the subprocess on stdout.
#[derive(Debug, Deserialize)]
pub struct ActionOutput {
    pub result: String,
    pub config: serde_json::Value,
}

/// Result of running an action subprocess.
pub struct ActionRunResult {
    pub status: ActionRunStatus,
    pub summary: String,
    pub stderr: String,
}

pub enum ActionRunStatus {
    Success,
    Failed,
    NotFound,
}

/// Look up an action row by function_name and credential_id, then spawn it as a subprocess.
///
/// Returns `None` if no matching action exists. Returns `Some(result)` for any
/// action that was looked up, even if the subprocess failed.
pub async fn run_push_action(
    db: &SqlitePool,
    function_name: &str,
    credential_id: &str,
    payload: &serde_json::Value,
) -> Result<Option<ActionRunResult>> {
    // 1. Look up the action row
    let row: Option<(String, Option<String>, Option<String>)> = sqlx::query_as(
        r#"SELECT id, config, credential_id
           FROM app_actions
           WHERE function_name = $1
             AND credential_id = $2
             AND enabled = 1
           LIMIT 1"#,
    )
    .bind(function_name)
    .bind(credential_id)
    .fetch_optional(db)
    .await
    .map_err(|e| Error::Database(format!("failed to look up action: {e}")))?;

    let Some((action_id, config_str, _cred_id)) = row else {
        tracing::warn!(
            function_name = %function_name,
            credential_id = %credential_id,
            "no action row found for ingest payload"
        );
        return Ok(None);
    };

    let config: serde_json::Value = config_str
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    // 2. Resolve credentials (for now, pass nothing for device credentials —
    //    iOS actions don't need OAuth tokens. The credential_id is enough.)
    let credentials = None;

    // 3. Resolve binary path
    let binary_path = resolve_binary_path(function_name)?;
    if !binary_path.exists() {
        let err = format!("action binary not found: {}", binary_path.display());
        record_failed_run(db, &action_id, function_name, &err).await?;
        return Ok(Some(ActionRunResult {
            status: ActionRunStatus::Failed,
            summary: String::new(),
            stderr: err,
        }));
    }

    // 4. Build stdin JSON
    let input = ActionInput {
        config: &config,
        credentials,
        payload: Some(payload),
    };
    let stdin_bytes = serde_json::to_vec(&input)
        .map_err(|e| Error::Other(format!("failed to serialize action input: {e}")))?;

    // 5. Spawn subprocess with DATABASE_URL inherited from parent env
    let mut child = Command::new(&binary_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Error::Other(format!("failed to spawn action binary: {e}")))?;

    // 6. Write stdin and close it
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(&stdin_bytes)
            .await
            .map_err(|e| Error::Other(format!("failed to write stdin: {e}")))?;
    }

    // 7. Wait for process and collect output
    let output = child
        .wait_with_output()
        .await
        .map_err(|e| Error::Other(format!("failed to wait for action subprocess: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        let err = if stderr.is_empty() {
            format!("subprocess exited with status {}", output.status)
        } else {
            stderr.clone()
        };
        record_failed_run(db, &action_id, function_name, &err).await?;
        return Ok(Some(ActionRunResult {
            status: ActionRunStatus::Failed,
            summary: String::new(),
            stderr,
        }));
    }

    // 8. Parse stdout JSON
    let action_output: ActionOutput = match serde_json::from_str(&stdout) {
        Ok(o) => o,
        Err(e) => {
            let err = format!("failed to parse subprocess stdout JSON: {e}. raw: {}", &stdout[..stdout.len().min(500)]);
            record_failed_run(db, &action_id, function_name, &err).await?;
            return Ok(Some(ActionRunResult {
                status: ActionRunStatus::Failed,
                summary: String::new(),
                stderr,
            }));
        }
    };

    // 9. Save returned config back to app_actions
    let config_json = serde_json::to_string(&action_output.config)
        .map_err(|e| Error::Other(format!("failed to serialize returned config: {e}")))?;
    sqlx::query("UPDATE app_actions SET config = $1, updated_at = datetime('now') WHERE id = $2")
        .bind(&config_json)
        .bind(&action_id)
        .execute(db)
        .await
        .map_err(|e| Error::Database(format!("failed to save action config: {e}")))?;

    // 10. Record successful run
    record_success_run(db, &action_id, function_name, &action_output.result).await?;

    Ok(Some(ActionRunResult {
        status: ActionRunStatus::Success,
        summary: action_output.result,
        stderr,
    }))
}

/// Run a cron action — an action with `credential_id IS NULL` that isn't tied
/// to any specific credential. Examples: `ios_microphone_transcribe`, future
/// background drainers, system maintenance jobs.
///
/// The contract is the same as `run_push_action` except payload is always
/// null. Returns `Ok(None)` if no row exists.
pub async fn run_cron_action(
    db: &SqlitePool,
    function_name: &str,
) -> Result<Option<ActionRunResult>> {
    let row: Option<(String, Option<String>)> = sqlx::query_as(
        r#"SELECT id, config
           FROM app_actions
           WHERE function_name = $1
             AND credential_id IS NULL
             AND enabled = 1
           LIMIT 1"#,
    )
    .bind(function_name)
    .fetch_optional(db)
    .await
    .map_err(|e| Error::Database(format!("failed to look up cron action: {e}")))?;

    let Some((action_id, config_str)) = row else {
        return Ok(None);
    };

    let config: serde_json::Value = config_str
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    let binary_path = resolve_binary_path(function_name)?;
    if !binary_path.exists() {
        let err = format!("action binary not found: {}", binary_path.display());
        record_failed_run(db, &action_id, function_name, &err).await?;
        return Ok(Some(ActionRunResult {
            status: ActionRunStatus::Failed,
            summary: String::new(),
            stderr: err,
        }));
    }

    let input = ActionInput {
        config: &config,
        credentials: None,
        payload: None,
    };
    let stdin_bytes = serde_json::to_vec(&input)
        .map_err(|e| Error::Other(format!("failed to serialize action input: {e}")))?;

    let mut child = Command::new(&binary_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Error::Other(format!("failed to spawn cron action binary: {e}")))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(&stdin_bytes)
            .await
            .map_err(|e| Error::Other(format!("failed to write stdin: {e}")))?;
    }

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| Error::Other(format!("failed to wait for cron subprocess: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        let err = if stderr.is_empty() {
            format!("subprocess exited with status {}", output.status)
        } else {
            stderr.clone()
        };
        record_failed_run(db, &action_id, function_name, &err).await?;
        return Ok(Some(ActionRunResult {
            status: ActionRunStatus::Failed,
            summary: String::new(),
            stderr,
        }));
    }

    let action_output: ActionOutput = match serde_json::from_str(&stdout) {
        Ok(o) => o,
        Err(e) => {
            let err = format!(
                "failed to parse cron subprocess stdout JSON: {e}. raw: {}",
                &stdout[..stdout.len().min(500)]
            );
            record_failed_run(db, &action_id, function_name, &err).await?;
            return Ok(Some(ActionRunResult {
                status: ActionRunStatus::Failed,
                summary: String::new(),
                stderr,
            }));
        }
    };

    let config_json = serde_json::to_string(&action_output.config)
        .map_err(|e| Error::Other(format!("failed to serialize returned config: {e}")))?;
    sqlx::query("UPDATE app_actions SET config = $1, updated_at = datetime('now') WHERE id = $2")
        .bind(&config_json)
        .bind(&action_id)
        .execute(db)
        .await
        .map_err(|e| Error::Database(format!("failed to save action config: {e}")))?;

    record_success_run(db, &action_id, function_name, &action_output.result).await?;

    Ok(Some(ActionRunResult {
        status: ActionRunStatus::Success,
        summary: action_output.result,
        stderr,
    }))
}

/// Resolve a function_name to a binary path.
///
/// Convention: `actions/target/{debug,release}/{function_name}`
/// — checks release first, falls back to debug for development.
fn resolve_binary_path(function_name: &str) -> Result<PathBuf> {
    // Allow override via env var for production deployments
    if let Ok(actions_dir) = std::env::var("VIRTUES_ACTIONS_BIN_DIR") {
        return Ok(PathBuf::from(actions_dir).join(function_name));
    }

    // Workspace target dir is ${REPO_ROOT}/target
    // We're typically run from the repo root or from core/, so we walk up.
    let cwd = std::env::current_dir()
        .map_err(|e| Error::Other(format!("failed to get cwd: {e}")))?;

    // Try cwd/target/release first
    for ancestor in cwd.ancestors() {
        let release = ancestor.join("target").join("release").join(function_name);
        if release.exists() {
            return Ok(release);
        }
        let debug = ancestor.join("target").join("debug").join(function_name);
        if debug.exists() {
            return Ok(debug);
        }
    }

    // Fallback: relative to cwd
    Ok(PathBuf::from("target/debug").join(function_name))
}

async fn record_success_run(
    db: &SqlitePool,
    action_id: &str,
    function_name: &str,
    summary: &str,
) -> Result<()> {
    record_run(db, action_id, function_name, "success", Some(summary), None).await
}

async fn record_failed_run(
    db: &SqlitePool,
    action_id: &str,
    function_name: &str,
    error: &str,
) -> Result<()> {
    record_run(db, action_id, function_name, "error", None, Some(error)).await
}

async fn record_run(
    db: &SqlitePool,
    action_id: &str,
    function_name: &str,
    status: &str,
    summary: Option<&str>,
    error: Option<&str>,
) -> Result<()> {
    let run_id = format!(
        "run_{}_{}",
        function_name,
        chrono::Utc::now().timestamp_micros()
    );
    // Cron actions don't have a credential, so we infer trigger from the
    // function_name suffix. This is a heuristic — fine for now since the only
    // cron action is the transcription resolver. When the scheduler refactor
    // happens, this goes away (the trigger comes from the dispatch site).
    let trigger = if function_name.ends_with("_resolution") {
        "cron"
    } else {
        "push"
    };
    let _ = sqlx::query(
        r#"INSERT INTO app_action_runs (id, action_id, status, started_at, completed_at, result_summary, error, trigger)
           VALUES ($1, $2, $3, datetime('now'), datetime('now'), $4, $5, $6)"#,
    )
    .bind(&run_id)
    .bind(action_id)
    .bind(status)
    .bind(summary)
    .bind(error)
    .bind(trigger)
    .execute(db)
    .await
    .map_err(|e| {
        tracing::warn!(error = %e, "failed to record action run");
        e
    });
    Ok(())
}

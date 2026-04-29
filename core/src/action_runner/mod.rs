//! Unified action runner.
//!
//! Single entry point for every action execution — cron, manual, webhook, api,
//! or tool-call. Dispatches to a subprocess binary, an LLM agent loop, or both
//! based on which fields are populated on the action row.
//!
//! ## Dispatch flow
//!
//! 1. Fetch action. Not found / disabled → return `NotFound`.
//! 2. Validate `trigger` is in `action.triggers`. Otherwise → `Forbidden`.
//! 3. Evaluate `condition` SQL expression if set. Falsy → record `skipped` run.
//! 4. Concurrency gate (skip if previous run still active, unless parallel).
//! 5. Create `running` run row.
//! 6. Resolve credentials from the `credentials` Vault and decrypt secrets
//!    (if `credential_id` set). Subprocess receives plaintext JSON.
//! 7. **Subprocess phase**: if `function_name` set, spawn binary, pipe stdin
//!    JSON, read stdout JSON, save returned config back to `app_actions.config`.
//! 8. **Agent phase**: if `agent` (instruction) set, run LLM agent loop with the
//!    subprocess result as context.
//! 9. Complete run row with final status + result summary.
//!
//! ## Subprocess contract
//!
//! Stdin (one JSON object):
//! ```json
//! { "config": { ... }, "credentials": { ... } | null, "payload": [...] | null }
//! ```
//!
//! Stdout (one JSON object):
//! ```json
//! { "result": "summary string", "config": { ... } }
//! ```
//!
//! Exit 0 = success. Exit non-0 = failure; stderr becomes the run error.

use sqlx::SqlitePool;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::error::{Error, Result};
use crate::scheduler::actions::{self, Action};
use crate::server::yjs::YjsState;

/// Dependencies threaded into the runner. Cheap to clone; holds references.
#[derive(Clone)]
pub struct RunnerDeps {
    pub db: SqlitePool,
    pub yjs: YjsState,
}

// Subprocess contract types live in `virtues_helpers::contract`. Re-export so
// existing call sites (`action_runner::ActionInput`) keep working.
pub use virtues_helpers::contract::{ActionInput, ActionOutput};

/// Outcome of a single action run.
#[derive(Debug)]
pub struct ActionRunResult {
    pub run_id: Option<String>,
    pub status: ActionRunStatus,
    pub summary: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionRunStatus {
    Success,
    Failed,
    Skipped,
    NotFound,
    Forbidden,
}

impl ActionRunResult {
    fn not_found() -> Self {
        Self {
            run_id: None,
            status: ActionRunStatus::NotFound,
            summary: String::new(),
            error: None,
        }
    }

    fn forbidden(msg: impl Into<String>) -> Self {
        Self {
            run_id: None,
            status: ActionRunStatus::Forbidden,
            summary: String::new(),
            error: Some(msg.into()),
        }
    }
}

/// Run an action end-to-end. The single dispatch entry point.
pub async fn run_action(
    deps: &RunnerDeps,
    action_id: &str,
    trigger: &str,
    payload: Option<&serde_json::Value>,
) -> Result<ActionRunResult> {
    // 1. Fetch action
    let action = match actions::get_action(&deps.db, action_id).await {
        Ok(a) if a.enabled => a,
        Ok(_) => {
            tracing::warn!(action_id, "action is disabled, ignoring run request");
            return Ok(ActionRunResult::not_found());
        }
        Err(e) => {
            tracing::warn!(action_id, error = %e, "action not found");
            return Ok(ActionRunResult::not_found());
        }
    };

    // 2. Triggers validation
    if !action.triggers.iter().any(|t| t == trigger) {
        tracing::warn!(
            action_id,
            trigger,
            allowed = ?action.triggers,
            "trigger not allowed for this action"
        );
        return Ok(ActionRunResult::forbidden(format!(
            "trigger '{}' not in allowed list {:?}",
            trigger, action.triggers
        )));
    }

    // 2b. Webhook invariant: every webhook-triggered dispatch must resolve to
    // an action with a credential_id. Reconcile validates this at startup;
    // this is the defensive belt-and-suspenders check.
    if trigger == "webhook" && action.credential_id.is_none() {
        tracing::error!(
            action_id,
            "webhook trigger on action with no credential_id — rejected"
        );
        return Ok(ActionRunResult::forbidden(
            "webhook trigger requires credential_id".to_string(),
        ));
    }

    // 3. Condition (SQL expression gate). Evaluate BEFORE creating a run row
    //    so noisy gates (e.g., "hourly cron that only fires once a day")
    //    don't spam `app_action_runs` with 23 skipped rows per day.
    //
    //    DEPRECATED FORMAT — raw SQL is fragile (limited expressiveness for
    //    timezone math, injection-by-design if untrusted). A named-evaluator
    //    registry is planned; see action_runner audit notes.
    if let Some(condition) = &action.condition {
        if !condition.trim().is_empty() {
            match eval_condition(&deps.db, condition).await {
                Ok(false) => {
                    tracing::debug!(action_id, "condition falsy, skipping silently");
                    return Ok(ActionRunResult {
                        run_id: None,
                        status: ActionRunStatus::Skipped,
                        summary: "condition evaluated false".to_string(),
                        error: None,
                    });
                }
                Ok(true) => {}
                Err(e) => {
                    // Record evaluation errors — they are bugs, not noise.
                    tracing::error!(action_id, error = %e, "condition evaluation failed");
                    let run =
                        actions::create_run(&deps.db, Some(&action.id), trigger).await?;
                    let msg = format!("condition evaluation error: {e}");
                    actions::complete_run(
                        &deps.db, &run.id, "error", 0, Some(&msg), None,
                    )
                    .await?;
                    return Ok(ActionRunResult {
                        run_id: Some(run.id),
                        status: ActionRunStatus::Failed,
                        summary: String::new(),
                        error: Some(msg),
                    });
                }
            }
        }
    }

    // 4. Concurrency gate — skip if a previous run is still active. No run
    //    row created for concurrency skips either (same noise logic as #3).
    if actions::has_active_run(&deps.db, &action.id)
        .await
        .unwrap_or(false)
    {
        tracing::info!(action_id, "previous run still active; skipping");
        return Ok(ActionRunResult {
            run_id: None,
            status: ActionRunStatus::Skipped,
            summary: "previous run still active".to_string(),
            error: None,
        });
    }

    // 5. Create run row (status='running')
    let run = actions::create_run(&deps.db, Some(&action.id), trigger).await?;

    // 6. Resolve credentials. Hard-fail if a credential_id is set but fetch
    //    errors — a subprocess running without credentials it was expecting
    //    to receive would silently produce bad data.
    let credentials = if let Some(cred_id) = &action.credential_id {
        match load_credentials(&deps.db, cred_id).await {
            Ok(c) => Some(c),
            Err(e) => {
                let msg = format!("failed to load credential {cred_id}: {e}");
                tracing::error!(action_id, error = %msg, "credential load failed");
                actions::complete_run(&deps.db, &run.id, "error", 0, Some(&msg), None).await?;
                return Ok(ActionRunResult {
                    run_id: Some(run.id),
                    status: ActionRunStatus::Failed,
                    summary: String::new(),
                    error: Some(msg),
                });
            }
        }
    } else {
        None
    };

    // 7. Subprocess phase
    let mut subprocess_summary: Option<String> = None;
    if let Some(fn_name) = &action.function_name {
        match run_subprocess(&deps.db, &action, fn_name, credentials.clone(), payload).await {
            Ok(summary) => {
                subprocess_summary = Some(summary);
            }
            Err(e) => {
                let msg = e.to_string();
                tracing::error!(action_id, error = %msg, "subprocess phase failed");
                actions::complete_run(&deps.db, &run.id, "error", 0, Some(&msg), None).await?;
                return Ok(ActionRunResult {
                    run_id: Some(run.id),
                    status: ActionRunStatus::Failed,
                    summary: String::new(),
                    error: Some(msg),
                });
            }
        }
    }

    // 8. Agent phase
    if let Some(prompt) = action.agent.as_ref().filter(|s| !s.trim().is_empty()) {
        let ctx = subprocess_summary.as_deref();
        match crate::agent::action_runner::run_agent_loop(
            &deps.db,
            &deps.yjs,
            &action,
            prompt,
            ctx,
        )
        .await
        {
            Ok(agent_result) => {
                let steps = agent_result.steps as i64;
                let summary = agent_result
                    .message
                    .clone()
                    .or(subprocess_summary.clone())
                    .unwrap_or_default();
                actions::complete_run(
                    &deps.db,
                    &run.id,
                    "success",
                    steps,
                    None,
                    Some(&summary),
                )
                .await?;
                return Ok(ActionRunResult {
                    run_id: Some(run.id),
                    status: ActionRunStatus::Success,
                    summary,
                    error: None,
                });
            }
            Err(e) => {
                let msg = e.to_string();
                tracing::error!(action_id, error = %msg, "agent phase failed");
                actions::complete_run(&deps.db, &run.id, "error", 0, Some(&msg), None).await?;
                return Ok(ActionRunResult {
                    run_id: Some(run.id),
                    status: ActionRunStatus::Failed,
                    summary: String::new(),
                    error: Some(msg),
                });
            }
        }
    }

    // 9. Complete run
    let summary = subprocess_summary.unwrap_or_default();
    actions::complete_run(
        &deps.db,
        &run.id,
        "success",
        0,
        None,
        Some(&summary),
    )
    .await?;
    Ok(ActionRunResult {
        run_id: Some(run.id),
        status: ActionRunStatus::Success,
        summary,
        error: None,
    })
}

// ============================================================================
// Condition evaluation
// ============================================================================

/// Evaluate a SQL condition expression. Returns true if the expression is
/// truthy, false otherwise. A truthy result is any non-zero, non-empty,
/// non-null value.
async fn eval_condition(db: &SqlitePool, condition: &str) -> Result<bool> {
    let sql = format!("SELECT ({}) AS result", condition);
    let result: Option<i64> = sqlx::query_scalar(&sql)
        .fetch_optional(db)
        .await
        .map_err(|e| Error::Database(format!("condition sql failed: {e}")))?;
    Ok(result.map(|v| v != 0).unwrap_or(false))
}

// ============================================================================
// Credentials
// ============================================================================

/// Load a credential and hand the subprocess a fully-decrypted view.
///
/// The returned JSON shape matches the connectors charter: subprocess sees
/// plaintext `secrets` (shape per the connector manifest) plus identity and
/// metadata. The master encryption key never crosses the subprocess boundary.
///
/// ```json
/// {
///   "id": "cred_...",
///   "source_id": "ios",
///   "secrets": { "token": "<plaintext>" },
///   "metadata": { "device_id": "...", ... }
/// }
/// ```
async fn load_credentials(db: &SqlitePool, credential_id: &str) -> Result<serde_json::Value> {
    let row: Option<(String, String, String)> = sqlx::query_as(
        r#"SELECT source_id, secrets_ciphertext, metadata
             FROM credentials
            WHERE id = ? AND status = 'active'"#,
    )
    .bind(credential_id)
    .fetch_optional(db)
    .await
    .map_err(|e| Error::Database(format!("failed to load credential: {e}")))?;

    let Some((source_id, secrets_ciphertext, metadata_raw)) = row else {
        return Err(Error::NotFound(format!(
            "credential not found or not active: {credential_id}"
        )));
    };

    // TODO: call ensure_fresh(credential) here once OAuth connectors land.
    // For self_issued_bearer (iOS) there's nothing to refresh.

    let encryptor = crate::crypto::TokenEncryptor::from_env()?;
    let secrets_plaintext = encryptor.decrypt(&secrets_ciphertext).map_err(|e| {
        Error::Other(format!(
            "failed to decrypt credential {credential_id}: {e}"
        ))
    })?;
    let secrets: serde_json::Value = serde_json::from_str(&secrets_plaintext)
        .unwrap_or_else(|_| serde_json::json!({}));

    let metadata: serde_json::Value =
        serde_json::from_str(&metadata_raw).unwrap_or_else(|_| serde_json::json!({}));

    Ok(serde_json::json!({
        "id": credential_id,
        "source_id": source_id,
        "secrets": secrets,
        "metadata": metadata,
    }))
}

// ============================================================================
// Subprocess phase
// ============================================================================

async fn run_subprocess(
    db: &SqlitePool,
    action: &Action,
    function_name: &str,
    credentials: Option<serde_json::Value>,
    payload: Option<&serde_json::Value>,
) -> Result<String> {
    let binary_path = resolve_binary_path(function_name)?;
    if !binary_path.exists() {
        return Err(Error::Other(format!(
            "action binary not found: {}",
            binary_path.display()
        )));
    }

    let input = ActionInput {
        config: action.config.clone(),
        credentials,
        payload: payload.cloned(),
    };
    let stdin_bytes = serde_json::to_vec(&input)
        .map_err(|e| Error::Other(format!("failed to serialize action input: {e}")))?;

    let mut child = Command::new(&binary_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Error::Other(format!("failed to spawn action binary: {e}")))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(&stdin_bytes)
            .await
            .map_err(|e| Error::Other(format!("failed to write stdin: {e}")))?;
    }

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| Error::Other(format!("failed to wait for action subprocess: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Err(Error::Other(if stderr.is_empty() {
            format!("subprocess exited with status {}", output.status)
        } else {
            stderr
        }));
    }

    let action_output: ActionOutput = serde_json::from_str(&stdout).map_err(|e| {
        Error::Other(format!(
            "failed to parse subprocess stdout JSON: {e}. raw: {}",
            &stdout[..stdout.len().min(500)]
        ))
    })?;

    // Persist returned config back to the action row
    let config_json = serde_json::to_string(&action_output.config)
        .map_err(|e| Error::Other(format!("failed to serialize returned config: {e}")))?;
    sqlx::query("UPDATE app_actions SET config = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(&config_json)
        .bind(&action.id)
        .execute(db)
        .await
        .map_err(|e| Error::Database(format!("failed to save action config: {e}")))?;

    Ok(action_output.result)
}

/// Resolve a function_name to a binary path.
///
/// Convention: `target/{debug,release}/{function_name}` walking up from cwd.
/// Override via `VIRTUES_ACTIONS_BIN_DIR` env var for production deployments.
fn resolve_binary_path(function_name: &str) -> Result<PathBuf> {
    if let Ok(actions_dir) = std::env::var("VIRTUES_ACTIONS_BIN_DIR") {
        return Ok(PathBuf::from(actions_dir).join(function_name));
    }

    let cwd = std::env::current_dir()
        .map_err(|e| Error::Other(format!("failed to get cwd: {e}")))?;

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

    Ok(PathBuf::from("target/debug").join(function_name))
}

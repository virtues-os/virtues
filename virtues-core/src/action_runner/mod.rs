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
//! 7. **Subprocess phase**: if `command` set, resolve argv[0] + spawn, pipe stdin
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

use sqlx::PgPool;
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
    pub db: PgPool,
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
    /// Run row was created and execution was spawned on a detached task.
    /// Used only by `run_action_detached` so the HTTP handler can return
    /// immediately with the run_id while the subprocess/agent finishes.
    Running,
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

/// Outcome of `prepare_run`: either an early-exit result (no/short execution
/// needed) or a created run row that the caller should drive to completion.
enum PrepareOutcome {
    Early(ActionRunResult),
    Ready { action: Action, run_id: String },
}

/// Run an action end-to-end. The single inline-await dispatch entry point.
///
/// Used by cron, webhooks, and tool-call paths that want to await the full
/// result. The HTTP handler uses `run_action_detached` instead so a client
/// disconnect can't drop the future mid-subprocess.
pub async fn run_action(
    deps: &RunnerDeps,
    action_id: &str,
    trigger: &str,
    payload: Option<&serde_json::Value>,
) -> Result<ActionRunResult> {
    match prepare_run(deps, action_id, trigger).await? {
        PrepareOutcome::Early(result) => Ok(result),
        PrepareOutcome::Ready { action, run_id } => {
            let payload = payload.cloned();
            Ok(execute_prepared(deps.clone(), action, run_id, payload).await)
        }
    }
}

/// Run an action with the heavy phase (credentials → subprocess → agent →
/// complete_run) detached on a `tokio::spawn` task. Returns immediately once
/// the run row is created so a cancelled HTTP request can't orphan the row.
///
/// Returns `Running` status with the new `run_id` for the happy path; early
/// exits (not_found / forbidden / condition-skipped / concurrency-skipped /
/// view-runtime) are unchanged from `run_action`.
pub async fn run_action_detached(
    deps: &RunnerDeps,
    action_id: &str,
    trigger: &str,
    payload: Option<&serde_json::Value>,
) -> Result<ActionRunResult> {
    match prepare_run(deps, action_id, trigger).await? {
        PrepareOutcome::Early(result) => Ok(result),
        PrepareOutcome::Ready { action, run_id } => {
            let payload = payload.cloned();
            let deps_owned = deps.clone();
            let run_id_for_task = run_id.clone();
            tokio::spawn(async move {
                let _ = execute_prepared(deps_owned, action, run_id_for_task, payload).await;
            });
            Ok(ActionRunResult {
                run_id: Some(run_id),
                status: ActionRunStatus::Running,
                summary: String::new(),
                error: None,
            })
        }
    }
}

/// Steps 1–5 of the dispatch flow. Resolves the action, validates trigger and
/// runtime gates, evaluates the condition, checks the concurrency gate, and
/// creates the `running` run row. All early exits short-circuit here so the
/// caller can decide whether to await execution inline or detach it.
async fn prepare_run(
    deps: &RunnerDeps,
    action_id: &str,
    trigger: &str,
) -> Result<PrepareOutcome> {
    // 1. Fetch action
    let action = match actions::get_action(&deps.db, action_id).await {
        Ok(a) if a.enabled => a,
        Ok(_) => {
            tracing::warn!(action_id, "action is disabled, ignoring run request");
            return Ok(PrepareOutcome::Early(ActionRunResult::not_found()));
        }
        Err(e) => {
            tracing::warn!(action_id, error = %e, "action not found");
            return Ok(PrepareOutcome::Early(ActionRunResult::not_found()));
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
        return Ok(PrepareOutcome::Early(ActionRunResult::forbidden(format!(
            "trigger '{}' not in allowed list {:?}",
            trigger, action.triggers
        ))));
    }

    // 2a. `view`-runtime actions are pure-frontend renderers; never invoked
    // server-side. Skip silently so cron ticks don't churn the runs table.
    if action.runtime == "view" {
        tracing::debug!(action_id, "view runtime — never invoked server-side");
        return Ok(PrepareOutcome::Early(ActionRunResult {
            run_id: None,
            status: ActionRunStatus::Skipped,
            summary: "view runtime — not server-invoked".to_string(),
            error: None,
        }));
    }

    // 2b. Webhook invariant: a webhook action must resolve to an identity that
    // authorizes the post — a device_id (device ingest, keyed on the proven iroh
    // key) or a credential_id (OAuth/api). One or the other must be set.
    if trigger == "webhook" && action.credential_id.is_none() && action.device_id.is_none() {
        tracing::error!(
            action_id,
            "webhook trigger on action with no device_id or credential_id — rejected"
        );
        return Ok(PrepareOutcome::Early(ActionRunResult::forbidden(
            "webhook trigger requires a device_id or credential_id".to_string(),
        )));
    }

    // 3. Condition (SQL gate). Evaluate before creating a run row.
    if let Some(condition) = &action.condition {
        if !condition.trim().is_empty() {
            match eval_condition(&deps.db, condition).await {
                Ok(false) => {
                    tracing::debug!(action_id, "condition falsy, skipping silently");
                    return Ok(PrepareOutcome::Early(ActionRunResult {
                        run_id: None,
                        status: ActionRunStatus::Skipped,
                        summary: "condition evaluated false".to_string(),
                        error: None,
                    }));
                }
                Ok(true) => {}
                Err(e) => {
                    tracing::error!(action_id, error = %e, "condition evaluation failed");
                    let run = actions::create_run(&deps.db, Some(&action.id), trigger).await?;
                    let msg = format!("condition evaluation error: {e}");
                    actions::complete_run(&deps.db, &run.id, "error", 0, Some(&msg), None)
                        .await?;
                    return Ok(PrepareOutcome::Early(ActionRunResult {
                        run_id: Some(run.id),
                        status: ActionRunStatus::Failed,
                        summary: String::new(),
                        error: Some(msg),
                    }));
                }
            }
        }
    }

    // 4. Concurrency gate — every applet is a singleton. Unlike a falsy
    // condition (silent by design: frequent polls would flood run history),
    // an overlap skip is rare and diagnostically important, so it records a
    // real `skipped` run row per the singleton doctrine.
    if actions::has_active_run(&deps.db, &action.id)
        .await
        .unwrap_or(false)
    {
        tracing::info!(action_id, "previous run still active; skipping");
        let run = actions::create_run(&deps.db, Some(&action.id), trigger).await?;
        actions::complete_run(
            &deps.db,
            &run.id,
            "skipped",
            0,
            None,
            Some("skipped — previous run still active"),
        )
        .await?;
        return Ok(PrepareOutcome::Early(ActionRunResult {
            run_id: Some(run.id),
            status: ActionRunStatus::Skipped,
            summary: "previous run still active".to_string(),
            error: None,
        }));
    }

    // 5. Create run row.
    let run = actions::create_run(&deps.db, Some(&action.id), trigger).await?;
    Ok(PrepareOutcome::Ready {
        action,
        run_id: run.id,
    })
}

/// Steps 6–9 of the dispatch flow. Loads credentials, runs the subprocess
/// and/or agent phase, and persists the final run state. Errors are recorded
/// against the run row rather than propagated, so this function always returns
/// an `ActionRunResult` and is safe to detach.
async fn execute_prepared(
    deps: RunnerDeps,
    action: Action,
    run_id: String,
    payload: Option<serde_json::Value>,
) -> ActionRunResult {
    let action_id = action.id.clone();

    // Helper: persist `error` status and return a Failed result. Logs and
    // swallows DB errors from `complete_run` since at this point we have
    // nowhere to propagate them — the caller may already be detached.
    async fn fail(
        deps: &RunnerDeps,
        run_id: &str,
        action_id: &str,
        msg: String,
    ) -> ActionRunResult {
        if let Err(e) =
            actions::complete_run(&deps.db, run_id, "error", 0, Some(&msg), None).await
        {
            tracing::error!(action_id, error = %e, "complete_run failed while recording error");
        }
        ActionRunResult {
            run_id: Some(run_id.to_string()),
            status: ActionRunStatus::Failed,
            summary: String::new(),
            error: Some(msg),
        }
    }

    // 6. Resolve credentials.
    let credentials = if let Some(cred_id) = &action.credential_id {
        match load_credentials(&deps.db, cred_id).await {
            Ok(c) => Some(c),
            Err(e) => {
                let msg = format!("failed to load credential {cred_id}: {e}");
                tracing::error!(action_id, error = %msg, "credential load failed");
                return fail(&deps, &run_id, &action_id, msg).await;
            }
        }
    } else {
        None
    };

    // 7. Subprocess phase.
    let mut subprocess_summary: Option<String> = None;
    let mut subprocess_records: i64 = 0;
    let has_command = action.command.as_ref().is_some_and(|c| !c.is_empty());
    if action.runtime == "service" && has_command {
        match run_app_trigger(&action, payload.as_ref()).await {
            Ok(summary) => {
                subprocess_summary = summary;
            }
            Err(e) => {
                let msg = e.to_string();
                tracing::error!(action_id, error = %msg, "app trigger failed");
                return fail(&deps, &run_id, &action_id, msg).await;
            }
        }
    } else if has_command {
        let command = action.command.clone().unwrap();
        match run_subprocess(
            &deps.db,
            &action,
            &command,
            credentials.clone(),
            payload.as_ref(),
        )
        .await
        {
            Ok(outcome) => {
                subprocess_summary = Some(outcome.summary);
                subprocess_records = outcome.records;
            }
            Err(e) => {
                let msg = e.to_string();
                tracing::error!(action_id, error = %msg, "subprocess phase failed");
                return fail(&deps, &run_id, &action_id, msg).await;
            }
        }
    }

    // 8. Agent phase.
    if let Some(prompt) = action.agent.as_ref().filter(|s| !s.trim().is_empty()) {
        let ctx = subprocess_summary.as_deref();
        match crate::agent::action_runner::run_agent_loop(
            &deps.db, &deps.yjs, &action, prompt, ctx,
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
                if let Err(e) =
                    actions::complete_run(&deps.db, &run_id, "success", steps, None, Some(&summary))
                        .await
                {
                    tracing::error!(action_id, error = %e, "complete_run failed after agent success");
                }
                maybe_archive_on_until(&deps.db, &action).await;
                return ActionRunResult {
                    run_id: Some(run_id),
                    status: ActionRunStatus::Success,
                    summary,
                    error: None,
                };
            }
            Err(e) => {
                let msg = e.to_string();
                tracing::error!(action_id, error = %msg, "agent phase failed");
                return fail(&deps, &run_id, &action_id, msg).await;
            }
        }
    }

    // 9. Complete run.
    let summary = subprocess_summary.unwrap_or_default();
    if let Err(e) =
        actions::complete_run(&deps.db, &run_id, "success", subprocess_records, None, Some(&summary)).await
    {
        tracing::error!(action_id, error = %e, "complete_run failed at end of run");
    }
    maybe_archive_on_until(&deps.db, &action).await;
    ActionRunResult {
        run_id: Some(run_id),
        status: ActionRunStatus::Success,
        summary,
        error: None,
    }
}

// ============================================================================
// Lifecycle
// ============================================================================

/// Post-success lifecycle check. `until` semantics: NULL/empty = forever;
/// the literal `once` = archive after this (first) success; anything else =
/// a SQL boolean, archive when it evaluates true. Evaluation reuses the
/// hardened `eval_condition` path (read-only tx, timeout, local timezone).
/// Failures are logged, never fatal — a broken `until` must not fail a run
/// that already succeeded.
async fn maybe_archive_on_until(db: &PgPool, action: &Action) {
    let Some(until) = action.until.as_deref().map(str::trim) else {
        return;
    };
    if until.is_empty() {
        return;
    }
    let done = if until.eq_ignore_ascii_case("once") {
        true
    } else {
        match eval_condition(db, until).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(action_id = %action.id, error = %e, "until evaluation failed; not archiving");
                false
            }
        }
    };
    if done {
        tracing::info!(action_id = %action.id, "lifecycle complete (until met); archiving");
        if let Err(e) = actions::archive_action(db, &action.id).await {
            tracing::error!(action_id = %action.id, error = %e, "archive_action failed");
        }
    }
}

// ============================================================================
// Condition evaluation
// ============================================================================

/// Evaluate a SQL condition expression. Returns true if the expression is
/// truthy, false otherwise (NULL is false).
///
/// The expression is LLM/user-authored, so it runs hardened: inside a
/// READ ONLY transaction (rolled back regardless), under a short
/// statement_timeout, with the session timezone set to the box's
/// home_timezone so clock-based gates like
/// `extract(hour from now()) < 6` mean the user's local time, not UTC.
/// The result is cast to boolean, so both boolean and integer expressions
/// evaluate correctly.
async fn eval_condition(db: &PgPool, condition: &str) -> Result<bool> {
    let mut tx = db
        .begin()
        .await
        .map_err(|e| Error::Database(format!("condition tx begin failed: {e}")))?;
    sqlx::query("SET TRANSACTION READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("condition read-only set failed: {e}")))?;
    sqlx::query("SET LOCAL statement_timeout = '5s'")
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("condition timeout set failed: {e}")))?;
    sqlx::query(
        "SELECT set_config('timezone', COALESCE(\
             (SELECT home_timezone FROM app_user_profile LIMIT 1), \
             current_setting('timezone')), true)",
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| Error::Database(format!("condition timezone set failed: {e}")))?;

    let sql = format!("SELECT (({}))::boolean AS result", condition);
    let result: Option<bool> = sqlx::query_scalar(&sql)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("condition sql failed: {e}")))?;
    tx.rollback().await.ok();
    Ok(result.unwrap_or(false))
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
async fn load_credentials(db: &PgPool, credential_id: &str) -> Result<serde_json::Value> {
    // Just-in-time refresh: if the access token is expired or about to expire,
    // call the proxy now so the subprocess always sees a valid token. No-op
    // for kinds without `expires_at` (api_key, self_issued_bearer, Plaid).
    // Errors here surface as a failed dispatch — better than handing the
    // subprocess a dead token and watching it 401.
    virtues_helpers::auth::ensure_fresh(db, credential_id)
        .await
        .map_err(|e| Error::Other(format!("credential refresh failed for {credential_id}: {e}")))?;

    // `metadata` is a JSONB column — decode it straight into a `Value`, not a
    // `String`. (Pre-Postgres it was a TEXT JSON string read via `from_str`;
    // the migration to JSONB made that decode fail with "Rust type String is
    // not compatible with SQL type JSONB", which silently broke every ingest.)
    // `secrets_ciphertext` stays TEXT (it's opaque ciphertext, not JSON).
    let row: Option<(String, String, serde_json::Value)> = sqlx::query_as(
        r#"SELECT source_id, secrets_ciphertext, metadata
             FROM credentials
            WHERE id = $1 AND status = 'active'"#,
    )
    .bind(credential_id)
    .fetch_optional(db)
    .await
    .map_err(|e| Error::Database(format!("failed to load credential: {e}")))?;

    let Some((source_id, secrets_ciphertext, metadata_value)) = row else {
        return Err(Error::NotFound(format!(
            "credential not found or not active: {credential_id}"
        )));
    };

    let encryptor = crate::crypto::TokenEncryptor::from_env()?;
    let secrets_plaintext = encryptor.decrypt(&secrets_ciphertext).map_err(|e| {
        Error::Other(format!(
            "failed to decrypt credential {credential_id}: {e}"
        ))
    })?;
    let secrets: serde_json::Value = serde_json::from_str(&secrets_plaintext)
        .unwrap_or_else(|_| serde_json::json!({}));

    // Normalize JSON `null` (or a non-object) to `{}` so downstream always sees
    // an object, matching the prior `from_str(...).unwrap_or({})` behavior.
    let metadata = if metadata_value.is_object() {
        metadata_value
    } else {
        serde_json::json!({})
    };

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

/// Dispatch a trigger to an `app`-runtime action via core's own proxy.
///
/// The supervised app is listening on a private port; we POST the
/// `ActionInput` JSON to `http://127.0.0.1:<api_port>/service/<action_id>/__trigger`
/// and let the proxy handler resolve the port. This keeps the runner from
/// needing a direct handle to `ServiceSupervisor` and avoids passing it through
/// every cron tick.
///
/// Conventions:
///   - 200 with optional `result` field → Success; `result` becomes summary.
///   - 404 → app doesn't implement `/__trigger` (UI-only app); treat as
///     Skipped (Ok(None) summary, no error).
///   - 503 → app not ready; surface as a soft error.
///   - other 4xx/5xx → error with body as message.
async fn run_app_trigger(
    action: &Action,
    payload: Option<&serde_json::Value>,
) -> Result<Option<String>> {
    let api_port = std::env::var("PORT").unwrap_or_else(|_| "8000".to_string());
    let url = format!(
        "http://127.0.0.1:{api_port}/service/{}/__trigger",
        action.id
    );

    let body = serde_json::json!({
        "config": action.config,
        "credential_id": action.credential_id,
        "payload": payload,
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| Error::Other(format!("build reqwest client: {e}")))?;

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| Error::Other(format!("POST {url} failed: {e}")))?;

    let status = resp.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        // App didn't implement `/__trigger`. Not an error — UI-only apps
        // don't need to handle cron/webhook fires.
        tracing::debug!(
            action_id = %action.id,
            "app has no /__trigger handler; nothing to do"
        );
        return Ok(None);
    }
    if !status.is_success() {
        let body_text = resp.text().await.unwrap_or_default();
        return Err(Error::Other(format!(
            "app /__trigger returned {}: {}",
            status.as_u16(),
            body_text
        )));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| Error::Other(format!("/__trigger non-JSON: {e}")))?;

    // Pull a summary string from the response if present.
    let summary = json
        .get("result")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| Some(json.to_string()));

    Ok(summary)
}

/// Hard ceiling on a single action subprocess. Generous enough for the largest
/// legitimate batch, short enough that a hung/wedged process frees the per-action
/// run lock instead of blocking the action until the box restarts. (A device
/// upload gives up far sooner; the box still finishes idempotently if it can.)
const SUBPROCESS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// Per-action ceiling override. The embedding indexer drains its whole backlog
/// in one run (see `search::indexer`) — initial onboarding embeds an entire
/// corpus, which is hours of legitimate work, not a hang. Its drain loop
/// enforces its own 2-hour wall-clock ceiling and commits per record, so the
/// subprocess gets that ceiling plus margin and the internal limit exits
/// cleanly rather than being SIGKILLed mid-drain. Keyed on the manifest dir
/// (stable), not the id (which gets collision-suffixed on rename).
fn subprocess_timeout(action: &Action) -> std::time::Duration {
    match action.dir.as_str() {
        "embedding_index" => std::time::Duration::from_secs(2 * 3600 + 300),
        _ => SUBPROCESS_TIMEOUT,
    }
}

/// What a successful subprocess phase produced: the one-line summary plus the
/// processed-record count (for `app_action_runs.records_processed`).
struct SubprocessOutcome {
    summary: String,
    records: i64,
}

async fn run_subprocess(
    db: &PgPool,
    action: &Action,
    command: &[String],
    credentials: Option<serde_json::Value>,
    payload: Option<&serde_json::Value>,
) -> Result<SubprocessOutcome> {
    let argv0 = command
        .first()
        .ok_or_else(|| Error::Other("action command is empty".to_string()))?;
    let program = resolve_program(argv0);
    // A resolved workspace-binary path must exist; a bare interpreter name
    // (python3, node) is left for the OS to find on PATH at spawn time.
    if program.to_string_lossy().contains('/') && !program.exists() {
        return Err(Error::Other(format!(
            "action binary not found: {}",
            program.display()
        )));
    }

    let input = ActionInput {
        config: action.config.clone(),
        credentials,
        payload: payload.cloned(),
    };
    let stdin_bytes = serde_json::to_vec(&input)
        .map_err(|e| Error::Other(format!("failed to serialize action input: {e}")))?;

    let mut child = Command::new(&program)
        .args(&command[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| Error::Other(format!("failed to spawn action command: {e}")))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(&stdin_bytes)
            .await
            .map_err(|e| Error::Other(format!("failed to write stdin: {e}")))?;
    }

    // Bound the wait so a hung action can't hold the per-action run lock (and thus
    // block every future webhook for this action) indefinitely. On timeout the
    // wait future is dropped; `kill_on_drop` then SIGKILLs the child, and the
    // caller records the run as `error`, freeing the lock.
    let ceiling = subprocess_timeout(action);
    let output = match tokio::time::timeout(ceiling, child.wait_with_output()).await {
        Ok(res) => {
            res.map_err(|e| Error::Other(format!("failed to wait for action subprocess: {e}")))?
        }
        Err(_) => {
            return Err(Error::Other(format!(
                "action subprocess timed out after {}s",
                ceiling.as_secs()
            )));
        }
    };

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

    // Persist returned config back to the action row (JSONB column)
    sqlx::query("UPDATE app_actions SET config = $1, updated_at = now() WHERE id = $2")
        .bind(&action_output.config)
        .bind(&action.id)
        .execute(db)
        .await
        .map_err(|e| Error::Database(format!("failed to save action config: {e}")))?;

    // Surface stderr even on a clean exit. An action can succeed (exit 0, valid
    // JSON) while warning on stderr — that channel used to be swallowed, which
    // is exactly how the transcription runaway stayed invisible. Log it, and
    // fold a short tail into the run summary so it shows in the Telemetry tab.
    let mut summary = action_output.result;
    if !stderr.trim().is_empty() {
        tracing::warn!(action_id = %action.id, "action stderr (exit 0): {}", stderr.trim());
        let tail: String = stderr.trim().chars().rev().take(500).collect::<Vec<_>>()
            .into_iter().rev().collect();
        summary = format!("{summary}\n[stderr] {tail}");
    }

    Ok(SubprocessOutcome {
        summary,
        records: action_output.records,
    })
}

/// Default deployed location for action binaries, matching the installer's
/// `InstallConfig::actions_bin_dir` (`$INSTALL_PREFIX/libexec/virtues`). Kept
/// in sync with where the installer copies `actions-bin/` and points
/// `VIRTUES_ACTIONS_BIN_DIR`.
const WELL_KNOWN_ACTIONS_BIN_DIR: &str = "/usr/local/libexec/virtues";

/// Resolve a command's program (argv[0]) to something spawnable.
///
/// A bare name (no path separator) is first looked up as a Cargo-built action
/// binary — `$VIRTUES_ACTIONS_BIN_DIR/<name>` in production, then the
/// well-known install dir, else `target/{release,debug}/<name>` walking up from
/// cwd. If no workspace binary matches (e.g. `python3`, `node`) the name is
/// returned verbatim so the OS resolves it on `PATH`. Explicit paths
/// (`./x`, `/usr/bin/x`) pass through.
fn resolve_program(argv0: &str) -> PathBuf {
    if argv0.contains('/') {
        return PathBuf::from(argv0);
    }

    if let Ok(actions_dir) = std::env::var("VIRTUES_ACTIONS_BIN_DIR") {
        let p = PathBuf::from(actions_dir).join(argv0);
        if p.exists() {
            return p;
        }
    }

    // Well-known install location (matches the installer's
    // `InstallConfig::actions_bin_dir`), so a deployed box still resolves
    // action binaries even if VIRTUES_ACTIONS_BIN_DIR didn't reach the process
    // environment. Dev builds fall through to the target/ walk below.
    let installed = PathBuf::from(WELL_KNOWN_ACTIONS_BIN_DIR).join(argv0);
    if installed.exists() {
        return installed;
    }

    if let Ok(cwd) = std::env::current_dir() {
        for ancestor in cwd.ancestors() {
            let release = ancestor.join("target").join("release").join(argv0);
            if release.exists() {
                return release;
            }
            let debug = ancestor.join("target").join("debug").join(argv0);
            if debug.exists() {
                return debug;
            }
        }
    }

    // Not a workspace binary — spawn via PATH (interpreters, system tools).
    PathBuf::from(argv0)
}

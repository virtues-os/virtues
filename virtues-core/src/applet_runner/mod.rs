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
//!    JSON, read stdout JSON, save returned config back to `app_applets.config`.
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

pub mod limits;

use crate::error::{Error, Result};
use crate::scheduler::applets::{self, Applet};
use crate::server::yjs::YjsState;
use limits::Limits;

/// Dependencies threaded into the runner. Cheap to clone; holds references.
#[derive(Clone)]
pub struct RunnerDeps {
    pub db: PgPool,
    pub yjs: YjsState,
}

// Subprocess contract types live in `virtues_helpers::contract`. Re-export so
// existing call sites (`applet_runner::AppletInput`) keep working.
pub use virtues_helpers::contract::{AppletInput, AppletOutput};

/// Outcome of a single action run.
#[derive(Debug)]
pub struct AppletRunResult {
    pub run_id: Option<String>,
    pub status: AppletRunStatus,
    pub summary: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppletRunStatus {
    Success,
    Failed,
    Skipped,
    NotFound,
    Forbidden,
    /// Stopped by a spend ceiling the owner set (`config.limits.max_llm_cost`
    /// or `max_llm_cost_per_day`). Distinct from `Failed` on purpose: nothing
    /// broke, so it does not belong in the needs-attention strip beside
    /// genuine breakage.
    BudgetExceeded,
    /// Run row was created and execution was spawned on a detached task.
    /// Used only by `run_applet_detached` so the HTTP handler can return
    /// immediately with the run_id while the subprocess/agent finishes.
    Running,
}

impl AppletRunResult {
    fn not_found() -> Self {
        Self {
            run_id: None,
            status: AppletRunStatus::NotFound,
            summary: String::new(),
            error: None,
        }
    }

    fn forbidden(msg: impl Into<String>) -> Self {
        Self {
            run_id: None,
            status: AppletRunStatus::Forbidden,
            summary: String::new(),
            error: Some(msg.into()),
        }
    }
}

/// Outcome of `prepare_run`: either an early-exit result (no/short execution
/// needed) or a created run row that the caller should drive to completion.
enum PrepareOutcome {
    Early(AppletRunResult),
    Ready { action: Applet, run_id: String },
}

/// Run an action end-to-end. The single inline-await dispatch entry point.
///
/// Used by cron, webhooks, and tool-call paths that want to await the full
/// result. The HTTP handler uses `run_applet_detached` instead so a client
/// disconnect can't drop the future mid-subprocess.
pub async fn run_applet(
    deps: &RunnerDeps,
    applet_id: &str,
    trigger: &str,
    payload: Option<&serde_json::Value>,
) -> Result<AppletRunResult> {
    match prepare_run(deps, applet_id, trigger).await? {
        PrepareOutcome::Early(result) => Ok(result),
        PrepareOutcome::Ready { action, run_id } => {
            let payload = payload.cloned();
            Ok(execute_prepared(deps.clone(), action, run_id, trigger.to_string(), payload).await)
        }
    }
}

/// Run an action with the heavy phase (credentials → subprocess → agent →
/// complete_run) detached on a `tokio::spawn` task. Returns immediately once
/// the run row is created so a cancelled HTTP request can't orphan the row.
///
/// Returns `Running` status with the new `run_id` for the happy path; early
/// exits (not_found / forbidden / condition-skipped / concurrency-skipped /
/// view-runtime) are unchanged from `run_applet`.
pub async fn run_applet_detached(
    deps: &RunnerDeps,
    applet_id: &str,
    trigger: &str,
    payload: Option<&serde_json::Value>,
) -> Result<AppletRunResult> {
    match prepare_run(deps, applet_id, trigger).await? {
        PrepareOutcome::Early(result) => Ok(result),
        PrepareOutcome::Ready { action, run_id } => {
            let payload = payload.cloned();
            let deps_owned = deps.clone();
            let run_id_for_task = run_id.clone();
            let trigger_owned = trigger.to_string();
            tokio::spawn(async move {
                let _ =
                    execute_prepared(deps_owned, action, run_id_for_task, trigger_owned, payload)
                        .await;
            });
            Ok(AppletRunResult {
                run_id: Some(run_id),
                status: AppletRunStatus::Running,
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
    applet_id: &str,
    trigger: &str,
) -> Result<PrepareOutcome> {
    // 1. Fetch action
    let action = match applets::get_applet(&deps.db, applet_id).await {
        Ok(a) if a.enabled => a,
        Ok(_) => {
            tracing::warn!(applet_id, "action is disabled, ignoring run request");
            return Ok(PrepareOutcome::Early(AppletRunResult::not_found()));
        }
        Err(e) => {
            tracing::warn!(applet_id, error = %e, "action not found");
            return Ok(PrepareOutcome::Early(AppletRunResult::not_found()));
        }
    };

    // 2. Triggers validation
    if !action.triggers.iter().any(|t| t == trigger) {
        tracing::warn!(
            applet_id,
            trigger,
            allowed = ?action.triggers,
            "trigger not allowed for this action"
        );
        return Ok(PrepareOutcome::Early(AppletRunResult::forbidden(format!(
            "trigger '{}' not in allowed list {:?}",
            trigger, action.triggers
        ))));
    }

    // 2a. Face-only applets (no command, no agent — the old `view` runtime,
    // now derived from field presence) are pure-frontend renderers; never
    // invoked server-side. Skip silently so cron ticks don't churn the runs
    // table.
    let has_agent = action.agent.as_deref().is_some_and(|s| !s.trim().is_empty());
    let has_exec = has_agent || action.command.as_ref().is_some_and(|c| !c.is_empty());
    if !has_exec {
        tracing::debug!(applet_id, "face-only applet — never invoked server-side");
        return Ok(PrepareOutcome::Early(AppletRunResult {
            run_id: None,
            status: AppletRunStatus::Skipped,
            summary: "face-only applet — not server-invoked".to_string(),
            error: None,
        }));
    }

    // 2b. Webhook invariant: a webhook action must resolve to an identity that
    // authorizes the post — a device_id (device ingest, keyed on the proven iroh
    // key) or a credential_id (OAuth/api). One or the other must be set.
    if trigger == "webhook" && action.credential_id.is_none() && action.device_id.is_none() {
        tracing::error!(
            applet_id,
            "webhook trigger on action with no device_id or credential_id — rejected"
        );
        return Ok(PrepareOutcome::Early(AppletRunResult::forbidden(
            "webhook trigger requires a device_id or credential_id".to_string(),
        )));
    }

    // 3. Condition (SQL gate). Evaluate before creating a run row.
    //
    // A condition gates POLLS, not people. `message` and `manual` are someone
    // acting deliberately, and a gate like `extract(hour from now()) < 8`
    // would silently swallow what they just sent — the same shape as the
    // catch-up × time-of-day trap, and the same answer as rate caps exempting
    // "Run now": a limit that refuses the person pressing the button is a lock.
    let gated = !matches!(trigger, "message" | "manual");
    if let Some(condition) = &action.condition {
        if gated && !condition.trim().is_empty() {
            match eval_condition(&deps.db, condition).await {
                Ok(false) => {
                    tracing::debug!(applet_id, "condition falsy, skipping silently");
                    return Ok(PrepareOutcome::Early(AppletRunResult {
                        run_id: None,
                        status: AppletRunStatus::Skipped,
                        summary: "condition evaluated false".to_string(),
                        error: None,
                    }));
                }
                Ok(true) => {}
                Err(e) => {
                    tracing::error!(applet_id, error = %e, "condition evaluation failed");
                    let run = applets::create_run(&deps.db, Some(&action.id), trigger).await?;
                    let msg = format!("condition evaluation error: {e}");
                    applets::complete_run(&deps.db, &run.id, "error", 0, Some(&msg), None)
                        .await?;
                    return Ok(PrepareOutcome::Early(AppletRunResult {
                        run_id: Some(run.id),
                        status: AppletRunStatus::Failed,
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
    if applets::has_active_run(&deps.db, &action.id)
        .await
        .unwrap_or(false)
    {
        tracing::info!(applet_id, "previous run still active; skipping");
        let run = applets::create_run(&deps.db, Some(&action.id), trigger).await?;
        applets::complete_run(
            &deps.db,
            &run.id,
            "skipped",
            0,
            None,
            Some("skipped — previous run still active"),
        )
        .await?;
        return Ok(PrepareOutcome::Early(AppletRunResult {
            run_id: Some(run.id),
            status: AppletRunStatus::Skipped,
            summary: "previous run still active".to_string(),
            error: None,
        }));
    }

    // 4b. Limits that can be answered before spending anything: the rate caps
    // and the rolling daily spend ceiling. Deliberately after the concurrency
    // gate — an overlap skip is not an attempt, and must not eat a rate
    // budget. The refusal records a real run row, because "your applet did
    // not run, and here is exactly why" is the whole point of having a cap.
    let applet_limits = Limits::from_config(&action.config);
    match limits::check_pre_run(&deps.db, &action.id, &applet_limits, trigger).await {
        Ok(Some(refusal)) => {
            tracing::info!(applet_id, reason = refusal.message(), "run refused by limits");
            let run = applets::create_run(&deps.db, Some(&action.id), trigger).await?;
            applets::complete_run(
                &deps.db,
                &run.id,
                refusal.status(),
                0,
                None,
                Some(refusal.message()),
            )
            .await?;
            let status = match refusal {
                limits::Refusal::RateLimited(_) => AppletRunStatus::Skipped,
                limits::Refusal::OverDailyBudget(_) => AppletRunStatus::BudgetExceeded,
            };
            return Ok(PrepareOutcome::Early(AppletRunResult {
                run_id: Some(run.id),
                status,
                summary: refusal.message().to_string(),
                error: None,
            }));
        }
        Ok(None) => {}
        // A limits query that fails must not become a limit that blocks. The
        // caps are protective, not load-bearing: if we cannot read the ledger
        // we let the run through and say so, rather than silently freezing an
        // applet on a transient database error.
        Err(e) => {
            tracing::warn!(applet_id, error = %e, "limits check failed — allowing the run");
        }
    }

    // 5. Create run row.
    let run = applets::create_run(&deps.db, Some(&action.id), trigger).await?;
    Ok(PrepareOutcome::Ready {
        action,
        run_id: run.id,
    })
}

/// Steps 6–9 of the dispatch flow. Loads credentials, runs the subprocess
/// and/or agent phase, and persists the final run state. Errors are recorded
/// against the run row rather than propagated, so this function always returns
/// an `AppletRunResult` and is safe to detach.
async fn execute_prepared(
    deps: RunnerDeps,
    action: Applet,
    run_id: String,
    trigger: String,
    payload: Option<serde_json::Value>,
) -> AppletRunResult {
    let applet_id = action.id.clone();

    // What the person said, when this wake was a person saying something. The
    // exchange lives on the run — this plus `result_summary` IS the
    // conversation, which is why no separate thread object is minted.
    let message: Option<String> = (trigger == "message")
        .then(|| {
            payload
                .as_ref()
                .and_then(|p| p.get("message").or_else(|| p.get("text")))
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .flatten();
    if let Some(text) = &message {
        if let Err(e) = applets::set_run_message(&deps.db, &run_id, text).await {
            tracing::warn!(applet_id, error = %e, "failed to record run message");
        }
    }

    // Helper: persist `error` status and return a Failed result. Logs and
    // swallows DB errors from `complete_run` since at this point we have
    // nowhere to propagate them — the caller may already be detached.
    async fn fail(
        deps: &RunnerDeps,
        run_id: &str,
        applet_id: &str,
        msg: String,
    ) -> AppletRunResult {
        if let Err(e) =
            applets::complete_run(&deps.db, run_id, "error", 0, Some(&msg), None).await
        {
            tracing::error!(applet_id, error = %e, "complete_run failed while recording error");
        }
        AppletRunResult {
            run_id: Some(run_id.to_string()),
            status: AppletRunStatus::Failed,
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
                tracing::error!(applet_id, error = %msg, "credential load failed");
                return fail(&deps, &run_id, &applet_id, msg).await;
            }
        }
    } else {
        None
    };

    // 7. Subprocess phase.
    let mut subprocess_summary: Option<String> = None;
    let mut subprocess_records: i64 = 0;
    let has_command = action.command.as_ref().is_some_and(|c| !c.is_empty());
    if has_command {
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
                tracing::error!(applet_id, error = %msg, "subprocess phase failed");
                return fail(&deps, &run_id, &applet_id, msg).await;
            }
        }
    }

    // 8. Agent phase.
    if let Some(prompt) = action.agent.as_ref().filter(|s| !s.trim().is_empty()) {
        let ctx = subprocess_summary.as_deref();
        match crate::agent::applet_runner::run_agent_loop(
            &deps.db,
            &deps.yjs,
            &action,
            prompt,
            ctx,
            &run_id,
            message.as_deref(),
        )
        .await
        {
            Ok(agent_result) => {
                let steps = agent_result.steps as i64;

                // Stopped at the ceiling: the work is partial by definition, so
                // it is neither a success nor a failure. Recording it as
                // `success` would let `until = "once"` archive an applet that
                // never finished its one job.
                if let Some(reason) = agent_result.budget_stopped {
                    if let Err(e) = applets::complete_run(
                        &deps.db,
                        &run_id,
                        "budget_exceeded",
                        steps,
                        None,
                        Some(&reason),
                    )
                    .await
                    {
                        tracing::error!(applet_id, error = %e, "complete_run failed after budget stop");
                    }
                    return AppletRunResult {
                        run_id: Some(run_id),
                        status: AppletRunStatus::BudgetExceeded,
                        summary: reason,
                        error: None,
                    };
                }

                let summary = agent_result
                    .message
                    .clone()
                    .or(subprocess_summary.clone())
                    .unwrap_or_default();
                if let Err(e) =
                    applets::complete_run(&deps.db, &run_id, "success", steps, None, Some(&summary))
                        .await
                {
                    tracing::error!(applet_id, error = %e, "complete_run failed after agent success");
                }
                maybe_archive_on_until(&deps.db, &action).await;
                return AppletRunResult {
                    run_id: Some(run_id),
                    status: AppletRunStatus::Success,
                    summary,
                    error: None,
                };
            }
            Err(e) => {
                let msg = e.to_string();
                tracing::error!(applet_id, error = %msg, "agent phase failed");
                return fail(&deps, &run_id, &applet_id, msg).await;
            }
        }
    }

    // 9. Complete run.
    let summary = subprocess_summary.unwrap_or_default();
    if let Err(e) =
        applets::complete_run(&deps.db, &run_id, "success", subprocess_records, None, Some(&summary)).await
    {
        tracing::error!(applet_id, error = %e, "complete_run failed at end of run");
    }
    maybe_archive_on_until(&deps.db, &action).await;
    AppletRunResult {
        run_id: Some(run_id),
        status: AppletRunStatus::Success,
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
async fn maybe_archive_on_until(db: &PgPool, action: &Applet) {
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
                tracing::warn!(applet_id = %action.id, error = %e, "until evaluation failed; not archiving");
                false
            }
        }
    };
    if done {
        tracing::info!(applet_id = %action.id, "lifecycle complete (until met); archiving");
        if let Err(e) = applets::archive_applet(db, &action.id).await {
            tracing::error!(applet_id = %action.id, error = %e, "archive_applet failed");
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

/// Per-applet subprocess ceiling: `config.limits.timeout_s`, falling back to
/// the global default. Applets with long legitimate runs declare their own —
/// e.g. embedding_index's initial-onboarding drain is hours of real work, so
/// its manifest carries `[config.limits] timeout_s = 7500` and its internal
/// 2-hour wall-clock limit exits cleanly rather than being SIGKILLed.
///
/// Parsing lives in [`limits::Limits`] with every other cap; this stays as the
/// call-site shorthand.
fn subprocess_timeout(action: &Applet) -> std::time::Duration {
    Limits::from_config(&action.config).subprocess_timeout()
}

/// What a successful subprocess phase produced: the one-line summary plus the
/// processed-record count (for `app_applet_runs.records_processed`).
struct SubprocessOutcome {
    summary: String,
    records: i64,
}

/// Environment every applet gets. Deliberately short: an applet receives its
/// config, its payload and its already-decrypted credentials on **stdin**, so
/// the environment is for reaching the box's own services, not for secrets.
const ENV_PASSTHROUGH: &[&str] = &[
    // Process basics. Without PATH a bare `python3` argv0 cannot resolve.
    "PATH",
    "HOME",
    "LANG",
    "LC_ALL",
    "TZ",
    "RUST_LOG",
    "RUST_BACKTRACE",
    // The box's own Postgres. `virtues_helpers::connect_from_env` reads these.
    "DATABASE_URL",
    "DATABASE_MAX_CONNECTIONS",
    // Where the box keeps its data, and the paths applets resolve against.
    "VIRTUES_DATA_DIR",
    "VIRTUES_LAKE_DIR",
    "VIRTUES_APPLETS_DIR",
    "VIRTUES_APPLET_STATE_DIR",
    "VIRTUES_APPLETS_BIN_DIR",
    "VIRTUES_BIN",
    // Outbound service endpoints (no credentials in either).
    "VIRTUES_OAUTH_PROXY_URL",
    "VIRTUES_API_URL",
    "ENVIRONMENT",
];

/// Memory ceiling for a jailed applet. Generous — some shipped syncs are
/// memory-hungry — but bounded, so a runaway import cannot take the box out.
const JAILED_MEMORY_MAX: &str = "1G";

/// Build the spawn command, jailing it when the code did not ship with the box.
///
/// The routing, not a ban, is the policy: a package may run native code, it
/// just does not get to run it as a user with passwordless sudo. `systemd-run`
/// is the same mechanism `code_interpreter` already uses (api/code.rs), which
/// is the strongest precedent in this codebase for containing code we did not
/// write.
///
/// The properties are looser than `code_interpreter`'s in one deliberate way:
/// **no `PrivateNetwork`**, because an applet's whole job is to reach Postgres
/// and usually an upstream API. What it does buy:
///
/// - `NoNewPrivileges=yes` — the single most valuable line here. The box user
///   has `NOPASSWD: ALL` (installer, by design), so without this any imported
///   applet is one `sudo -n` from root.
/// - `ProtectSystem=strict` + `ProtectHome=yes` — the filesystem is read-only
///   apart from the paths an applet legitimately writes.
/// - `PrivateTmp`, `PrivateDevices`, memory and runtime ceilings, and a
///   syscall filter.
///
/// Not `DynamicUser`: an applet writes to the lake and its own state directory
/// as the box user, and a dynamic uid cannot.
///
/// Off Linux, or in a debug build without systemd, we run direct — that is a
/// developer machine. In a release build on Linux a missing `systemd-run` is a
/// hard refusal rather than a silent downgrade, matching `code.rs`: the moment
/// the sandbox quietly stops applying is the moment it stops being one.
fn build_command(
    program: &std::path::Path,
    args: &[String],
    shipped: bool,
    timeout: std::time::Duration,
    env: &[(String, String)],
) -> Result<Command> {
    // Direct spawn: env_clear plus exactly what we chose to pass.
    let direct = || {
        let mut cmd = Command::new(program);
        cmd.args(args);
        cmd.env_clear();
        for (k, v) in env {
            cmd.env(k, v);
        }
        cmd
    };

    if shipped {
        return Ok(direct());
    }

    if !cfg!(target_os = "linux") {
        tracing::warn!(
            "running an unshipped applet unjailed — no systemd on this platform (developer machine)"
        );
        return Ok(direct());
    }

    if which_systemd_run().is_none() {
        if cfg!(debug_assertions) {
            tracing::warn!("systemd-run unavailable; running unjailed (debug build only)");
            return Ok(direct());
        }
        return Err(Error::Other(
            "applet sandbox (systemd-run) is unavailable; refusing to run imported code unjailed"
                .to_string(),
        ));
    }

    let state = crate::applet_templates::state_root();

    // `sudo -n`, matching api/updates.rs. The box runs as `User=virtues`
    // (installer), and a non-root user creating a *system* transient unit goes
    // through polkit, which on a headless box with no agent denies outright. A
    // bare `systemd-run` therefore does not fail loudly — it exits non-zero and
    // the run is recorded as a generic subprocess failure, so the sandbox
    // reports itself present while every unshipped applet silently dies.
    let mut cmd = Command::new("sudo");
    cmd.args(["-n", "systemd-run"]);
    // And drop back to the box user. A transient SYSTEM unit with no `User=`
    // runs as ROOT — `code.rs` only avoids that because it sets
    // `DynamicUser=yes`, which this cannot use (an applet writes to the lake as
    // the box user). Without this line, adding the sudo above would have
    // escalated every imported applet from `virtues` to uid 0, i.e. strictly
    // worse than no jail at all. `NoNewPrivileges` does not help you if you
    // start at root.
    cmd.args(["-p", &format!("User={}", box_uid())]);
    cmd.args([
        "--pipe",
        "--wait",
        "--collect",
        "--quiet",
        "-p",
        "NoNewPrivileges=yes",
        "-p",
        "ProtectSystem=strict",
        "-p",
        "ProtectHome=yes",
        "-p",
        "PrivateTmp=yes",
        "-p",
        "PrivateDevices=yes",
        "-p",
        "SystemCallFilter=@system-service",
        "-p",
        "SystemCallErrorNumber=EPERM",
        "-p",
        "MemorySwapMax=0",
    ]);
    cmd.args(["-p", &format!("MemoryMax={JAILED_MEMORY_MAX}")]);
    cmd.args(["-p", &format!("RuntimeMaxSec={}", timeout.as_secs())]);
    // The applet's own folder and the lake are the only writable paths it gets.
    cmd.args(["-p", &format!("ReadWritePaths={}", state.display())]);
    if let Ok(lake) = std::env::var("VIRTUES_LAKE_DIR") {
        if !lake.is_empty() {
            cmd.args(["-p", &format!("ReadWritePaths={lake}")]);
        }
    }
    // The unit does NOT inherit our environment, and `cmd.env()` would set
    // systemd-run's own rather than the unit's — so every variable the applet
    // needs has to be handed over explicitly, before the `--`.
    for (k, v) in env {
        // `ProtectHome=yes` mounts /home empty, so forwarding the real HOME
        // hands the applet a path that exists in its environment and not in its
        // namespace — anything touching ~/.cache (python, pip, git, most HTTP
        // clients) fails on it. Point it at the private tmp instead, as code.rs
        // does.
        if k == "HOME" {
            continue;
        }
        cmd.args(["-E", &format!("{k}={v}")]);
    }
    cmd.args(["-E", "HOME=/tmp"]);
    cmd.arg("--");
    cmd.arg(program);
    cmd.args(args);
    Ok(cmd)
}

/// The uid the box itself runs as, so a jailed applet lands on the same user
/// rather than root. Read from `/proc/self` to avoid a libc dependency; the
/// jail is Linux-only, which is exactly where this path exists.
fn box_uid() -> u32 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata("/proc/self").map(|m| m.uid()).unwrap_or(0)
}

fn which_systemd_run() -> Option<std::path::PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|p| p.join("systemd-run"))
            .find(|p| p.is_file())
    })
}

/// Whether this applet's code shipped with the box, decided by **where the
/// folder actually is** rather than by what its manifest claims. A manifest's
/// `owner` is attacker-controlled — a package can declare `owner = "system"` —
/// so it must not gate anything security-relevant. The filesystem cannot lie.
fn is_shipped_applet(applet_id: &str) -> bool {
    let Some(dir) = crate::applet_templates::dir_for_applet_id(applet_id) else {
        // Unknown to the catalog: treat as untrusted.
        return false;
    };
    let resolved = crate::applet_templates::resolve_applet_dir(&dir);
    let shipped = crate::applet_templates::shipped_root();
    match (resolved.canonicalize(), shipped.canonicalize()) {
        (Ok(r), Ok(s)) => r.starts_with(s),
        // If either path can't be resolved we cannot prove provenance, so we
        // don't grant on it.
        _ => false,
    }
}

/// Build the subprocess environment.
///
/// The spawn used to inherit the server's entire environment, which handed
/// every applet `VIRTUES_ENCRYPTION_KEY` — the master key for the whole
/// credential vault — and the unscoped `DATABASE_URL`. That made
/// `load_credentials`' promise ("the master encryption key never crosses the
/// subprocess boundary") true of the stdin payload and false in fact, and it
/// meant any imported package could decrypt every credential on the box.
///
/// `credential_refresh` genuinely needs the key: it re-encrypts rotated tokens
/// through `ensure_fresh`. So the key is granted, but only to code that shipped
/// with the box — never to an imported or authored package.
fn env_pairs(applet_id: &str, shipped: bool) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = ENV_PASSTHROUGH
        .iter()
        .filter_map(|k| std::env::var(k).ok().map(|v| ((*k).to_string(), v)))
        .collect();
    // Two conditions, both required: the applet must declare it needs the key,
    // and its code must have shipped with the box. The declaration keeps the
    // grant auditable from the manifest instead of implicit in provenance; the
    // provenance check keeps a package from simply declaring its way in.
    if shipped && crate::applet_templates::declares_vault_key_need(applet_id) {
        if let Ok(val) = std::env::var("VIRTUES_ENCRYPTION_KEY") {
            out.push(("VIRTUES_ENCRYPTION_KEY".to_string(), val));
        }
    }
    // The dev-only wallet override (`make dev` sets it; prod leaves it unset and
    // `BearerClient::ensure_bearer` reads the vault through the pool instead).
    // The Plaid syncs call virtues-api for every page of transactions, so
    // without this they fail on a dev box with "no virtues_api key" — env_clear
    // took it away and nothing else supplies it. Shipped-only: it spends the
    // user's wallet, and an imported package has no business presenting it.
    if shipped {
        if let Ok(val) = std::env::var("VIRTUES_API_KEY") {
            if !val.is_empty() {
                out.push(("VIRTUES_API_KEY".to_string(), val));
            }
        }
    }
    out
}

async fn run_subprocess(
    db: &PgPool,
    action: &Applet,
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

    let input = AppletInput {
        config: action.config.clone(),
        credentials,
        payload: payload.cloned(),
    };
    let stdin_bytes = serde_json::to_vec(&input)
        .map_err(|e| Error::Other(format!("failed to serialize action input: {e}")))?;

    let shipped = is_shipped_applet(&action.id);
    let env = env_pairs(&action.id, shipped);
    let mut cmd = build_command(
        &program,
        &command[1..],
        shipped,
        subprocess_timeout(action),
        &env,
    )?;

    let mut child = cmd
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
    // When jailed, RuntimeMaxSec is the real enforcer and this is only a
    // backstop — equal values mean the tokio clock (which starts before
    // systemd-run has finished setting the unit up) always wins, and dropping
    // the future SIGKILLs the systemd-run *client*, not the unit, which PID 1
    // owns. The applet would keep running with its pipes closed while the
    // per-applet lock was already released. Same grace `code.rs` uses.
    let ceiling = if shipped {
        subprocess_timeout(action)
    } else {
        subprocess_timeout(action) + std::time::Duration::from_secs(10)
    };
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

    let applet_output: AppletOutput = serde_json::from_str(&stdout).map_err(|e| {
        Error::Other(format!(
            "failed to parse subprocess stdout JSON: {e}. raw: {}",
            &stdout[..stdout.len().min(500)]
        ))
    })?;

    // Persist returned config back to the action row (JSONB column)
    sqlx::query("UPDATE app_applets SET config = $1, updated_at = now() WHERE id = $2")
        .bind(&applet_output.config)
        .bind(&action.id)
        .execute(db)
        .await
        .map_err(|e| Error::Database(format!("failed to save action config: {e}")))?;

    // Surface stderr even on a clean exit. An action can succeed (exit 0, valid
    // JSON) while warning on stderr — that channel used to be swallowed, which
    // is exactly how the transcription runaway stayed invisible. Log it, and
    // fold a short tail into the run summary so it shows in the Telemetry tab.
    let mut summary = applet_output.result;
    if !stderr.trim().is_empty() {
        tracing::warn!(applet_id = %action.id, "action stderr (exit 0): {}", stderr.trim());
        let tail: String = stderr.trim().chars().rev().take(500).collect::<Vec<_>>()
            .into_iter().rev().collect();
        summary = format!("{summary}\n[stderr] {tail}");
    }

    Ok(SubprocessOutcome {
        summary,
        records: applet_output.records,
    })
}

/// Default deployed location for action binaries, matching the installer's
/// `InstallConfig::applets_bin_dir` (`$INSTALL_PREFIX/libexec/virtues`). Kept
/// in sync with where the installer copies `actions-bin/` and points
/// `VIRTUES_ACTIONS_BIN_DIR`.
const WELL_KNOWN_APPLETS_BIN_DIR: &str = "/usr/local/libexec/virtues";

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

    for var in ["VIRTUES_APPLETS_BIN_DIR", "VIRTUES_ACTIONS_BIN_DIR"] {
        if let Ok(bin_dir) = std::env::var(var) {
            let p = PathBuf::from(bin_dir).join(argv0);
            if p.exists() {
                return p;
            }
        }
    }

    // Well-known install location (matches the installer's
    // `InstallConfig::applets_bin_dir`), so a deployed box still resolves
    // applet binaries even if VIRTUES_APPLETS_BIN_DIR didn't reach the process
    // environment. Dev builds fall through to the target/ walk below.
    let installed = PathBuf::from(WELL_KNOWN_APPLETS_BIN_DIR).join(argv0);
    if installed.exists() {
        return installed;
    }

    // Dev: look beside the running binary. Applet binaries are built into the
    // same profile directory as virtues-core itself, so this holds wherever
    // cargo puts them — and it has to, because the target dir is not `./target`
    // here: `.cargo/config.toml` redirects it to a shared cache so parallel
    // worktrees don't each cold-build 67GB. The `target/` walk below assumed the
    // default layout and silently missed, leaving a bare argv0 that the OS then
    // failed to find on PATH ("No such file or directory") every cron tick.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join(argv0);
            if sibling.exists() {
                return sibling;
            }
        }
    }

    // Explicit override, and the conventional layout, for callers whose target
    // dir neither matches the running binary's nor is configured.
    let roots = std::env::var("CARGO_TARGET_DIR")
        .map(|d| vec![PathBuf::from(d)])
        .unwrap_or_default();
    for root in roots {
        for profile in ["release", "debug"] {
            let p = root.join(profile).join(argv0);
            if p.exists() {
                return p;
            }
        }
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

#[cfg(test)]
mod tests {
    use super::ENV_PASSTHROUGH;
    use super::*;
    use crate::server::yjs::YjsState;

    async fn applet_with_falsy_condition(pool: &sqlx::PgPool, id: &str, triggers: &str) {
        sqlx::query(
            "INSERT INTO app_applets (id, name, owner, command, condition, triggers, enabled) \
             VALUES ($1, $1, 'user', '[\"echo\"]', 'FALSE', $2::jsonb, TRUE)",
        )
        .bind(id)
        .bind(triggers)
        .execute(pool)
        .await
        .expect("insert");
    }

    async fn run_count(pool: &sqlx::PgPool, id: &str) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM app_applet_runs WHERE applet_id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    /// A condition gates POLLS. Someone who just pressed send is not a poll, and
    /// a clock gate like `extract(hour from now()) < 8` would otherwise swallow
    /// what they typed — silently, since a falsy condition records nothing.
    /// Same shape as the catch-up × time-of-day trap, same answer as rate caps
    /// exempting "Run now".
    #[sqlx::test]
    async fn a_condition_does_not_swallow_a_message(pool: sqlx::PgPool) {
        let deps = RunnerDeps {
            db: pool.clone(),
            yjs: YjsState::new(pool.clone()),
        };
        applet_with_falsy_condition(&pool, "applet_m", r#"["cron","message"]"#).await;

        // The cron wake is gated, and silently: no run row at all.
        let cron = run_applet(&deps, "applet_m", "cron", None).await.unwrap();
        assert_eq!(cron.status, AppletRunStatus::Skipped);
        assert_eq!(run_count(&pool, "applet_m").await, 0, "a poll is gated silently");

        // The message is not.
        let payload = serde_json::json!({ "message": "I had eggs" });
        let msg = run_applet(&deps, "applet_m", "message", Some(&payload))
            .await
            .unwrap();
        assert_ne!(msg.status, AppletRunStatus::Skipped, "the person was heard");
        assert_eq!(run_count(&pool, "applet_m").await, 1);

        let said: Option<String> =
            sqlx::query_scalar("SELECT message FROM app_applet_runs WHERE applet_id = 'applet_m'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(said.as_deref(), Some("I had eggs"), "the words are on the run");
    }

    /// `message` still has to be granted like any other trigger.
    #[sqlx::test]
    async fn an_applet_that_does_not_take_messages_refuses_one(pool: sqlx::PgPool) {
        let deps = RunnerDeps {
            db: pool.clone(),
            yjs: YjsState::new(pool.clone()),
        };
        applet_with_falsy_condition(&pool, "applet_n", r#"["cron"]"#).await;
        let payload = serde_json::json!({ "message": "hello" });
        let r = run_applet(&deps, "applet_n", "message", Some(&payload))
            .await
            .unwrap();
        assert_eq!(r.status, AppletRunStatus::Forbidden);
    }

    /// The vault master key must never be granted by the blanket passthrough.
    /// It is handed out in `apply_env` only to shipped code, and the tempting
    /// fix for "my applet can't decrypt" is to add it here — which would return
    /// every imported package's access to every credential on the box.
    #[test]
    fn passthrough_never_carries_the_vault_key() {
        assert!(
            !ENV_PASSTHROUGH.contains(&"VIRTUES_ENCRYPTION_KEY"),
            "VIRTUES_ENCRYPTION_KEY must stay provenance-gated in apply_env"
        );
    }
}

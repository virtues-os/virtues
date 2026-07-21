//! Action + ActionRun models and CRUD operations
//!
//! An `Action` is a scheduled unit of work: pure configuration, no runtime
//! state. An `ActionRun` is one execution of an action with full history.
//!
//! Behavior is inferred from which fields are populated on the row:
//! - `command` set  → spawn it (subprocess for `function`, daemon for `service`)
//! - `agent` set    → LLM agent loop (runs after subprocess if both set)
//! - neither set    → invalid (runner rejects)
//!
//! `triggers` is a JSON array of trigger names controlling who can invoke the
//! action. Enum: `cron | manual | tool | api | webhook`.

use crate::error::{Error, Result};
use crate::ids::generate_id;
use sqlx::{Row, PgPool};

// ID prefixes
const RUN_PREFIX: &str = "run";

// Run-log text field caps. Agent messages and subprocess stderr can balloon
// to tens of KB; we store a useful prefix and drop the rest.
const RESULT_SUMMARY_MAX_BYTES: usize = 8 * 1024;
const ERROR_MAX_BYTES: usize = 4 * 1024;
const TRUNCATED_SUFFIX: &str = "… [truncated]";

/// Truncate a string to at most `max` bytes, appending a truncation suffix on
/// overflow. Respects UTF-8 char boundaries so the result is always valid.
fn truncate_utf8_bytes(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let suffix_len = TRUNCATED_SUFFIX.len();
    let budget = max.saturating_sub(suffix_len);
    let mut end = 0;
    for (i, _) in s.char_indices() {
        if i > budget {
            break;
        }
        end = i;
    }
    let mut out = String::with_capacity(end + suffix_len);
    out.push_str(&s[..end]);
    out.push_str(TRUNCATED_SUFFIX);
    out
}

/// A scheduled action — pure configuration, no runtime state.
///
/// `runtime` declares how the action executes:
///   - `function` — fork-per-trigger CLI (today's pattern)
///   - `service`  — long-running supervised HTTP server, dispatched via proxy
///   - `view`     — pure Svelte component, never invoked server-side
///
/// `command` is the argv to spawn (JSON array in SQL). A bare `command[0]`
/// (no path separator) resolves to a Cargo-built action binary under
/// `target/{debug,release}`; anything else (`./x`, `python3`, `node`) runs via
/// PATH. Same field for both the `function` runner and the `service` supervisor.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Action {
    pub id: String,
    pub owner: String,
    pub name: String,
    pub agent: Option<String>,
    pub cron_schedule: Option<String>,
    pub enabled: bool,
    pub config: serde_json::Value,
    pub condition: Option<String>,
    pub triggers: Vec<String>,
    pub memory: Option<String>,
    /// Outbound OAuth/API-key actions anchor here (the secret they call with).
    pub credential_id: Option<String>,
    /// Device-ingest (webhook) actions anchor here — the owning device whose
    /// proven iroh key authorizes posts to this action.
    pub device_id: Option<String>,
    pub command: Option<Vec<String>>,
    /// Lifecycle: NULL = forever · `"once"` = archive after first success ·
    /// SQL boolean = archive when it evaluates true (checked post-success).
    pub until: Option<String>,
    /// Set when the lifecycle completed; archived applets also get
    /// `enabled = FALSE` so the scheduler skips them naturally.
    pub archived_at: Option<crate::types::Timestamp>,
    /// Command applets: run as a long-lived supervised service (the old
    /// `runtime = 'service'`) instead of fork-per-trigger.
    pub supervise: bool,
    pub created_at: crate::types::Timestamp,
    pub updated_at: crate::types::Timestamp,
}

/// One execution of an action.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActionRun {
    pub id: String,
    pub action_id: Option<String>,
    pub status: String,
    pub started_at: crate::types::Timestamp,
    pub completed_at: Option<crate::types::Timestamp>,
    pub records_processed: i64,
    pub error: Option<String>,
    pub trigger: String,
    pub parent_run_id: Option<String>,
    pub transform_stage: Option<String>,
    pub result_summary: Option<String>,
    pub created_at: crate::types::Timestamp,
}

// ============================================================================
// Action CRUD
// ============================================================================

/// Get all enabled actions.
pub async fn get_enabled_actions(db: &PgPool) -> Result<Vec<Action>> {
    let rows = sqlx::query("SELECT * FROM app_applets WHERE enabled = TRUE ORDER BY name")
        .fetch_all(db)
        .await?;

    rows.iter().map(action_from_row).collect()
}

/// Get all actions (for API listing).
pub async fn get_all_actions(db: &PgPool) -> Result<Vec<Action>> {
    let rows = sqlx::query("SELECT * FROM app_applets ORDER BY name")
        .fetch_all(db)
        .await?;

    rows.iter().map(action_from_row).collect()
}

/// Get an action by ID.
pub async fn get_action(db: &PgPool, action_id: &str) -> Result<Action> {
    let row = sqlx::query("SELECT * FROM app_applets WHERE id = $1")
        .bind(action_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Action not found: {}", action_id)))?;

    action_from_row(&row)
}

/// Toggle an action's enabled state.
pub async fn toggle_action(db: &PgPool, action_id: &str, enabled: bool) -> Result<()> {
    let affected = sqlx::query("UPDATE app_applets SET enabled = $1 WHERE id = $2")
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
pub async fn update_memory(db: &PgPool, action_id: &str, memory: &str) -> Result<()> {
    let affected = sqlx::query(
        "UPDATE app_applets SET memory = $1, updated_at = now() WHERE id = $2",
    )
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
/// Delete a user-owned applet: its row, its run-history linkage, its on-disk
/// folder (so reconcile won't resurrect it), and — only when `drop_data` — the
/// private `applet_<slug>` schema it owns. System rows are refused.
///
/// Folder + schema teardown live here so BOTH the HTTP delete and the chat
/// `delete_applet` tool tear down identically (one door). `drop_data` defaults
/// to keeping data: an applet's tables outlive it unless the user opts in.
pub async fn delete_action(db: &PgPool, action_id: &str, drop_data: bool) -> Result<()> {
    let owner: Option<String> = sqlx::query_scalar("SELECT owner FROM app_applets WHERE id = $1")
        .bind(action_id)
        .fetch_optional(db)
        .await?;
    if owner.as_deref() == Some("system") {
        return Err(crate::Error::InvalidInput("Cannot delete system action".into()));
    }

    // Resolve the on-disk folder BEFORE the row goes away. Only chat-authored
    // folders (under user/) are ours to remove; builtin/imported folders are
    // managed by their own lanes.
    let dir =
        crate::action_templates::dir_for_action_id(action_id).filter(|d| d.starts_with("user/"));

    sqlx::query("UPDATE app_applet_runs SET action_id = NULL WHERE action_id = $1")
        .bind(action_id)
        .execute(db)
        .await?;
    sqlx::query("DELETE FROM app_applets WHERE id = $1")
        .bind(action_id)
        .execute(db)
        .await?;

    // Drop the applet's owned data schema only when the caller opts in. The
    // schema name is derived from the id (slugs are `[a-z0-9_]`, so it is a
    // safe unquoted identifier).
    if drop_data {
        if let Some(schema) = applet_schema_name(action_id) {
            sqlx::query(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
                .execute(db)
                .await?;
        }
    }

    if let Some(d) = dir {
        let path = crate::action_templates::actions_root().join(&d);
        if let Err(e) = std::fs::remove_dir_all(&path) {
            return Err(crate::Error::Other(format!(
                "applet row deleted but folder removal failed ({e}); it may reappear on \
                 reconcile — remove {d} manually"
            )));
        }
        crate::action_templates::reload_catalog();
    }

    Ok(())
}

/// The Postgres schema a user applet owns for its private tables, or `None` for
/// non-user applets — only `action_user__<slug>` applets own an `applet_`
/// schema. Slugs are `[a-z0-9_]`, so the result is a safe unquoted identifier.
pub(crate) fn applet_schema_name(action_id: &str) -> Option<String> {
    let slug = action_id.strip_prefix("action_user__")?;
    if slug.is_empty()
        || !slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        return None;
    }
    Some(format!("applet_{slug}"))
}

/// List the base tables in a user applet's owned `applet_<slug>` schema. Empty
/// when the applet owns no schema (never created one, or isn't a user applet) —
/// used by the delete confirm to show exactly what `drop_data` would remove.
pub async fn applet_data_tables(db: &PgPool, action_id: &str) -> Result<Vec<String>> {
    let Some(schema) = applet_schema_name(action_id) else {
        return Ok(Vec::new());
    };
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables \
         WHERE table_schema = $1 AND table_type = 'BASE TABLE' \
         ORDER BY table_name",
    )
    .bind(&schema)
    .fetch_all(db)
    .await?;
    Ok(tables)
}

/// Fields the user can tune on a `system`-owned action row. Must match the
/// set of fields that template reconcile preserves (see
/// `action_templates::upsert_row`) — otherwise the next reconcile would
/// silently clobber what the user just changed.
pub const SYSTEM_EDITABLE_FIELDS: &[&str] = &["enabled", "cron_schedule", "config", "memory"];

/// Create a new user-owned action. Used by chat tools + the HTTP POST
/// endpoint. `id` is generated from the name if not provided.
#[allow(clippy::too_many_arguments)]
pub async fn create_user_action(
    db: &PgPool,
    id: Option<&str>,
    name: &str,
    agent: Option<&str>,
    cron_schedule: Option<&str>,
    triggers: &[String],
    config: Option<&serde_json::Value>,
) -> Result<Action> {
    if name.trim().is_empty() {
        return Err(Error::InvalidInput("name cannot be empty".into()));
    }
    if triggers.is_empty() {
        return Err(Error::InvalidInput("triggers cannot be empty".into()));
    }
    for t in triggers {
        if !matches!(t.as_str(), "cron" | "manual" | "tool" | "api" | "webhook") {
            return Err(Error::InvalidInput(format!(
                "invalid trigger '{t}': must be one of cron, manual, tool, api, webhook"
            )));
        }
    }
    if triggers.iter().any(|t| t == "webhook") {
        return Err(Error::InvalidInput(
            "webhook trigger requires a credential_id, which user-created actions can't set; use a pairing flow instead".into()
        ));
    }

    let base_id = match id {
        Some(s) => s.to_string(),
        None => format!("action_user_{}", slugify(name)),
    };
    let triggers_json = serde_json::to_string(triggers)
        .map_err(|e| Error::Other(format!("failed to serialize triggers: {e}")))?;
    let config_json = config
        .map(|v| v.to_string())
        .unwrap_or_else(|| "{}".to_string());

    // Retry with a numeric suffix on UNIQUE collision so two actions with
    // the same slug don't fail with an opaque "UNIQUE constraint failed".
    // Capped at 100 attempts; if the user has 100 actions with identical
    // slugs something weirder is going on and they can pass an explicit id.
    const MAX_ATTEMPTS: u32 = 100;
    let mut action_id = base_id.clone();
    for attempt in 1u32..=MAX_ATTEMPTS {
        let result = sqlx::query(
            r#"INSERT INTO app_applets (id, name, owner, agent, cron_schedule, enabled, config, triggers)
               VALUES ($1, $2, 'user', $3, $4, TRUE, $5::jsonb, $6::jsonb)"#,
        )
        .bind(&action_id)
        .bind(name)
        .bind(agent)
        .bind(cron_schedule)
        .bind(&config_json)
        .bind(&triggers_json)
        .execute(db)
        .await;

        match result {
            Ok(_) => return get_action(db, &action_id).await,
            Err(sqlx::Error::Database(dbe)) if is_unique_violation(&*dbe) => {
                if id.is_some() {
                    // Caller supplied an explicit id — don't silently rename.
                    return Err(Error::InvalidInput(format!(
                        "action id '{action_id}' already exists"
                    )));
                }
                action_id = format!("{base_id}_{}", attempt + 1);
                continue;
            }
            Err(e) => {
                return Err(Error::Database(format!("failed to create action: {e}")));
            }
        }
    }

    Err(Error::Database(format!(
        "failed to create action: exhausted {MAX_ATTEMPTS} id variations"
    )))
}

/// Detect a Postgres UNIQUE constraint violation from a `DatabaseError`.
fn is_unique_violation(dbe: &dyn sqlx::error::DatabaseError) -> bool {
    // sqlx maps Postgres SQLSTATE 23505 (unique_violation) here.
    dbe.is_unique_violation()
}

/// Partial update for an action. Applies a JSON patch, enforcing the
/// system-owner guard: `system` rows accept only `SYSTEM_EDITABLE_FIELDS`.
///
/// Unknown field names are rejected (400). Null values are allowed for
/// nullable columns (`agent`, `cron_schedule`, `condition`, `memory`).
pub async fn update_action(
    db: &PgPool,
    action_id: &str,
    patch: &serde_json::Value,
) -> Result<Action> {
    let obj = patch
        .as_object()
        .ok_or_else(|| Error::InvalidInput("patch must be a JSON object".into()))?;
    if obj.is_empty() {
        return get_action(db, action_id).await;
    }

    let current = get_action(db, action_id).await?;
    let is_system = current.owner == "system";

    // Validate every field name up front so we either apply the whole patch
    // or reject it cleanly.
    const ALLOWED: &[&str] = &[
        "name",
        "agent",
        "cron_schedule",
        "enabled",
        "config",
        "condition",
        "triggers",
        "memory",
    ];
    for key in obj.keys() {
        if !ALLOWED.contains(&key.as_str()) {
            return Err(Error::InvalidInput(format!(
                "unknown field '{key}'; allowed: {ALLOWED:?}"
            )));
        }
        if is_system && !SYSTEM_EDITABLE_FIELDS.contains(&key.as_str()) {
            return Err(Error::InvalidInput(format!(
                "field '{key}' is system-managed; only {SYSTEM_EDITABLE_FIELDS:?} are editable on system actions"
            )));
        }
    }

    // Webhook invariant (mirror of reconcile + dispatch guards): if the
    // patch touches `triggers` and the resulting set contains `webhook`,
    // the action must already have a credential_id. Reject at write time
    // rather than at dispatch, so the row never enters a misconfigured
    // state.
    if let Some(v) = obj.get("triggers") {
        if let Some(arr) = v.as_array() {
            let wants_webhook = arr
                .iter()
                .any(|t| t.as_str() == Some("webhook"));
            if wants_webhook && current.credential_id.is_none() {
                return Err(Error::InvalidInput(
                    "webhook trigger requires a credential_id; pair a device or source first".into(),
                ));
            }
        }
    }

    // Build the UPDATE statement. SQL has no "set only these keys" shortcut;
    // we use conditional per-field binds. The SET-clause loop and the bind
    // loop walk the fields in the same fixed order so the `$N` placeholders
    // line up 1-to-1 with the bind values. `config`/`triggers` are JSONB
    // columns, so their placeholders are cast (`::jsonb`); the string we bind
    // is a JSON document, mirroring the template upsert in `action_templates`.
    let mut sets: Vec<String> = Vec::new();
    let mut bind_idx = 0u32;
    let mut next = || {
        bind_idx += 1;
        bind_idx
    };

    if obj.contains_key("name") {
        sets.push(format!("name = ${}", next()));
    }
    if obj.contains_key("agent") {
        sets.push(format!("agent = ${}", next()));
    }
    if obj.contains_key("cron_schedule") {
        sets.push(format!("cron_schedule = ${}", next()));
    }
    if obj.contains_key("enabled") {
        sets.push(format!("enabled = ${}", next()));
    }
    if obj.contains_key("config") {
        sets.push(format!("config = ${}::jsonb", next()));
    }
    if obj.contains_key("condition") {
        sets.push(format!("condition = ${}", next()));
    }
    if obj.contains_key("triggers") {
        sets.push(format!("triggers = ${}::jsonb", next()));
    }
    if obj.contains_key("memory") {
        sets.push(format!("memory = ${}", next()));
    }

    sets.push("updated_at = now()".to_string());
    let id_param = next();
    let query = format!(
        "UPDATE app_applets SET {} WHERE id = ${}",
        sets.join(", "),
        id_param
    );

    // Now bind in the same order.
    let mut q = sqlx::query(&query);
    if let Some(v) = obj.get("name") {
        let s = v
            .as_str()
            .ok_or_else(|| Error::InvalidInput("name must be a string".into()))?;
        q = q.bind(s.to_string());
    }
    if let Some(v) = obj.get("agent") {
        if v.is_null() {
            q = q.bind(Option::<String>::None);
        } else {
            let s = v
                .as_str()
                .ok_or_else(|| Error::InvalidInput("agent must be a string or null".into()))?;
            q = q.bind(Some(s.to_string()));
        }
    }
    if let Some(v) = obj.get("cron_schedule") {
        if v.is_null() {
            q = q.bind(Option::<String>::None);
        } else {
            let s = v.as_str().ok_or_else(|| {
                Error::InvalidInput("cron_schedule must be a string or null".into())
            })?;
            validate_cron(s)?;
            q = q.bind(Some(s.to_string()));
        }
    }
    if let Some(v) = obj.get("enabled") {
        let b = v
            .as_bool()
            .ok_or_else(|| Error::InvalidInput("enabled must be a bool".into()))?;
        q = q.bind(b);
    }
    if let Some(v) = obj.get("config") {
        if !v.is_object() {
            return Err(Error::InvalidInput("config must be an object".into()));
        }
        q = q.bind(v.to_string());
    }
    if let Some(v) = obj.get("condition") {
        if v.is_null() {
            q = q.bind(Option::<String>::None);
        } else {
            let s = v.as_str().ok_or_else(|| {
                Error::InvalidInput("condition must be a string or null".into())
            })?;
            q = q.bind(Some(s.to_string()));
        }
    }
    if let Some(v) = obj.get("triggers") {
        let arr = v
            .as_array()
            .ok_or_else(|| Error::InvalidInput("triggers must be an array".into()))?;
        let triggers: Vec<String> = arr
            .iter()
            .map(|t| {
                t.as_str()
                    .map(String::from)
                    .ok_or_else(|| Error::InvalidInput("each trigger must be a string".into()))
            })
            .collect::<Result<_>>()?;
        for t in &triggers {
            if !matches!(t.as_str(), "cron" | "manual" | "tool" | "api" | "webhook") {
                return Err(Error::InvalidInput(format!(
                    "invalid trigger '{t}': must be one of cron, manual, tool, api, webhook"
                )));
            }
        }
        q = q.bind(serde_json::to_string(&triggers).map_err(|e| {
            Error::Other(format!("failed to serialize triggers: {e}"))
        })?);
    }
    if let Some(v) = obj.get("memory") {
        if v.is_null() {
            q = q.bind(Option::<String>::None);
        } else {
            let s = v
                .as_str()
                .ok_or_else(|| Error::InvalidInput("memory must be a string or null".into()))?;
            q = q.bind(Some(s.to_string()));
        }
    }
    q = q.bind(action_id);

    let res = q
        .execute(db)
        .await
        .map_err(|e| Error::Database(format!("failed to update action: {e}")))?;
    if res.rows_affected() == 0 {
        return Err(Error::NotFound(format!("Action not found: {action_id}")));
    }

    // Restore fidelity for chat-authored applets: the user's enable/disable
    // choice mirrors into the manifest's default_enabled, so a DB rebuilt
    // from disk comes back in the last chosen state (authoring plan §E).
    if current.owner == "ai" {
        if let Some(enabled) = obj.get("enabled").and_then(|v| v.as_bool()) {
            crate::action_templates::mirror_enabled_to_manifest(action_id, enabled);
        }
    }

    get_action(db, action_id).await
}

/// Slugify a name into a lowercase `a-z0-9_` identifier suitable for use as
/// an action id suffix.
fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_underscore = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_underscore = false;
        } else if !last_underscore && !out.is_empty() {
            out.push('_');
            last_underscore = true;
        }
    }
    // Trim trailing underscore
    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() {
        out.push_str("unnamed");
    }
    out
}

/// Validate a cron expression's shape (5 or 6 space-separated fields).
fn validate_cron(cron: &str) -> Result<()> {
    let fields: Vec<&str> = cron.split_whitespace().collect();
    if fields.len() < 5 || fields.len() > 6 {
        return Err(Error::InvalidInput(format!(
            "Invalid cron expression '{cron}': expected 5 or 6 fields"
        )));
    }
    Ok(())
}

// ============================================================================
// ActionRun CRUD
// ============================================================================

/// Create a new run for an action.
pub async fn create_run(
    db: &PgPool,
    action_id: Option<&str>,
    trigger: &str,
) -> Result<ActionRun> {
    let run_id = generate_id(
        RUN_PREFIX,
        &[
            action_id.unwrap_or("adhoc"),
            trigger,
            &chrono::Utc::now().to_rfc3339(),
        ],
    );

    let row = sqlx::query(
        r#"INSERT INTO app_applet_runs (id, action_id, trigger)
           VALUES ($1, $2, $3)
           RETURNING *"#,
    )
    .bind(&run_id)
    .bind(action_id)
    .bind(trigger)
    .fetch_one(db)
    .await?;

    // Onboarding (Tier 0/1): stamp the first init-sync start on the device this
    // action collects for. No-op for cloud sources / transforms (the action has
    // no credential, or the credential has no device_id → subquery is NULL).
    if let Some(aid) = action_id {
        let _ = sqlx::query(
            "UPDATE app_device SET init_sync_started_at = now() \
             WHERE id = (SELECT c.device_id FROM app_applets a \
                         JOIN credentials c ON c.id = a.credential_id WHERE a.id = $1) \
               AND init_sync_started_at IS NULL",
        )
        .bind(aid)
        .execute(db)
        .await;
    }

    run_from_row(&row)
}

/// Create a child run (for transform chaining).
pub async fn create_child_run(
    db: &PgPool,
    parent_run_id: &str,
    transform_stage: &str,
    trigger: &str,
) -> Result<ActionRun> {
    let run_id = generate_id(
        RUN_PREFIX,
        &[
            parent_run_id,
            transform_stage,
            &chrono::Utc::now().to_rfc3339(),
        ],
    );

    let row = sqlx::query(
        r#"INSERT INTO app_applet_runs (id, parent_run_id, transform_stage, trigger)
           VALUES ($1, $2, $3, $4)
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

/// Complete a run (success, error, skipped, cancelled).
pub async fn complete_run(
    db: &PgPool,
    run_id: &str,
    status: &str,
    records_processed: i64,
    error: Option<&str>,
    result_summary: Option<&str>,
) -> Result<()> {
    let error = error.map(|s| truncate_utf8_bytes(s, ERROR_MAX_BYTES));
    let result_summary = result_summary.map(|s| truncate_utf8_bytes(s, RESULT_SUMMARY_MAX_BYTES));

    sqlx::query(
        r#"UPDATE app_applet_runs
           SET status = $1, completed_at = now(),
               records_processed = $2, error = $3, result_summary = $4
           WHERE id = $5"#,
    )
    .bind(status)
    .bind(records_processed)
    .bind(error.as_deref())
    .bind(result_summary.as_deref())
    .bind(run_id)
    .execute(db)
    .await?;

    // Onboarding (Tier 0/1): a device's first successful run completes its
    // init backfill. No-op for cloud/transform runs (device_id resolves NULL).
    if status == "success" {
        let _ = sqlx::query(
            "UPDATE app_device SET init_sync_completed_at = now() \
             WHERE id = (SELECT c.device_id FROM app_applet_runs r \
                         JOIN app_applets a ON a.id = r.action_id \
                         JOIN credentials c ON c.id = a.credential_id WHERE r.id = $1) \
               AND init_sync_completed_at IS NULL",
        )
        .bind(run_id)
        .execute(db)
        .await;
    }

    Ok(())
}

/// How long a run may sit in `running` before the concurrency gate treats it as
/// dead. A run whose process crashed (or the box restarted mid-run) leaves a
/// stale `running` row; without an age bound that row would block the action
/// forever, since the only reaper (`cleanup_stale_runs`) runs at startup. This is
/// safely larger than `SUBPROCESS_TIMEOUT` (300s), which actively kills + records
/// `error` for hangs the box itself observes.
const RUN_STALE_TTL_SECS: f64 = 600.0;

/// Check if an action has an active (running) run, ignoring runs that have been
/// `running` longer than [`RUN_STALE_TTL_SECS`] (treated as dead).
pub async fn has_active_run(db: &PgPool, action_id: &str) -> Result<bool> {
    let result = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM app_applet_runs \
         WHERE action_id = $1 AND status = 'running' \
         AND started_at > now() - make_interval(secs => $2))",
    )
    .bind(action_id)
    .bind(RUN_STALE_TTL_SECS)
    .fetch_one(db)
    .await?;

    Ok(result)
}

/// Get the most recent run for an action.
pub async fn last_run(db: &PgPool, action_id: &str) -> Result<Option<ActionRun>> {
    let row = sqlx::query(
        "SELECT * FROM app_applet_runs WHERE action_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(action_id)
    .fetch_optional(db)
    .await?;

    row.as_ref().map(run_from_row).transpose()
}

/// Get a run by ID.
pub async fn get_run(db: &PgPool, run_id: &str) -> Result<ActionRun> {
    let row = sqlx::query("SELECT * FROM app_applet_runs WHERE id = $1")
        .bind(run_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Run not found: {}", run_id)))?;

    run_from_row(&row)
}

/// Query runs with filters.
pub async fn query_runs(
    db: &PgPool,
    action_id: Option<&str>,
    status: Option<&str>,
    limit: i64,
) -> Result<Vec<ActionRun>> {
    let rows = sqlx::query(
        r#"SELECT * FROM app_applet_runs
           WHERE ($1 IS NULL OR action_id = $2)
             AND ($3 IS NULL OR status = $4)
           ORDER BY created_at DESC
           LIMIT $5"#,
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
pub async fn cancel_run(db: &PgPool, run_id: &str) -> Result<()> {
    let affected = sqlx::query(
        r#"UPDATE app_applet_runs
           SET status = 'cancelled', completed_at = now()
           WHERE id = $1 AND status = 'running'"#,
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
pub async fn cleanup_stale_runs(db: &PgPool) -> Result<u64> {
    let affected = sqlx::query(
        r#"UPDATE app_applet_runs
           SET status = 'error', error = 'interrupted by restart', completed_at = now()
           WHERE status = 'running'"#,
    )
    .execute(db)
    .await?
    .rows_affected();

    Ok(affected)
}

/// Get child runs for a parent run.
pub async fn get_child_runs(db: &PgPool, parent_run_id: &str) -> Result<Vec<ActionRun>> {
    let rows = sqlx::query(
        "SELECT * FROM app_applet_runs WHERE parent_run_id = $1 ORDER BY created_at ASC",
    )
    .bind(parent_run_id)
    .fetch_all(db)
    .await?;

    rows.iter().map(run_from_row).collect()
}

// ============================================================================
// Row mapping helpers
// ============================================================================

pub fn action_from_row(row: &sqlx::postgres::PgRow) -> Result<Action> {
    let triggers_val: serde_json::Value = row
        .try_get("triggers")
        .unwrap_or_else(|_| serde_json::json!([]));
    let triggers: Vec<String> = serde_json::from_value(triggers_val).unwrap_or_default();

    let config: serde_json::Value = row
        .try_get("config")
        .unwrap_or_else(|_| serde_json::json!({}));

    // `command` is a JSON-encoded Vec<String> argv; None for face-only
    // (no execution) or pure-agent actions.
    let command_raw: Option<String> = row.try_get("command").ok();
    let command: Option<Vec<String>> = command_raw
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok());

    Ok(Action {
        id: row.try_get("id")?,
        owner: row.try_get("owner")?,
        name: row.try_get("name")?,
        agent: row.try_get("agent")?,
        cron_schedule: row.try_get("cron_schedule")?,
        enabled: row.try_get::<bool, _>("enabled")?,
        config,
        condition: row.try_get("condition")?,
        triggers,
        memory: row.try_get("memory")?,
        credential_id: row.try_get("credential_id")?,
        device_id: row.try_get("device_id")?,
        command,
        until: row.try_get("until").ok().flatten(),
        archived_at: row.try_get("archived_at").ok().flatten(),
        supervise: row.try_get("supervise").unwrap_or(false),
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

/// Derived display shape — the old `runtime` taxonomy, computed from fields:
/// supervise ⇒ service; no command and no agent ⇒ view (face-only);
/// otherwise function. Presentation only; nothing executes off this.
pub fn derived_runtime(a: &Action) -> &'static str {
    if a.supervise {
        "service"
    } else if a.command.as_ref().is_none_or(|c| c.is_empty())
        && a.agent.as_deref().is_none_or(|s| s.trim().is_empty())
    {
        "view"
    } else {
        "function"
    }
}

/// Clear the archived state — used when a user explicitly re-authors a
/// completed applet (re-arm). Distinct from reconcile, which must NOT
/// un-archive (that would resurrect completed one-shots on every boot).
pub async fn unarchive_action(db: &PgPool, action_id: &str) -> Result<()> {
    sqlx::query("UPDATE app_applets SET archived_at = NULL WHERE id = $1")
        .bind(action_id)
        .execute(db)
        .await
        .map_err(|e| Error::Database(format!("unarchive_action failed: {e}")))?;
    Ok(())
}

/// Archive an applet whose lifecycle completed: stamp `archived_at` and
/// disable it (the scheduler only loads `enabled = TRUE`, so an archived
/// applet stops waking without any scheduler-side special case).
pub async fn archive_action(db: &PgPool, action_id: &str) -> Result<()> {
    sqlx::query(
        "UPDATE app_applets SET archived_at = now(), enabled = FALSE \
         WHERE id = $1 AND archived_at IS NULL",
    )
    .bind(action_id)
    .execute(db)
    .await
    .map_err(|e| Error::Database(format!("archive_action failed: {e}")))?;
    Ok(())
}

fn run_from_row(row: &sqlx::postgres::PgRow) -> Result<ActionRun> {
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

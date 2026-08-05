//! `setup_applet` tool — materialize a chat-authored applet as a folder.
//!
//! Phase-3 authoring loop (docs/applet-authoring-plan.md). The model fills
//! flat params; this executor — trusted Rust — validates them (the "check"),
//! writes the folder at `applets root/user/<slug>/`, and reconciles under the
//! global mutex. The DB row is derived from disk like every other applet:
//! one door.
//!
//! Core invariant served here: **no path from model output to an enabled,
//! scheduled row without a user action** — any applet that crosses a boundary
//! (schedule/trigger, credential, recurring spend) materializes with
//! `default_enabled = false`; enabling is a user-surface action the tool
//! layer refuses (see `edit_applet`).

use serde::Serialize;
use sqlx::PgPool;

use super::executor::{ToolContext, ToolError, ToolResult};
use crate::scheduler::applets;

const FACE_HTML_MAX: usize = 48 * 1024;
const SCHEMA_SQL_MAX: usize = 16 * 1024;
const AGENT_MAX: usize = 24 * 1024;

/// Manifest shape we serialize. Field names match the template loader.
#[derive(Serialize)]
struct ManifestOut {
    name: String,
    description: String,
    owner: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    schedule: Option<String>,
    triggers: Vec<String>,
    default_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    until: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    config: Option<toml::Value>,
}

/// Execute the setup_applet tool.
pub async fn execute(
    pool: &PgPool,
    arguments: serde_json::Value,
    context: &ToolContext,
) -> Result<ToolResult, ToolError> {
    // Owned copy up front — borrowing `context` across the awaits below
    // trips rustc's higher-ranked Send inference inside the agent stream.
    let chat_id: Option<String> = context.chat_id.clone();

    // ---- 1. Parse params -------------------------------------------------
    let name = req_str(&arguments, "name")?;
    let description = req_str(&arguments, "description")?;
    // `agent` is OPTIONAL. A pure dashboard/View (a face that reads data) or a
    // Tracker (schema + face) has no server-side run and needs no prompt — the
    // face queries directly. Only Reflect/Rule applets that DO something each
    // run (write a page, post to chat, compute) carry an agent.
    let agent = opt_str(&arguments, "agent")
        .or_else(|| opt_str(&arguments, "instruction"));
    let schedule = opt_str(&arguments, "schedule")
        .or_else(|| opt_str(&arguments, "cron_schedule"));
    let condition = opt_str(&arguments, "condition");
    let until = opt_str(&arguments, "until");
    let schema_sql = opt_str(&arguments, "schema_sql");
    let face_html = opt_str(&arguments, "face_html");
    let limits = arguments.get("limits").cloned().filter(|v| v.is_object());

    // An applet must DO or SHOW something: a prompt to run, or a face to view.
    if agent.is_none() && face_html.is_none() {
        return Ok(ToolResult::success(serde_json::json!({
            "status": "check_failed",
            "findings": [finding("agent", "an applet needs either an `agent` prompt (to run) or a `face_html` (to show) — a dashboard is face-only, a reminder is agent-only", None)],
        })));
    }

    let triggers: Vec<String> = arguments
        .get("triggers")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_else(|| {
            if schedule.is_some() {
                vec!["cron".into(), "manual".into(), "tool".into()]
            } else {
                vec!["manual".into(), "tool".into()]
            }
        });

    // ---- 2. Check (validate BEFORE anything touches disk) ----------------
    let mut findings: Vec<serde_json::Value> = Vec::new();

    let slug = slugify(&name);
    if slug.is_empty() {
        findings.push(finding("name", "name produces an empty slug", None));
    }

    for t in &triggers {
        if !matches!(t.as_str(), "cron" | "manual" | "tool" | "api" | "webhook" | "message") {
            findings.push(finding(
                "triggers",
                &format!("invalid trigger '{t}'"),
                Some("one of: cron, manual, tool, api, webhook, message"),
            ));
        }
    }

    if let Some(cron) = schedule.as_deref() {
        let fields = cron.split_whitespace().count();
        if !(5..=6).contains(&fields) {
            findings.push(finding(
                "schedule",
                &format!("cron '{cron}' has {fields} fields"),
                Some("6 fields (seconds first), box-local timezone — e.g. '0 0 6 * * *' = daily 6am"),
            ));
        }
    }

    if agent.as_deref().is_some_and(|a| a.len() > AGENT_MAX) {
        findings.push(finding("agent", "prompt too large (24KB max)", None));
    }
    if let Some(f) = &face_html {
        if f.len() > FACE_HTML_MAX {
            findings.push(finding("face_html", "face too large (48KB max)", None));
        }
    }

    let sql_checks: Vec<(&'static str, String)> = [
        ("condition", condition.clone()),
        ("until", until.clone()),
    ]
    .into_iter()
    .filter_map(|(f, v)| v.map(|sql| (f, sql)))
    .filter(|(f, sql)| !(*f == "until" && sql.eq_ignore_ascii_case("once")))
    .collect();
    for (field, sql) in sql_checks {
        if let Err(e) = explain_bool_expr(pool, &sql).await {
            let suggestion = did_you_mean(pool, &e).await;
            findings.push(finding(field, &e, suggestion.as_deref()));
        }
    }

    if let Some(ddl) = schema_sql.as_deref() {
        if ddl.len() > SCHEMA_SQL_MAX {
            findings.push(finding("schema_sql", "schema too large (16KB max)", None));
        } else if let Err(e) = crate::tools::applet_schema::check(pool, ddl, &slug).await {
            findings.push(finding("schema_sql", &e, None));
        } else {
            // The failure a dry-run cannot see: DDL that is valid, applies
            // cleanly, and changes nothing. `CREATE TABLE IF NOT EXISTS` on an
            // existing table silently drops every column added since — so the
            // model would believe a column exists and write a prompt that uses
            // it, failing at runtime nightly and forever.
            for drift in crate::tools::applet_schema::detect_drift(pool, ddl).await {
                findings.push(finding(
                    "schema_sql",
                    &drift.message(),
                    Some(&drift.suggestion()),
                ));
            }
        }
    }

    // A limit nobody enforces is worse than no limit: it reads as protection
    // on the gate and does nothing. So an unknown key is a finding, not a
    // silently-ignored field. (This is how `timeout` came to be advertised
    // for months while only `timeout_s` was ever read.)
    for f in check_limits(limits.as_ref()) {
        findings.push(f);
    }

    if !findings.is_empty() {
        return Ok(ToolResult::success(serde_json::json!({
            "status": "check_failed",
            "findings": findings,
            "hint": "fix the findings and call setup_applet again — nothing was created",
        })));
    }

    // ---- 3. Resolve the folder (collision-safe) --------------------------
    // Authoring always writes to the state root — never the shipped tree,
    // which is root-owned and replaced wholesale on upgrade.
    let root = crate::applet_templates::state_root();
    // A slug is "ours to update" only if an existing folder's manifest has the
    // same name. A different applet that merely collapses to the same slug
    // gets a numeric suffix instead of silently overwriting it.
    let slug = disambiguate_slug(&root, &slug, name);
    let dir = root.join("user").join(&slug);
    let existed = dir.join("manifest.toml").is_file();
    let applet_id = format!(
        "{}{slug}",
        crate::scheduler::applets::USER_APPLET_PREFIX
    );

    // Boundary predicate: schedule/trigger beyond manual+tool = unattended.
    let crosses_boundary = schedule.is_some()
        || triggers.iter().any(|t| matches!(t.as_str(), "api" | "webhook"));

    // Re-gate logic: read the current row (if any) so an update can preserve
    // the user's enable choice AND force-disable when a boundary is newly
    // added to an already-enabled applet (the gate invariant).
    let (was_boundary, was_enabled) = if existed {
        match applets::get_applet(pool, &applet_id).await {
            Ok(a) => (
                a.cron_schedule.is_some()
                    || a.triggers.iter().any(|t| t == "api" || t == "webhook"),
                a.enabled,
            ),
            Err(_) => (false, false),
        }
    } else {
        (false, false)
    };
    let re_gate = existed && crosses_boundary && !was_boundary;

    // Manifest default_enabled: fresh → gate on boundary; re-gate → false;
    // otherwise preserve the user's last choice (restore fidelity).
    let default_enabled = if !existed {
        !crosses_boundary
    } else if re_gate {
        false
    } else {
        was_enabled
    };

    let mut config = toml::value::Table::new();
    if let Some(cid) = &chat_id {
        config.insert("chat_id".into(), toml::Value::String(cid.clone()));
    }
    if let Some(lim) = &limits {
        if let Ok(v) = toml_from_json(lim) {
            config.insert("limits".into(), v);
        }
    }

    let manifest = ManifestOut {
        name: name.to_string(),
        description: description.to_string(),
        owner: "ai",
        schedule: schedule.clone(),
        triggers: triggers.clone(),
        default_enabled,
        condition: condition.clone(),
        until: until.clone(),
        agent: agent.clone(),
        config: if config.is_empty() { None } else { Some(toml::Value::Table(config)) },
    };
    let manifest_toml = toml::to_string_pretty(&manifest)
        .map_err(|e| ToolError::ExecutionFailed(format!("manifest serialize failed: {e}")))?;

    // ---- 4. Write the folder; reconcile applies the schema ----------------
    // Each submission is one numbered, append-only migration rather than a
    // rewritten file. Identical DDL — the ordinary re-setup that only changed
    // the prompt — matches the last recorded checksum and is recognized as
    // already applied, so nothing is appended and nothing is re-run.
    //
    // The DDL is NOT applied here. Reconcile's replay pass is the single apply
    // path, so a folder that arrives any other way (git import, restored
    // backup) reaches the same tables by the same code. Applying here as well
    // would run every new version twice — harmless for a CREATE, an outright
    // error for the ALTER that an edit now produces.
    //
    // What used to justify applying first was the orphan row: an applet whose
    // tables were never created. The transactional dry-run in the check above
    // covers that better than ordering did — it runs the real DDL and rolls it
    // back — and the post-reconcile verification below closes the rest.
    let mut schema_version: Option<i32> = None;
    let mut schema_file: Option<String> = None;
    if let Some(ddl) = &schema_sql {
        use crate::tools::applet_schema as schema;
        let already = schema::applied(pool, &applet_id).await.unwrap_or_default();
        schema_version = Some(match schema::classify(ddl, &already) {
            schema::Submission::AlreadyApplied { version } => version,
            schema::Submission::NewVersion { version } => version,
        });
    }

    std::fs::create_dir_all(&dir)
        .map_err(|e| ToolError::ExecutionFailed(format!("mkdir failed: {e}")))?;
    std::fs::write(dir.join("manifest.toml"), &manifest_toml)
        .map_err(|e| ToolError::ExecutionFailed(format!("manifest write failed: {e}")))?;
    if let (Some(ddl), Some(version)) = (&schema_sql, schema_version) {
        use crate::tools::applet_schema as schema;
        let schema_dir = dir.join(schema::SCHEMA_DIR);
        std::fs::create_dir_all(&schema_dir)
            .map_err(|e| ToolError::ExecutionFailed(format!("schema dir failed: {e}")))?;
        let file_name = format!("{version:04}_schema.sql");
        let path = schema_dir.join(&file_name);
        // Written unconditionally, including for AlreadyApplied: the row may
        // exist on a box whose folder was restored from elsewhere, and the
        // folder is what a fresh box replays.
        std::fs::write(&path, ddl)
            .map_err(|e| ToolError::ExecutionFailed(format!("schema write failed: {e}")))?;
        schema_file = Some(file_name);
    }
    if let Some(html) = &face_html {
        let face_dir = dir.join("face");
        std::fs::create_dir_all(&face_dir)
            .map_err(|e| ToolError::ExecutionFailed(format!("face dir failed: {e}")))?;
        std::fs::write(face_dir.join("index.html"), html)
            .map_err(|e| ToolError::ExecutionFailed(format!("face write failed: {e}")))?;
    }

    crate::applet_templates::reload_and_reconcile(pool)
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("reconcile failed: {e}")))?;

    // Reconcile ran the pending version as part of its replay pass. Confirm it
    // actually landed rather than trusting that it did: the whole point of
    // this change is that a schema apply which quietly does nothing must never
    // again be reported as success.
    if let (Some(version), Some(_)) = (schema_version, &schema_file) {
        use crate::tools::applet_schema as schema;
        let landed = schema::applied(pool, &applet_id)
            .await
            .unwrap_or_default()
            .iter()
            .any(|a| a.version == version);
        if !landed {
            return Ok(ToolResult::success(serde_json::json!({
                "status": "error",
                "error": format!(
                    "the applet was created but schema version {version} did not apply — \
                     check the box logs for the DDL error"
                ),
            })));
        }
        let _ = crate::server::faces::ensure_applet_db_grants(pool).await;
    }

    // Explicit re-author re-arms a completed applet. Reconcile never
    // un-archives (that would resurrect one-shots on every boot), so clear it
    // here — this path is only ever reached by the user-driven authoring tool.
    if existed {
        let _ = applets::unarchive_applet(pool, &applet_id).await;
    }

    // Re-gate: reconcile deliberately preserves `enabled`, so an applet that
    // just gained a boundary while enabled is still enabled — flip it off so
    // the user must re-enable (the gate invariant holds on updates too).
    if re_gate {
        let _ = applets::update_applet(
            pool,
            &applet_id,
            &serde_json::json!({ "enabled": false }),
        )
        .await;
    }

    // ---- 5. Proposal ------------------------------------------------------
    let mut capabilities = vec!["reads your data (read-only SQL)".to_string()];
    if schema_sql.is_some() {
        capabilities.push(format!("owns tables in schema applet_{slug}"));
    }
    if face_html.is_some() {
        capabilities.push("has a face (sandboxed page)".into());
    }
    // Only agent-bearing applets run and deliver; a face-only View just renders.
    if agent.is_some() && chat_id.is_some() {
        capabilities.push("posts run results to this chat".into());
    }

    // Only an applet with a prompt spends anything — a face renders and a
    // subprocess runs compiled code, and quoting a model cost for either would
    // be inventing a number. The per-run figure is a slot constant, not a
    // measurement, which is why the card calls it an estimate.
    let runs_per_day = schedule.as_deref().map(estimate_runs_per_day);
    let est_cost_per_day = runs_per_day
        .filter(|_| agent.is_some())
        .map(|r| r * 0.01);

    Ok(ToolResult::success(serde_json::json!({
        "status": if existed { "updated" } else { "created" },
        "applet_id": applet_id,
        "name": name,
        "slug": slug,
        "folder": format!("user/{slug}"),
        "enabled": default_enabled,
        // The card renders from these; the string below is for the model, so
        // it says the same thing in its own reply.
        "description": description,
        "schedule": schedule,
        "gated": !default_enabled,
        "gate": if !default_enabled {
            "DISABLED — an Enable card is shown to the user in this chat. Say what it does and that it is waiting for them; do NOT tell them to go to the applet page."
        } else {
            "no boundary crossed: enabled already, nothing for the user to approve"
        },
        "lifecycle": until.as_deref().map(|u| {
            if u.eq_ignore_ascii_case("once") { "once" } else { "until" }
        }).unwrap_or("forever"),
        "capabilities": capabilities,
        "estimated_cost_per_day": est_cost_per_day,
        "manifest": manifest_toml,
    })))
}

// ============================================================================
// Check helpers
// ============================================================================

fn req_str<'a>(args: &'a serde_json::Value, key: &str) -> Result<&'a str, ToolError> {
    args.get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ToolError::InvalidParameters(format!("{key} is required")))
}

fn opt_str(args: &serde_json::Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

fn finding(field: &str, error: &str, suggestion: Option<&str>) -> serde_json::Value {
    serde_json::json!({ "field": field, "error": error, "suggestion": suggestion })
}

/// The `limits` keys the runner actually enforces, with the unit each is read
/// in. Anything outside this set is rejected at check time rather than written
/// into `config.limits` to be ignored forever.
const LIMIT_KEYS: &[(&str, &str)] = &[
    ("max_llm_cost", "dollars — ceiling on model spend within one run, e.g. 0.25"),
    ("max_llm_cost_per_day", "dollars — ceiling on model spend across a rolling 24h"),
    ("max_runs_per_hour", "whole number of runs"),
    ("max_runs_per_day", "whole number of runs (`max_runs` is accepted for this)"),
    ("timeout_s", "seconds of wall clock for the subprocess phase"),
];

/// Validate the `limits` object: known keys, right types, sane values.
fn check_limits(limits: Option<&serde_json::Value>) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    let Some(obj) = limits.and_then(|v| v.as_object()) else {
        return out;
    };

    let known: Vec<&str> = LIMIT_KEYS.iter().map(|(k, _)| *k).collect();
    for (key, value) in obj {
        // `max_runs` and `timeout` are accepted aliases, not typos.
        let canonical = matches!(key.as_str(), "max_runs" | "timeout")
            || known.contains(&key.as_str());
        if !canonical {
            let suggestion = known
                .iter()
                .map(|k| (levenshtein(key, k), *k))
                .min_by_key(|(d, _)| *d)
                .filter(|(d, _)| *d <= 4)
                .map(|(_, k)| format!("did you mean `{k}`?"))
                .unwrap_or_else(|| format!("enforced limits are: {}", known.join(", ")));
            out.push(finding(
                "limits",
                &format!("unknown limit `{key}` — it would be stored and never enforced"),
                Some(&suggestion),
            ));
            continue;
        }

        let is_money = key.starts_with("max_llm_cost");
        let ok = if is_money {
            value.as_f64().is_some_and(|d| d.is_finite() && d >= 0.0)
        } else {
            value.as_u64().is_some()
        };
        if !ok {
            let unit = LIMIT_KEYS
                .iter()
                .find(|(k, _)| *k == key || (*k == "max_runs_per_day" && key == "max_runs"))
                .map(|(_, u)| *u)
                .unwrap_or("a positive number");
            out.push(finding(
                "limits",
                &format!("`{key}` must be a positive number — got {value}"),
                Some(unit),
            ));
        }
    }
    out
}

/// Return a slug that is either free, or already owned by an applet with the
/// SAME name (an update). If the base slug is taken by a DIFFERENT applet
/// (two names collapsing to one slug), append `_2`, `_3`, … so we never
/// overwrite an unrelated applet.
fn disambiguate_slug(root: &std::path::Path, base: &str, name: &str) -> String {
    let owns = |slug: &str| -> Option<bool> {
        // Some(true) = folder exists and its manifest name matches (ours to update)
        // Some(false) = folder exists, different name (collision)
        // None = free
        let mf = root.join("user").join(slug).join("manifest.toml");
        let text = std::fs::read_to_string(&mf).ok()?;
        let existing_name = text
            .parse::<toml::Value>()
            .ok()
            .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(String::from))
            .unwrap_or_default();
        Some(existing_name == name)
    };
    match owns(base) {
        None | Some(true) => return base.to_string(),
        Some(false) => {}
    }
    for n in 2..=99 {
        let cand = format!("{base}_{n}");
        match owns(&cand) {
            None | Some(true) => return cand,
            Some(false) => continue,
        }
    }
    base.to_string()
}

pub(crate) fn slugify(name: &str) -> String {
    let mut slug = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('_') && !slug.is_empty() {
            slug.push('_');
        }
    }
    let slug = slug.trim_end_matches('_');
    slug[..slug.len().min(48)].to_string()
}

/// EXPLAIN a boolean SQL expression under the hardened read-only path.
async fn explain_bool_expr(pool: &PgPool, expr: &str) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("SET TRANSACTION READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("SET LOCAL statement_timeout = '5s'")
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    let sql = format!("EXPLAIN SELECT (({expr}))::boolean");
    let res = sqlx::query(&sql).fetch_all(&mut *tx).await;
    tx.rollback().await.ok();
    res.map(|_| ()).map_err(|e| e.to_string())
}


/// Did-you-mean for unknown relation/column errors, against the live catalog.
async fn did_you_mean(pool: &PgPool, error: &str) -> Option<String> {
    let needle = error
        .split('"')
        .nth(1)
        .filter(|s| !s.is_empty())?
        .to_lowercase();
    let names: Vec<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables \
         WHERE table_schema = 'public' AND (table_name LIKE 'data\\_%' OR table_name LIKE 'wiki\\_%') \
         UNION \
         SELECT column_name FROM information_schema.columns \
         WHERE table_schema = 'public' AND (table_name LIKE 'data\\_%' OR table_name LIKE 'wiki\\_%')",
    )
    .fetch_all(pool)
    .await
    .ok()?;

    let mut best: Option<(usize, &String)> = None;
    for n in &names {
        let d = levenshtein(&needle, &n.to_lowercase());
        if best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, n));
        }
    }
    best.filter(|(d, _)| *d <= 3)
        .map(|(_, n)| format!("did you mean \"{n}\"?"))
}

fn levenshtein(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Crude runs/day estimate from a 6-field cron, for the cost line.
fn estimate_runs_per_day(cron: &str) -> f64 {
    let f: Vec<&str> = cron.split_whitespace().collect();
    let (min, hour, dow) = match f.len() {
        6 => (f[1], f[2], f[5]),
        5 => (f[0], f[1], f[4]),
        _ => return 1.0,
    };
    let per_day = if let Some(n) = min.strip_prefix("*/").and_then(|n| n.parse::<f64>().ok()).filter(|n| *n > 0.0) {
        1440.0 / n
    } else if min == "*" {
        1440.0
    } else if let Some(n) = hour.strip_prefix("*/").and_then(|n| n.parse::<f64>().ok()).filter(|n| *n > 0.0) {
        24.0 / n
    } else if hour == "*" {
        24.0
    } else {
        1.0
    };
    if dow != "*" && per_day <= 1.0 {
        per_day / 7.0 * dow.split(',').count() as f64
    } else {
        per_day
    }
}

fn toml_from_json(v: &serde_json::Value) -> Result<toml::Value, String> {
    toml::Value::try_from(v).map_err(|e| e.to_string())
}

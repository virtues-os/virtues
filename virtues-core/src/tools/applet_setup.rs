//! The `applet` tool's `setup` action — materialize a chat-authored applet
//! as a folder.
//!
//! The chat authoring loop (agents/record/applets.md). The model fills
//! flat params; this executor — trusted Rust — validates them (the "check"),
//! writes the folder at `<state root>/user/<name>/` with its `SKILL.md`, and
//! reconciles under the global mutex. The DB row is derived from disk like
//! every other applet: one door.
//!
//! Core invariant served here: **no path from model output to an enabled,
//! scheduled row without a user action** — any applet that crosses a boundary
//! (schedule/trigger, credential, recurring spend) materializes with
//! `default_enabled = false`; enabling is a user-surface action the tool
//! layer refuses (see the `edit` action in `applet_management`).

use sqlx::PgPool;

use super::executor::{ToolContext, ToolError, ToolResult};
use crate::scheduler::applets;

const FACE_HTML_MAX: usize = 48 * 1024;
const SCHEMA_SQL_MAX: usize = 16 * 1024;
const AGENT_MAX: usize = 24 * 1024;

/// The authoring contract, one source: applets/AGENTS.md is what an agent in
/// the repo reads and what `applet {"action": "setup", "guide": true}` hands the in-box
/// assistant. The tool definition stays short because every chat step
/// re-sends it; the guide is fetched once, by the chat that is authoring.
const AGENTS_MD: &str = include_str!("../../../applets/AGENTS.md");

/// The guide as the chat reads it: AGENTS.md up to its pointer for builtin
/// (Rust) applet development, which is about this repo, not about authoring.
pub(crate) fn authoring_guide() -> &'static str {
    AGENTS_MD
        .split_once("\n## For builtin (Rust) applet development")
        .map_or(AGENTS_MD, |(guide, _)| guide)
        .trim_end()
}

/// `{"guide": true}` asks for the guide and creates nothing.
pub(crate) fn wants_guide(arguments: &serde_json::Value) -> bool {
    arguments.get("guide").and_then(|v| v.as_bool()) == Some(true)
}

/// `{"check_only": true}` runs the check and creates nothing: the dry run an
/// author iterates against (`virtues applet check`), and the reason it needs no
/// "I allow" in chat.
pub(crate) fn wants_check_only(arguments: &serde_json::Value) -> bool {
    arguments.get("check_only").and_then(|v| v.as_bool()) == Some(true)
}

/// The daily spend limit, in dollars, an applet put by an outside agent (the
/// CLI's `applet put`, or a setup through `/api/mcp`) gets when it has a prompt
/// and sets none. A scheduled applet written by an outside agent runs with
/// nobody watching; this is what stops it running up a bill overnight. The
/// owner can raise or remove it like any limit.
pub(crate) const DEFAULT_MAX_LLM_COST_PER_DAY: f64 = 1.0;

/// Adds [`DEFAULT_MAX_LLM_COST_PER_DAY`] to setup arguments that carry a
/// prompt and no daily limit of their own.
pub(crate) fn with_default_spend_cap(arguments: &mut serde_json::Value) {
    let has_prompt = arguments.get("agent").or_else(|| arguments.get("instruction")).is_some();
    if has_prompt && arguments["limits"].get("max_llm_cost_per_day").is_none() {
        if !arguments["limits"].is_object() {
            arguments["limits"] = serde_json::json!({});
        }
        arguments["limits"]["max_llm_cost_per_day"] = serde_json::json!(DEFAULT_MAX_LLM_COST_PER_DAY);
    }
}

/// The authored fields, parsed once for the check and the write.
pub(crate) struct Draft {
    name: String,
    description: String,
    agent: Option<String>,
    schedule: Option<String>,
    condition: Option<String>,
    until: Option<String>,
    schema_sql: Option<String>,
    face_html: Option<String>,
    limits: Option<serde_json::Value>,
    triggers: Vec<String>,
    /// Who may start it by hand: `chat` (the `/` menu) and `assistant`, the
    /// latter also from `tool` in `triggers`.
    invoke: Vec<String>,
    /// `allowed-tools`: what its prompt may call. None is the default set.
    tools: Option<Vec<String>>,
    /// `standard` or `deep`. None is the background model for an unattended
    /// applet and Standard for one `/name` starts.
    model: Option<String>,
}

impl Draft {
    pub(crate) fn parse(arguments: &serde_json::Value) -> Result<Self, ToolError> {
        let name = req_str(arguments, "name")?.to_string();
        let description = req_str(arguments, "description")?.to_string();
        // `agent` is OPTIONAL. A pure dashboard/View (a face that reads data)
        // or a Tracker (schema + face) has no server-side run and needs no
        // prompt — the face queries directly. Only Reflect/Rule applets that
        // DO something each run (write a page, post to chat, compute) carry
        // an agent.
        let agent = opt_str(arguments, "agent").or_else(|| opt_str(arguments, "instruction"));
        let schedule = opt_str(arguments, "schedule");
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
        Ok(Self {
            name,
            description,
            agent,
            schedule,
            condition: opt_str(arguments, "condition"),
            until: opt_str(arguments, "until"),
            schema_sql: opt_str(arguments, "schema_sql"),
            face_html: opt_str(arguments, "face_html"),
            limits: arguments.get("limits").cloned().filter(|v| v.is_object()),
            triggers,
            invoke: str_list(arguments, "invoke").unwrap_or_default(),
            tools: str_list(arguments, "tools"),
            model: opt_str(arguments, "model"),
        })
    }
}

/// The check: every finding a draft has, each naming its fix. Validates
/// BEFORE anything touches disk, and is the whole of what `check_only` runs.
pub(crate) async fn check(pool: &PgPool, draft: &Draft) -> Vec<serde_json::Value> {
    let Draft { name, agent, schedule, condition, until, schema_sql, face_html, limits, triggers, .. } = draft;
    let mut findings: Vec<serde_json::Value> = Vec::new();
    findings.extend(check_invoke(draft));

    // An applet must DO or SHOW something: a prompt to run, or a face to view.
    if agent.is_none() && face_html.is_none() {
        findings.push(finding("agent", "an applet needs either an `agent` prompt (to run) or a `face_html` (to show) — a dashboard is face-only, a reminder is agent-only", None));
    }

    let slug = slugify(name);
    if slug.is_empty() {
        findings.push(finding("name", "name produces an empty slug", None));
    }

    for t in triggers {
        match t.as_str() {
            "cron" | "manual" | "tool" | "message" => {}
            "api" => findings.push(finding("triggers", "the `api` trigger never fired and is gone", Some("leave it out"))),
            "webhook" => findings.push(finding(
                "triggers",
                "a `webhook` applet needs a connected source, which setup cannot give it",
                Some("leave it out"),
            )),
            _ => findings.push(finding(
                "triggers",
                &format!("invalid trigger '{t}'"),
                Some("one of: cron, manual, tool, message"),
            )),
        }
    }
    match until.as_deref().map(str::trim) {
        Some(u) if u.eq_ignore_ascii_case("once") && schedule.is_none() => findings.push(finding(
            "until",
            "`until: \"once\"` runs the schedule once, and there is no schedule",
            Some("add a schedule, or leave `until` out"),
        )),
        Some(u) if !u.is_empty() && !u.eq_ignore_ascii_case("once") => findings.push(finding(
            "until",
            "an applet no longer ends on a SQL check",
            Some("use \"once\" for a one-off, or leave `until` out"),
        )),
        _ => {}
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
    if let Some(f) = face_html {
        if f.len() > FACE_HTML_MAX {
            findings.push(finding("face_html", "face too large (48KB max)", None));
        }
    }

    if let Some(sql) = condition {
        if let Err(e) = explain_bool_expr(pool, sql).await {
            let suggestion = did_you_mean(pool, &e).await;
            findings.push(finding("condition", &e, suggestion.as_deref()));
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

    // The soft failure the whole authoring contract warns about: a prompt that
    // names a table which does not exist fails quietly, nightly, forever. It
    // was documented as unmachine-checkable — "you are the check" — and then I
    // wrote a shipped prompt naming three columns that were not there. Prose
    // is not fully checkable, but a table name is a token, and a typo in one
    // is the common case.
    if let Some(prompt) = agent.as_deref() {
        for f in check_prompt_tables(pool, prompt).await {
            findings.push(f);
        }
    }

    // A limit nobody enforces is worse than no limit: it reads as protection
    // on the gate and does nothing. So an unknown key is a finding, not a
    // silently-ignored field. (This is how `timeout` came to be advertised
    // for months while only `timeout_s` was ever read.)
    for f in check_limits(limits.as_ref()) {
        findings.push(f);
    }

    findings
}

/// Execute the applet tool's setup action.
pub async fn execute(
    pool: &PgPool,
    arguments: serde_json::Value,
    context: &ToolContext,
) -> Result<ToolResult, ToolError> {
    // Owned copy up front — borrowing `context` across the awaits below
    // trips rustc's higher-ranked Send inference inside the agent stream.
    let chat_id: Option<String> = context.chat_id.clone();

    if wants_guide(&arguments) {
        return Ok(ToolResult::success(serde_json::json!({ "guide": authoring_guide() })));
    }

    // ---- 1. Parse, 2. Check ---------------------------------------------
    let draft = Draft::parse(&arguments)?;
    let findings = check(pool, &draft).await;

    if wants_check_only(&arguments) {
        return Ok(ToolResult::success(serde_json::json!({
            "status": if findings.is_empty() { "ok" } else { "check_failed" },
            "findings": findings,
        })));
    }
    if !findings.is_empty() {
        return Ok(ToolResult::success(serde_json::json!({
            "status": "check_failed",
            "findings": findings,
            "hint": "fix the findings and call setup again; nothing was created",
        })));
    }

    let Draft { name, description, agent, schedule, condition, until, schema_sql, face_html, limits, triggers, invoke, tools, model } =
        draft;
    let name = name.as_str();
    let description = description.as_str();

    // ---- 3. Resolve the folder ------------------------------------------
    // Authoring always writes to the state root — never the shipped tree,
    // which is root-owned and replaced wholesale on upgrade. The folder is
    // the spec's name, lowercase words joined by hyphens, so the same name is
    // the same applet: "Calorie tracker" is user/calorie-tracker.
    let root = crate::applet_templates::state_root();
    let spec = spec_name(name);
    let slug = spec.replace('-', "_");
    let rel = format!("user/{spec}");
    let dir = root.join(&rel);
    let existed = dir.join("SKILL.md").is_file();
    let applet_id = format!("{}{slug}", crate::scheduler::applets::USER_APPLET_PREFIX);

    // Boundary predicate: a schedule = unattended.
    let crosses_boundary = schedule.is_some();

    // Re-gate logic: read the current row (if any) so an update can preserve
    // the user's enable choice AND force-disable when a boundary is newly
    // added to an already-enabled applet (the gate invariant).
    let (was_boundary, was_enabled) = if existed {
        match applets::get_applet(pool, &applet_id).await {
            Ok(a) => (a.schedule.is_some(), a.enabled),
            Err(_) => (false, false),
        }
    } else {
        (false, false)
    };
    let re_gate = existed && crosses_boundary && !was_boundary;

    // Fresh → gate on boundary; re-gate → off; otherwise preserve the user's
    // last choice (restore fidelity).
    let default_enabled = if !existed {
        !crosses_boundary
    } else if re_gate {
        false
    } else {
        was_enabled
    };

    // ---- 4. The SKILL.md ---------------------------------------------------
    // When it starts: the schedule, or its next time for a one-off.
    let once = until.as_deref().is_some_and(|u| u.trim().eq_ignore_ascii_case("once"));
    let trigger = match schedule.as_deref() {
        Some(cron) if once => {
            let tz = crate::scheduler::resolve_schedule_tz(pool).await;
            Some(
                crate::applet_templates::next_once(cron, tz, chrono::Utc::now())
                    .ok_or_else(|| ToolError::InvalidParameters(format!("`{cron}` has no next time to run once")))?,
            )
        }
        Some(cron) => Some(format!("cron {cron}")),
        None => None,
    };
    let mut invoke = invoke;
    if triggers.iter().any(|t| t == "tool") && !invoke.iter().any(|i| i == "assistant") {
        invoke.push("assistant".into());
    }
    if invoke.is_empty() {
        invoke.push("none".into());
    }
    let chat_started = invoke.iter().any(|i| i == "chat");
    let md = crate::applet_templates::SkillMd {
        name: spec.clone(),
        description: description.to_string(),
        allowed_tools: tools.clone(),
        invoke,
        trigger,
        // A condition is SQL that finds a row when the run should happen.
        run: condition.as_deref().map(|c| format!("sql select 1 where ({c})")),
        limit: limits.as_ref().and_then(|l| l.as_object()).and_then(crate::applet_templates::limit_from_config),
        deep: model.as_deref() == Some("deep"),
        // Unattended prompts run on the background model unless they ask.
        lite: agent.is_some() && model.is_none() && !chat_started,
        enabled: default_enabled,
        body: agent.clone().unwrap_or_default(),
        ..Default::default()
    };
    let skill_md = crate::applet_templates::render_skill_md(&md).map_err(ToolError::ExecutionFailed)?;

    // ---- 5. Write the folder; reconcile applies the schema ----------------
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
    if let Some(html) = &face_html {
        std::fs::write(dir.join("view.html"), html)
            .map_err(|e| ToolError::ExecutionFailed(format!("view write failed: {e}")))?;
    }
    if let (Some(ddl), Some(version)) = (&schema_sql, schema_version) {
        use crate::tools::applet_schema as schema;
        let tables_dir = dir.join(schema::TABLES_DIR);
        std::fs::create_dir_all(&tables_dir)
            .map_err(|e| ToolError::ExecutionFailed(format!("tables dir failed: {e}")))?;
        let file_name = format!("{version:04}_schema.sql");
        // Written unconditionally, including for AlreadyApplied: the row may
        // exist on a box whose folder was restored from elsewhere, and the
        // folder is what a fresh box replays.
        std::fs::write(tables_dir.join(&file_name), ddl)
            .map_err(|e| ToolError::ExecutionFailed(format!("tables write failed: {e}")))?;
        schema_file = Some(file_name);
    }
    // What is written must load, or the next reconcile would skip it.
    if let Err(problems) = crate::applet_templates::check_skill_md(&skill_md, &dir, &rel) {
        let findings: Vec<serde_json::Value> = problems.iter().map(|p| finding("skill_md", p, None)).collect();
        return Ok(ToolResult::success(serde_json::json!({ "status": "check_failed", "findings": findings })));
    }
    // Written whole, then moved into place, so a reader never sees half a file.
    let tmp = dir.join(".SKILL.md.tmp");
    std::fs::write(&tmp, &skill_md).map_err(|e| ToolError::ExecutionFailed(format!("write failed: {e}")))?;
    std::fs::rename(&tmp, dir.join("SKILL.md")).map_err(|e| ToolError::ExecutionFailed(format!("write failed: {e}")))?;

    crate::applet_templates::reload_and_reconcile(pool)
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("reconcile failed: {e}")))?;

    // The chat it was made in is where its runs post until runs have a home
    // of their own. It lives in the row: it is this box's, not the applet's.
    if let Some(cid) = &chat_id {
        sqlx::query("UPDATE app_applets SET config = config || jsonb_build_object('chat_id', $1::text) WHERE id = $2")
            .bind(cid)
            .bind(&applet_id)
            .execute(pool)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("could not link the chat: {e}")))?;
    }

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
        "folder": rel,
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
        "run_it_with": chat_started.then(|| format!("/{spec}")),
        "skill_md": skill_md,
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

fn str_list(arguments: &serde_json::Value, key: &str) -> Option<Vec<String>> {
    arguments
        .get(key)
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.trim().to_string())).filter(|s| !s.is_empty()).collect())
}

/// The spec name a draft's name becomes: lowercase words joined by hyphens,
/// which is also its folder. "Morning note" is `morning-note`.
fn spec_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').chars().take(64).collect::<String>().trim_end_matches('-').to_string()
}

/// The findings `invoke` can have: its values, a prompt for an applet
/// `/name` starts, and a name no chat mode or shipped applet already has.
fn check_invoke(draft: &Draft) -> Vec<serde_json::Value> {
    let mut findings = Vec::new();
    for i in &draft.invoke {
        if !matches!(i.as_str(), "chat" | "assistant") {
            findings.push(finding("invoke", &format!("invalid invoke '{i}'"), Some("chat, assistant, or both")));
        }
    }
    if draft.invoke.iter().any(|i| i == "chat") {
        if draft.agent.is_none() {
            findings.push(finding("agent", "an applet started with /name needs a prompt to run", None));
        }
        let name = spec_name(&draft.name);
        if crate::api::chat_mode::RESERVED_CHAT_NAMES.contains(&name.as_str()) {
            findings.push(finding("name", &format!("'{name}' is a chat mode's name"), Some("pick another name")));
        } else if crate::applet_templates::chat_applet_named(&name).is_some_and(|a| a.shipped) {
            findings.push(finding("name", &format!("'{name}' is a built-in applet's name"), Some("pick another name")));
        }
    }
    if let Some(m) = draft.model.as_deref() {
        if !matches!(m, "standard" | "deep") {
            findings.push(finding("model", &format!("invalid model '{m}'"), Some("standard or deep")));
        }
    }
    findings
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

/// Check every `data_*` / `wiki_*` table a prompt names against the live
/// catalog, with a did-you-mean for near misses.
///
/// Deliberately narrow. It checks TABLE NAMES, which are tokens, and nothing
/// about the prose around them — a prompt can still ask for something
/// impossible in fluent English. But a typo in a table name is the common
/// case and the one that produces a run that looks fine and does nothing.
async fn check_prompt_tables(pool: &PgPool, prompt: &str) -> Vec<serde_json::Value> {
    let mut named: Vec<String> = Vec::new();
    for token in prompt.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
        if (token.starts_with("data_") || token.starts_with("wiki_"))
            && token.len() > 5
            && !named.iter().any(|n| n == token)
        {
            named.push(token.to_string());
        }
    }
    if named.is_empty() {
        return Vec::new();
    }

    let known: Vec<String> = match sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables \
          WHERE table_schema = 'public' \
            AND (table_name LIKE 'data\\_%' OR table_name LIKE 'wiki\\_%')",
    )
    .fetch_all(pool)
    .await
    {
        Ok(k) => k,
        // If the catalog cannot be read, say nothing rather than invent
        // findings — a check that fires on its own failure is worse than none.
        Err(_) => return Vec::new(),
    };

    named
        .into_iter()
        .filter(|t| !known.contains(t))
        .map(|missing| {
            let suggestion = known
                .iter()
                .map(|k| (levenshtein(&missing, k), k))
                .min_by_key(|(d, _)| *d)
                .filter(|(d, _)| *d <= 3)
                .map(|(_, k)| format!("did you mean `{k}`?"));
            finding(
                "agent",
                &format!("the prompt names `{missing}`, which is not a table on this box"),
                suggestion.as_deref(),
            )
        })
        .collect()
}

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
/// Plan the condition as the reader login, which is what runs it: setup
/// writes it as `run: sql`, so a table the reader cannot see fails here
/// rather than on every run.
async fn explain_bool_expr(pool: &PgPool, expr: &str) -> Result<(), String> {
    let (mut tx, _zone) =
        crate::server::faces::reader_transaction(pool, "5s").await?;
    let sql = format!("EXPLAIN SELECT 1 WHERE ({expr})");
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

pub(crate) fn levenshtein(a: &str, b: &str) -> usize {
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


#[cfg(test)]
mod tests {
    use super::*;

    /// The definition sends the model here before it authors, so the guide
    /// must carry what the definition used to: the fields, the migration
    /// rule, the runtime verbs, the gate and the limits.
    #[test]
    fn the_guide_is_agents_md_without_the_repo_section() {
        let guide = authoring_guide();
        for must in [
            "## The fields",
            "is a migration, not a schema",
            "## What the applet can do at runtime",
            "created **disabled**",
            "## Limits",
            "6-field cron",
            "virtues.query",
        ] {
            assert!(guide.contains(must), "guide lost {must:?}");
        }
        assert!(!guide.contains("For builtin (Rust)"));
        // The cut point must exist verbatim: renamed, the split finds nothing
        // and the repo section leaks into the guide.
        assert!(AGENTS_MD.contains("\n## For builtin (Rust) applet development"));
        assert!(wants_guide(&serde_json::json!({ "guide": true })));
        assert!(!wants_guide(&serde_json::json!({ "name": "x", "description": "y" })));
    }

    /// A typo in a table name is the failure the authoring contract calls out
    /// as unmachine-checkable, and it is a token, so it is checkable after
    /// all. This is the exact example the plan gives.
    #[sqlx::test]
    async fn a_prompt_naming_a_table_that_does_not_exist_is_a_finding(pool: PgPool) {
        let out = check_prompt_tables(
            &pool,
            "Read yesterday from data_helth_sleep and summarise it.",
        )
        .await;
        assert_eq!(out.len(), 1, "the typo must be caught");
        let f = &out[0];
        assert!(f["error"].as_str().unwrap().contains("data_helth_sleep"));
        assert!(
            f["suggestion"].as_str().unwrap().contains("data_health_sleep"),
            "and it must say what was meant: {f:?}"
        );
    }

    /// Real tables, and prose that merely rhymes with one, must stay silent —
    /// a check that cries wolf gets ignored, which is worse than no check.
    #[sqlx::test]
    async fn real_tables_and_ordinary_prose_produce_nothing(pool: PgPool) {
        let out = check_prompt_tables(
            &pool,
            "Look at data_health_heart_rate and data_calendar_event. \
             Mention the data pipeline and wiki pages if useful.",
        )
        .await;
        assert!(out.is_empty(), "unexpected findings: {out:?}");
    }

    /// What setup refuses about `invoke`, `until`, and triggers the format
    /// no longer has.
    #[test]
    fn checks_refuse_what_skill_md_cannot_say() {
        let draft = |extra: serde_json::Value| {
            let mut args = serde_json::json!({
                "name": "Garden log",
                "description": "Says what grew this week",
                "agent": "Read the garden pages and say what grew.",
                "invoke": ["chat"],
            });
            for (k, v) in extra.as_object().unwrap() {
                args[k] = v.clone();
            }
            Draft::parse(&args).unwrap()
        };
        assert_eq!(spec_name("Garden log"), "garden-log");
        assert_eq!(spec_name("  Weekly -- Review! "), "weekly-review");
        assert!(check_invoke(&draft(serde_json::json!({}))).is_empty());
        for (extra, field) in [
            (serde_json::json!({ "invoke": ["everyone"] }), "invoke"),
            (serde_json::json!({ "name": "Chat" }), "name"),
            (serde_json::json!({ "name": "Weekly review" }), "name"),
            (serde_json::json!({ "model": "huge" }), "model"),
        ] {
            let f = check_invoke(&draft(extra.clone()));
            assert!(f.iter().any(|x| x["field"] == field), "{extra}: {f:?}");
        }
    }

    #[sqlx::test]
    async fn until_must_be_once_on_a_schedule_and_api_and_webhook_are_gone(pool: PgPool) {
        let check_of = |extra: serde_json::Value| {
            let mut args = serde_json::json!({
                "name": "Morning note",
                "description": "Writes a short note each morning.",
                "agent": "Write one sentence about the weather.",
            });
            for (k, v) in extra.as_object().unwrap() {
                args[k] = v.clone();
            }
            Draft::parse(&args).unwrap()
        };
        for (extra, field) in [
            (serde_json::json!({ "until": "once" }), "until"),
            (serde_json::json!({ "schedule": "0 0 7 * * *", "until": "select true" }), "until"),
            (serde_json::json!({ "triggers": ["api"] }), "triggers"),
            (serde_json::json!({ "triggers": ["webhook"] }), "triggers"),
        ] {
            let f = check(&pool, &check_of(extra.clone())).await;
            assert!(f.iter().any(|x| x["field"] == field), "{extra}: {f:?}");
        }
        let ok = check(&pool, &check_of(serde_json::json!({ "schedule": "0 0 7 * * *", "until": "once" }))).await;
        assert!(ok.is_empty(), "{ok:?}");
    }
}

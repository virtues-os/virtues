//! The `applet` tool's `setup` action: write an applet's file from the
//! file's own keys.
//!
//! The parameters are the `SKILL.md` keys with the same names (`prompt` is the
//! body, `tools` is `allowed-tools`, `tables` and `view` are files beside it),
//! so what the model sends is what the file says. A setup with a new name
//! writes `<state root>/user/<name>/`; one with an applet's existing name
//! changes only the keys it sends, in that applet's own file, or in a shipped
//! applet's override. The check runs before anything is written, and
//! reconcile then rebuilds the row from the file (agents/plan/
//! applet-shape.md, slice 8).
//!
//! An applet set up here starts on, with no step for the owner to approve
//! (decided 2026-10-06). What keeps an unattended one from running up a bill
//! is its limit: one with a prompt and no daily limit gets
//! [`DEFAULT_DAILY_LIMIT`], shown on its card.

use sqlx::PgPool;

use super::executor::{ToolContext, ToolError, ToolResult};
use crate::applet_templates::SetupTarget;
use crate::scheduler::applets;

const VIEW_MAX: usize = 48 * 1024;
const TABLES_MAX: usize = 16 * 1024;
const PROMPT_MAX: usize = 24 * 1024;

/// `{"check_only": true}` runs the check and creates nothing: the dry run an
/// author iterates against (`virtues applet check`), and the reason it needs no
/// "I allow" in chat.
pub(crate) fn wants_check_only(arguments: &serde_json::Value) -> bool {
    arguments.get("check_only").and_then(|v| v.as_bool()) == Some(true)
}

/// The daily limit an applet with a prompt gets when its file sets none. It
/// starts on and may run with nobody watching; this is what stops it running
/// up a bill overnight. The owner can raise or remove it like any limit.
pub(crate) const DEFAULT_DAILY_LIMIT: &str = "$1 per day";

/// Setup's parameters before 2026-10-28 and what replaced each, so a caller
/// that still sends one is told what to send instead.
const OLD_KEYS: &[(&str, &str)] = &[
    ("agent", "`prompt`"),
    ("instruction", "`prompt`"),
    ("schedule", "`trigger`, such as `every day at 07:00` or `cron 0 0 7 * * *`"),
    ("until", "`trigger`: `on 2026-11-02 at 09:00` for a one-off, or `every … until 2026-12-01`"),
    ("condition", "`run`: `sql select 1 where (…)`"),
    ("triggers", "`invoke`"),
    ("schema_sql", "`tables`, every CREATE TABLE it should have"),
    ("face_html", "`view`"),
    ("limits", "`limit`, such as `$1 per day`"),
    ("guide", "nothing: the tool's description is the whole guide"),
];

/// The keys a setup sends, read once for the check and the write.
pub(crate) struct Draft {
    name: String,
    /// The file's keys this setup sets, by their `SKILL.md` names, with
    /// `None` clearing one. Keys not sent are not here and do not change.
    keys: Vec<(&'static str, Option<String>)>,
    tables: Option<String>,
    view: Option<String>,
}

impl Draft {
    pub(crate) fn parse(arguments: &serde_json::Value) -> Result<Self, ToolError> {
        for (old, new) in OLD_KEYS {
            if arguments.get(*old).is_some() {
                return Err(ToolError::InvalidParameters(format!("`{old}` is gone; use {new}")));
            }
        }
        let name = req_str(arguments, "name")?.to_string();
        let text = |key: &str| -> Option<Option<String>> {
            let v = arguments.get(key)?;
            Some(v.as_str().map(str::trim).filter(|s| !s.is_empty()).map(String::from))
        };
        let list = |key: &str, sep: &str| -> Option<Option<String>> {
            let v = arguments.get(key)?;
            let items: Vec<String> = match v {
                serde_json::Value::Array(a) => a.iter().filter_map(|x| x.as_str()).map(|x| x.trim().to_string()).collect(),
                serde_json::Value::String(s) => s.split([',', ' ']).map(|x| x.trim().to_string()).collect(),
                _ => Vec::new(),
            };
            let items: Vec<String> = items.into_iter().filter(|x| !x.is_empty()).collect();
            Some((!items.is_empty()).then(|| items.join(sep)))
        };
        let mut keys: Vec<(&'static str, Option<String>)> = Vec::new();
        for (param, key) in [("description", "description"), ("prompt", "prompt"), ("trigger", "trigger"), ("run", "run"), ("limit", "limit"), ("model", "model")] {
            if let Some(v) = text(param) {
                keys.push((key, v));
            }
        }
        if let Some(v) = list("invoke", ", ") {
            keys.push(("invoke", v));
        }
        if let Some(v) = list("tools", " ") {
            keys.push(("allowed-tools", v));
        }
        Ok(Self {
            name,
            keys,
            tables: text("tables").flatten(),
            view: text("view").flatten(),
        })
    }

    fn sent(&self, key: &str) -> Option<&Option<String>> {
        self.keys.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
    }
}

/// Everything a setup will write, worked out and checked before anything is.
struct Plan {
    target: SetupTarget,
    /// What the applet's file reads as after this setup (for a shipped one,
    /// with its override laid on top).
    text: String,
    /// The shipped applet's override after this setup, `None` when it is
    /// back to what ships.
    override_text: Option<String>,
    existed: bool,
    slug: String,
    /// The next migration of its tables, when `tables` adds anything.
    migration: Option<String>,
}

/// The check: every finding a setup has, each naming its fix, or the plan
/// that writes it. Runs before anything touches disk, and is the whole of
/// what `check_only` runs.
async fn plan(pool: &PgPool, draft: &Draft) -> Result<Plan, Vec<serde_json::Value>> {
    let mut findings: Vec<serde_json::Value> = Vec::new();
    let spec = spec_name(&draft.name);
    if spec.is_empty() {
        return Err(vec![finding("name", "the name has no letters or digits", Some("a few words, such as `Water log`"))]);
    }
    let slug = spec.replace('-', "_");
    let target = crate::applet_templates::setup_target(&spec);

    // The file before and after.
    let (existed, before, shipped_text) = match &target {
        SetupTarget::Own { folder, .. } => match std::fs::read_to_string(folder.join("SKILL.md")) {
            Ok(t) => (true, t, None),
            Err(_) => {
                let Some(Some(description)) = draft.sent("description") else {
                    return Err(vec![finding("description", "a new applet needs a description", Some("one sentence: what it does for the person, and when to use it"))]);
                };
                let md = crate::applet_templates::SkillMd { name: spec.clone(), description: description.clone(), ..Default::default() };
                (false, crate::applet_templates::render_skill_md(&md).map_err(|e| vec![finding("name", &e, None)])?, None)
            }
        },
        SetupTarget::Shipped { dir, .. } => {
            let (shipped, over) = crate::applet_templates::shipped_and_override(dir).map_err(|e| vec![finding("name", &e.to_string(), None)])?;
            let current = match &over {
                Some(o) => crate::applet_templates::overlay(&shipped, o).map_err(|e| vec![finding("name", &e, None)])?,
                None => shipped.clone(),
            };
            for (key, param) in [("run", "run"), ("allowed-tools", "tools")] {
                if draft.sent(key).is_some() {
                    findings.push(finding(
                        param,
                        &format!("`{spec}` is built in, so its `{param}` cannot change"),
                        Some(&format!("leave `{param}` out to change its prompt or trigger, or give your applet another name")),
                    ));
                }
            }
            if draft.tables.is_some() || draft.view.is_some() {
                findings.push(finding("tables", &format!("`{spec}` is built in, so its tables and view cannot change"), Some("give the applet a new name")));
            }
            (true, current, Some(shipped))
        }
    };
    let text = write_keys(&before, draft, existed, shipped_text.is_some()).map_err(|e| vec![finding("name", &e, None)])?;

    // Each key's own checks.
    let prompt = crate::applet_templates::key_value(&text, "prompt");
    let has_view = draft.view.is_some() || matches!(&target, SetupTarget::Own { folder, .. } if folder.join("view.html").is_file());
    if prompt.is_none() && !has_view && shipped_text.is_none() {
        findings.push(finding("prompt", "an applet needs a `prompt` (something to do) or a `view` (something to show)", None));
    }
    if prompt.as_deref().is_some_and(|p| p.len() > PROMPT_MAX) {
        findings.push(finding("prompt", "the prompt is over 24 KB", Some("say what to read, what to write and how long, in under 300 words")));
    }
    if let Some(p) = &prompt {
        findings.extend(check_prompt_tables(pool, p).await);
    }
    if draft.view.as_deref().is_some_and(|v| v.len() > VIEW_MAX) {
        findings.push(finding("view", "the view is over 48 KB", None));
    }
    if let Some(view) = &draft.view {
        match other_host(view) {
            Some(OtherHost::Loads(host)) => findings.push(finding(
                "view",
                &format!("the view loads {host}, and a view cannot reach any other host"),
                Some("write its styles and scripts in the document, with no import or fetch from another host, and draw charts with inline SVG or canvas"),
            )),
            Some(OtherHost::Links(host)) => findings.push(finding(
                "view",
                &format!("the view links to {host}, and a link out opens inside the view's own frame, where most sites refuse to load"),
                Some("show the address as text, or open a page of the app with `virtues.open('/page/…')`"),
            )),
            None => {}
        }
        if awaits_outside_async(view) {
            findings.push(finding(
                "view",
                "the view's script uses `await` outside an async function, which stops the whole script and leaves the page empty",
                Some("make it `<script type=\"module\">`, where `await` works at the top"),
            ));
        }
        for file in ["virtues.css", "virtues.js"] {
            if view.contains(&format!("\"/{file}\"")) || view.contains(&format!("'/{file}'")) {
                findings.push(finding(
                    "view",
                    &format!("the view links /{file}, which is the box's root and not the view's folder"),
                    Some(&format!("link `{file}`, with no leading slash")),
                ));
            }
        }
    }
    if let Some(Some(run)) = draft.sent("run") {
        match run.strip_prefix("sql ") {
            None => findings.push(finding("run", "setup takes only SQL for `run`", Some("`sql select 1 where (…)`, which returns a row only when it should run"))),
            Some(sql) => {
                // A data trigger's rows are `new_rows`, empty here.
                let new_rows = crate::applet_templates::key_value(&text, "trigger")
                    .and_then(|t| crate::applet_templates::watch_in(&t))
                    .and_then(|watch| crate::scheduler::watch::Batch { watch, range: None }.cte().ok());
                if let Err(e) = explain_run_sql(pool, sql, new_rows.as_deref(), &format!("applet_{slug}")).await {
                    let suggestion = did_you_mean(pool, &e).await;
                    findings.push(finding("run", &e, suggestion.as_deref()));
                }
            }
        }
    }
    // A data trigger's rows are `new_rows`. A `run` that reads the watched
    // table itself sees all of it, so once one row matches it is true on
    // every batch after.
    let watch = crate::applet_templates::key_value(&text, "trigger").and_then(|t| crate::applet_templates::watch_in(&t));
    if let (Some(watch), Some(run)) = (watch, crate::applet_templates::key_value(&text, "run")) {
        if reads_table_not_new_rows(&run, &watch.table) {
            findings.push(finding(
                "run",
                &format!("`run` reads {} itself, so it sees every row and not only the new ones", watch.table),
                Some("read `new_rows`, the rows that woke it: `sql select … from new_rows where …`"),
            ));
        }
    }
    let mut migration = None;
    if let Some(tables) = &draft.tables {
        if tables.len() > TABLES_MAX {
            findings.push(finding("tables", "the tables are over 16 KB", None));
        } else {
            match crate::tools::applet_schema::delta(pool, tables, &slug).await {
                Err(e) => findings.push(finding("tables", &e, None)),
                Ok(None) => {}
                Ok(Some(ddl)) => match crate::tools::applet_schema::check(pool, &ddl, &slug).await {
                    Err(e) => findings.push(finding("tables", &e, None)),
                    Ok(()) => migration = Some(ddl),
                },
            }
        }
    }

    // The file as a whole loads, or the next reconcile would skip it.
    let (folder, dir, shipped) = match &target {
        SetupTarget::Own { folder, rel } => (folder.clone(), rel.clone(), false),
        SetupTarget::Shipped { folder, dir } => (folder.clone(), dir.clone(), true),
    };
    if let Err(problems) = crate::applet_templates::read_skill_md(&text, &folder, &dir, shipped) {
        findings.extend(problems.iter().map(|p| finding(key_named(p), p, None)));
    }
    // The checks above name each key a built-in applet cannot take. The
    // override's own refusal is for a key they miss, so it is the only
    // finding only when they found nothing.
    let override_text = match &shipped_text {
        Some(s) => match crate::applet_templates::override_between(s, &text) {
            Ok(o) => o,
            Err(_) if !findings.is_empty() => None,
            Err(e) => return Err(vec![finding(key_named(&e), &e, None)]),
        },
        None => None,
    };
    if !findings.is_empty() {
        return Err(findings);
    }
    Ok(Plan { target, text, override_text, existed, slug, migration })
}

/// The file after `draft`'s keys, with a new applet's defaults written into
/// it so they are visible: started by the assistant, the background model for
/// an unattended prompt, and a daily limit on any applet with a prompt that
/// has none (a shipped applet keeps its own).
fn write_keys(before: &str, draft: &Draft, existed: bool, shipped: bool) -> Result<String, String> {
    let mut keys = draft.keys.clone();
    if !existed {
        if draft.sent("invoke").is_none() {
            keys.push(("invoke", Some("assistant".into())));
        }
        let prompt = draft.sent("prompt").cloned().flatten();
        let started_from_chat = draft.sent("invoke").cloned().flatten().is_some_and(|i| i.contains("chat"));
        if prompt.is_some() && draft.sent("model").is_none() && !started_from_chat {
            keys.push(("model", Some("lite".into())));
        }
    }
    let text = crate::applet_templates::with_keys(before, &keys)?;
    if shipped || crate::applet_templates::key_value(&text, "prompt").is_none() {
        return Ok(text);
    }
    let limit = crate::applet_templates::key_value(&text, "limit");
    if limit.as_deref().is_some_and(|l| l.split(',').any(|p| p.trim().starts_with('$') && p.contains("per day"))) {
        return Ok(text);
    }
    let with_daily = match limit {
        Some(l) => format!("{l}, {DEFAULT_DAILY_LIMIT}"),
        None => DEFAULT_DAILY_LIMIT.to_string(),
    };
    crate::applet_templates::with_keys(&text, &[("limit", Some(with_daily))])
}

/// Whether `run` names `table` and not `new_rows`, by whole identifiers.
fn reads_table_not_new_rows(run: &str, table: &str) -> bool {
    let lower = run.to_ascii_lowercase();
    let words: Vec<&str> = lower.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).collect();
    words.contains(&table) && !words.contains(&"new_rows")
}

/// The key a reader's sentence is about, by the first one it names, so the
/// finding points at the parameter to fix.
fn key_named(problem: &str) -> &'static str {
    let named = |k: &str| problem.contains(&format!("`{k}")) || problem.contains(&format!("`metadata.{k}"));
    for (key, param) in [("trigger", "trigger"), ("run", "run"), ("invoke", "invoke"), ("limit", "limit"), ("model", "model"), ("allowed-tools", "tools"), ("description", "description"), ("name", "name")] {
        if named(key) {
            return param;
        }
    }
    "prompt"
}

/// Execute the applet tool's setup action.
pub async fn execute(
    pool: &PgPool,
    arguments: serde_json::Value,
    context: &ToolContext,
) -> Result<ToolResult, ToolError> {
    let draft = Draft::parse(&arguments)?;
    let checked = plan(pool, &draft).await;
    if wants_check_only(&arguments) {
        let findings = checked.err().unwrap_or_default();
        return Ok(ToolResult::success(serde_json::json!({
            "status": if findings.is_empty() { "ok" } else { "check_failed" },
            "findings": findings,
        })));
    }
    let plan = match checked {
        Ok(p) => p,
        Err(findings) => {
            return Ok(ToolResult::success(serde_json::json!({
                "status": "check_failed",
                "findings": findings,
                "hint": "fix each finding and call setup again; nothing was written",
            })))
        }
    };

    // ---- Write ------------------------------------------------------------
    // The DDL is not applied here. Reconcile's replay pass is the one apply
    // path, so a folder that arrives any other way (a restored backup) gets
    // the same tables by the same code; the check above ran the migration
    // in a transaction and rolled it back.
    let write = |e: crate::error::Error| ToolError::ExecutionFailed(e.to_string());
    let mut schema_version: Option<i32> = None;
    let (folder_label, shipped) = match &plan.target {
        SetupTarget::Own { rel, folder } => {
            std::fs::create_dir_all(folder).map_err(|e| ToolError::ExecutionFailed(format!("could not create {}: {e}", folder.display())))?;
            if let Some(html) = &draft.view {
                std::fs::write(folder.join("view.html"), html)
                    .map_err(|e| ToolError::ExecutionFailed(format!("could not write the view: {e}")))?;
            }
            if let Some(ddl) = &plan.migration {
                use crate::tools::applet_schema as schema;
                let id = format!("{}{}", applets::USER_APPLET_PREFIX, plan.slug);
                let already = schema::applied(pool, &id).await.unwrap_or_default();
                let on_disk = schema::versions_on_disk(folder).iter().map(|(v, _, _)| *v).max().unwrap_or(0);
                let version = match schema::classify(ddl, &already) {
                    schema::Submission::AlreadyApplied { version } | schema::Submission::NewVersion { version } => version.max(on_disk + 1),
                };
                let tables_dir = folder.join(schema::TABLES_DIR);
                std::fs::create_dir_all(&tables_dir)
                    .map_err(|e| ToolError::ExecutionFailed(format!("could not create tables/: {e}")))?;
                std::fs::write(tables_dir.join(format!("{version:04}_schema.sql")), ddl)
                    .map_err(|e| ToolError::ExecutionFailed(format!("could not write the tables: {e}")))?;
                schema_version = Some(version);
            }
            crate::applet_templates::write_skill_md(folder, &plan.text).map_err(write)?;
            (rel.clone(), false)
        }
        SetupTarget::Shipped { dir, .. } => {
            let _reconcile = crate::applet_templates::reconcile_lock().lock().await;
            crate::applet_templates::set_override(dir, plan.override_text.as_deref()).map_err(write)?;
            (format!("{dir} (your changes to the built-in applet)"), true)
        }
    };
    crate::applet_templates::reload_and_reconcile(pool)
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("reconcile failed: {e}")))?;

    let (folder, dir) = match &plan.target {
        SetupTarget::Own { folder, rel } => (folder.clone(), rel.clone()),
        SetupTarget::Shipped { folder, dir } => (folder.clone(), dir.clone()),
    };
    let (applet_id, schedule, until, chat_started) = crate::applet_templates::read_skill_md(&plan.text, &folder, &dir, shipped)
        .map_err(|p| ToolError::ExecutionFailed(p.join(" ")))?;

    // Reconcile ran the new migration in its replay pass. Confirm it landed
    // rather than trusting that it did: an apply that quietly did nothing
    // must never be reported as success.
    if let Some(version) = schema_version {
        let landed = crate::tools::applet_schema::applied(pool, &applet_id)
            .await
            .unwrap_or_default()
            .iter()
            .any(|a| a.version == version);
        if !landed {
            return Ok(ToolResult::success(serde_json::json!({
                "status": "error",
                "error": format!("the applet was saved but its tables (version {version}) did not apply; the box's log has the error"),
            })));
        }
        let _ = crate::server::faces::ensure_applet_db_grants(pool).await;
    }

    // Setting an applet up again re-arms one that finished. Reconcile never
    // un-archives (that would resurrect one-shots on every boot).
    if plan.existed {
        let _ = applets::unarchive_applet(pool, &applet_id).await;
    }
    // The applet is saved by now, so a failed read back does not fail the
    // setup. It is logged, and the card says on, which is how setup leaves a
    // new applet.
    let made = match applets::get_applet(pool, &applet_id).await {
        Ok(a) => Some(a),
        Err(e) => {
            tracing::error!(applet_id, error = %e, "could not read the applet back after setup");
            None
        }
    };
    let enabled = made.as_ref().map(|a| a.enabled).unwrap_or(true);
    // A prompt that starts on its own is a task the assistant does. It gets
    // its row in the task list, filed in the thread of the chat that made it.
    let task_id = match &made {
        Some(a) => match crate::api::repeating_tasks::ensure(pool, a, context.thread_id.as_deref()).await {
            Ok(id) => id,
            Err(e) => {
                tracing::warn!(applet_id, error = %e, "could not open the task row of a repeating task");
                None
            }
        },
        None => None,
    };

    // ---- What the card shows -----------------------------------------------
    let has_prompt = crate::applet_templates::key_value(&plan.text, "prompt").is_some();
    let has_tables = plan.migration.is_some() || folder.join(crate::tools::applet_schema::TABLES_DIR).is_dir();
    let mut capabilities = vec!["reads your records".to_string()];
    if has_tables {
        capabilities.push(format!("keeps its own tables in applet_{}", plan.slug));
    }
    let has_view = draft.view.is_some() || folder.join("view.html").is_file();
    if has_view {
        capabilities.push("has a view".into());
    }
    // Only an applet with a prompt spends anything. The per-run figure is a
    // slot constant, not a measurement, which is why the card calls it an
    // estimate.
    let est_cost_per_day = schedule.as_deref().map(estimate_runs_per_day).filter(|_| has_prompt).map(|r| r * 0.01);
    let spec = spec_name(&draft.name);

    Ok(ToolResult::success(serde_json::json!({
        "status": if plan.existed { "updated" } else { "created" },
        "applet_id": applet_id,
        "name": draft.name,
        "folder": folder_label,
        "tables_schema": has_tables.then(|| format!("applet_{}", plan.slug)),
        "enabled": enabled,
        "description": crate::applet_templates::key_value(&plan.text, "description"),
        "schedule": schedule,
        "lifecycle": until.as_deref().map(|u| if u.eq_ignore_ascii_case("once") { "once" } else { "until" }).unwrap_or("forever"),
        "capabilities": capabilities,
        "has_view": has_view,
        "task_id": task_id,
        "estimated_cost_per_day": est_cost_per_day,
        "run_it_with": chat_started.then(|| format!("/{spec}")),
        "skill_md": plan.text,
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

/// What a view does with an address on another host.
#[derive(Debug, PartialEq)]
enum OtherHost {
    /// It loads from there: a script, stylesheet, font, image, module import
    /// or `fetch`. The view's CSP allows only the server itself, so it never
    /// arrives. A module that imports from another host does not run at all,
    /// and the page stays empty.
    Loads(String),
    /// It links there with an `<a href>`. Nothing fails to load, but the link
    /// opens inside the view's own frame.
    Links(String),
}

/// The first address in a view that names another host: a load when there is
/// one, since that breaks the page, and otherwise a link.
fn other_host(view: &str) -> Option<OtherHost> {
    let lower = view.to_ascii_lowercase();
    let elsewhere = |rest: &str| rest.starts_with("http://") || rest.starts_with("https://") || rest.starts_with("//");
    // Where the address starts after `lead`, past an `=`, spaces and a quote.
    let address_after = |at: usize, lead: &str, assigns: bool| -> Option<usize> {
        let mut rest = &lower[at + lead.len()..];
        if assigns {
            rest = rest.trim_start().strip_prefix('=')?;
        }
        let rest = rest.trim_start_matches(['"', '\'', '`', ' ', '(']);
        elsewhere(rest).then(|| lower.len() - rest.len())
    };
    let mut load: Option<usize> = None;
    let mut link: Option<usize> = None;
    let note = |slot: &mut Option<usize>, start: usize| *slot = Some(slot.map_or(start, |f| f.min(start)));

    for (at, _) in lower.match_indices("src") {
        if let Some(start) = address_after(at, "src", true) {
            note(&mut load, start);
        }
    }
    for (at, _) in lower.match_indices("href") {
        let Some(start) = address_after(at, "href", true) else { continue };
        // `<a href>` is a link. On any other tag (`<link>`, `<image>`,
        // `<use>`) and in a script (`a.href = …` aside) it is a load.
        let tag = lower[..at].rfind('<').map(|open| &lower[open..at]);
        let anchor = tag.is_some_and(|t| !t.contains('>') && (t.starts_with("<a ") || t.starts_with("<a\n") || t.starts_with("<a\t")));
        note(if anchor { &mut link } else { &mut load }, start);
    }
    for lead in ["url(", "@import ", "import(", "import ", "from ", "fetch("] {
        for (at, _) in lower.match_indices(lead) {
            // `new URL("https://…")` only parses an address.
            if lead == "url(" && lower[..at].trim_end().ends_with("new") {
                continue;
            }
            if let Some(start) = address_after(at, lead, false) {
                note(&mut load, start);
            }
        }
    }

    let address = |start: usize| -> String {
        let end = view[start..]
            .find(|c: char| c == '"' || c == '\'' || c == '`' || c == ')' || c == '>' || c.is_whitespace())
            .map_or(view.len(), |e| start + e);
        view[start..end].chars().take(80).collect()
    };
    match (load, link) {
        (Some(start), _) => Some(OtherHost::Loads(address(start))),
        (None, Some(start)) => Some(OtherHost::Links(address(start))),
        (None, None) => None,
    }
}

/// Whether a view has a plain inline script that awaits with nothing to make
/// it legal. `await` outside a module and outside an async function is a
/// syntax error, so the browser runs none of the script and the page stays
/// empty. Only the certain case counts: a script that is not a module, has
/// no `src`, uses `await` and has no `async` anywhere in it.
fn awaits_outside_async(view: &str) -> bool {
    let lower = view.to_ascii_lowercase();
    let mut at = 0;
    while let Some(open) = lower[at..].find("<script") {
        let tag_start = at + open;
        let Some(tag_len) = lower[tag_start..].find('>') else {
            return false;
        };
        let tag = &lower[tag_start..tag_start + tag_len];
        let body_start = tag_start + tag_len + 1;
        let body_end = lower[body_start..]
            .find("</script")
            .map_or(lower.len(), |e| body_start + e);
        at = body_end;
        // A module may await at the top, and a script with a `src` or with a
        // type that is not JavaScript (a JSON data block) has no code here.
        let other_type = tag.contains("type=") && !tag.contains("javascript");
        if tag.contains("src=") || other_type {
            continue;
        }
        let code = without_js_comments(&lower[body_start..body_end]);
        if has_js_word(&code, "await") && !has_js_word(&code, "async") {
            return true;
        }
    }
    false
}

/// JavaScript with its comments dropped, so a word in a comment is not read
/// as code. `//` after a colon is an address in a string and is kept.
fn without_js_comments(code: &str) -> String {
    let bytes = code.as_bytes();
    let mut out = String::with_capacity(code.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
            i = code[i + 2..].find("*/").map_or(bytes.len(), |e| i + 2 + e + 2);
        } else if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'/') && (i == 0 || bytes[i - 1] != b':') {
            i = code[i..].find('\n').map_or(bytes.len(), |e| i + e);
        } else {
            let ch = code[i..].chars().next().expect("i is inside the string");
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// Whether `word` appears as a word of its own, not inside a longer name.
fn has_js_word(code: &str, word: &str) -> bool {
    let name_char = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '$';
    code.match_indices(word).any(|(at, _)| {
        let before = code[..at].chars().next_back();
        let after = code[at + word.len()..].chars().next();
        !before.is_some_and(name_char) && !after.is_some_and(name_char)
    })
}

fn finding(field: &str, error: &str, suggestion: Option<&str>) -> serde_json::Value {
    serde_json::json!({ "field": field, "error": error, "suggestion": suggestion })
}

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
                "prompt",
                &format!("the prompt names `{missing}`, which is not a table on this box"),
                suggestion.as_deref(),
            )
        })
        .collect()
}

/// Plan `run` SQL as the reader login, with the applet's schema on its path,
/// which is how it runs, so a table the reader cannot see fails here rather
/// than on every run.
async fn explain_run_sql(pool: &PgPool, sql: &str, with: Option<&str>, schema: &str) -> Result<(), String> {
    let (mut tx, _zone) = crate::server::faces::reader_transaction(pool, "5s", Some(schema)).await?;
    let with = with.unwrap_or_default();
    let res = sqlx::query(&format!("EXPLAIN {with}SELECT * FROM ({sql}) q")).fetch_all(&mut *tx).await;
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

    fn draft(args: serde_json::Value) -> Draft {
        Draft::parse(&args).unwrap()
    }

    /// A caller that still sends an old parameter is told the new one.
    #[test]
    fn an_old_parameter_names_its_replacement() {
        for (old, new) in [("agent", "prompt"), ("schedule", "trigger"), ("schema_sql", "tables"), ("face_html", "view"), ("limits", "limit")] {
            let args = serde_json::json!({ "name": "Water log", old: "x" });
            let Err(ToolError::InvalidParameters(e)) = Draft::parse(&args) else { panic!("{old} was accepted") };
            assert!(e.contains(new), "{old}: {e}");
        }
    }

    /// The keys go into the file by the file's own names, and a new applet's
    /// defaults are written where the owner can see them.
    #[test]
    fn a_new_applet_is_its_keys_and_visible_defaults() {
        let d = draft(serde_json::json!({
            "name": "Water log",
            "description": "Counts the glasses of water you drink. Use when someone says they drank water.",
            "prompt": "Log what they drank in applet_water_log.drinks.",
            "trigger": "every day at 21:00",
            "tools": ["sql_query", "sql_write"],
        }));
        let before = crate::applet_templates::render_skill_md(&crate::applet_templates::SkillMd {
            name: "water-log".into(),
            description: "Counts the glasses of water you drink. Use when someone says they drank water.".into(),
            ..Default::default()
        })
        .unwrap();
        let text = write_keys(&before, &d, false, false).unwrap();
        let read = |k: &str| crate::applet_templates::key_value(&text, k);
        assert_eq!(read("trigger").as_deref(), Some("every day at 21:00"));
        assert_eq!(read("allowed-tools").as_deref(), Some("sql_query sql_write"));
        assert_eq!(read("invoke").as_deref(), Some("assistant"));
        assert_eq!(read("model").as_deref(), Some("lite"));
        assert_eq!(read("limit").as_deref(), Some(DEFAULT_DAILY_LIMIT));
        assert_eq!(read("prompt").as_deref(), Some("Log what they drank in applet_water_log.drinks."));

        // An update changes only what it sends, and a limit with a daily cap
        // keeps it.
        let d = draft(serde_json::json!({ "name": "Water log", "limit": "$0.25 per run, $2 per day" }));
        let again = write_keys(&text, &d, true, false).unwrap();
        assert_eq!(crate::applet_templates::key_value(&again, "limit").as_deref(), Some("$0.25 per run, $2 per day"));
        assert_eq!(crate::applet_templates::key_value(&again, "trigger").as_deref(), Some("every day at 21:00"));
        let d = draft(serde_json::json!({ "name": "Water log", "limit": "$0.25 per run" }));
        let capped = write_keys(&text, &d, true, false).unwrap();
        assert_eq!(crate::applet_templates::key_value(&capped, "limit").as_deref(), Some("$0.25 per run, $1 per day"));
    }

    #[test]
    fn a_reader_sentence_points_at_its_parameter() {
        assert_eq!(key_named("`trigger` \"every fortnight\" is not a time."), "trigger");
        assert_eq!(key_named("`allowed-tools` lists `nope`, which is not a tool."), "tools");
        assert_eq!(key_named("Something else entirely."), "prompt");
    }

    /// `run` SQL that reads `new_rows` plans when the trigger has a data
    /// trigger to fill it, and is a finding when it has none.
    #[sqlx::test]
    async fn run_sql_reads_new_rows_only_with_a_data_trigger(pool: PgPool) {
        crate::server::faces::ensure_applet_db_grants(&pool).await.unwrap();
        let sql = "select title from new_rows where title ilike '%review%'";
        let watch = crate::applet_templates::watch_in("every day at 07:00, new data_calendar_event").expect("a data trigger");
        let cte = crate::scheduler::watch::Batch { watch, range: None }.cte().unwrap();
        assert_eq!(explain_run_sql(&pool, sql, Some(&cte), "applet_reviews").await, Ok(()));
        let e = explain_run_sql(&pool, sql, None, "applet_reviews").await.unwrap_err();
        assert!(e.contains("new_rows"), "{e}");
    }

    #[test]
    fn a_run_that_reads_the_watched_table_instead_of_new_rows() {
        let t = "data_health_heart_rate";
        assert!(reads_table_not_new_rows("sql SELECT bpm FROM data_health_heart_rate WHERE bpm > 120", t));
        assert!(reads_table_not_new_rows("sql select 1 from public.data_health_heart_rate", t));
        assert!(!reads_table_not_new_rows("sql select bpm from new_rows where bpm > 120", t));
        // Comparing the new rows with the table's history is fine.
        assert!(!reads_table_not_new_rows(
            "sql select n.bpm from new_rows n where n.bpm > (select avg(bpm) * 1.5 from data_health_heart_rate)",
            t
        ));
        assert!(!reads_table_not_new_rows("sql select 1 from data_health_heart_rate_daily", t));
    }

    /// Setup says so when a data-triggered `run` reads the watched table.
    #[sqlx::test]
    async fn setup_finds_a_run_that_skips_new_rows(pool: PgPool) {
        crate::server::faces::ensure_applet_db_grants(&pool).await.unwrap();
        let draft = |run: &str| {
            Draft::parse(&serde_json::json!({
                "name": "Heart alert", "description": "Tells you when your heart rate runs high.",
                "trigger": "new data_health_heart_rate", "run": run, "prompt": "Say when it was high.",
            }))
            .unwrap()
        };
        let found = |f: &Vec<serde_json::Value>| f.iter().any(|f| f["field"] == "run" && f["error"].as_str().unwrap().contains("itself"));
        let bad = plan(&pool, &draft("sql select bpm from data_health_heart_rate where bpm > 120")).await.err().expect("refused");
        assert!(found(&bad), "{bad:?}");
        let good = plan(&pool, &draft("sql select bpm from new_rows where bpm > 120")).await.err().unwrap_or_default();
        assert!(!found(&good), "{good:?}");
    }

    /// A built-in applet's name with `tools` is refused for `tools`, by the
    /// parameter's name, with what to do instead, and nothing else.
    #[sqlx::test]
    async fn a_built_in_applets_tools_are_refused_by_their_parameter(pool: PgPool) {
        let draft = Draft::parse(&serde_json::json!({
            "name": "Weekly review", "prompt": "Review my week.", "trigger": "every sunday at 19:00", "tools": ["sql_query"],
        }))
        .unwrap();
        let findings = plan(&pool, &draft).await.err().expect("refused");
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0]["field"], "tools");
        assert!(findings[0]["suggestion"].as_str().unwrap().contains("another name"), "{findings:?}");
    }

    /// A view that links its runtime from the box's root is a finding.
    #[sqlx::test]
    async fn a_view_links_its_runtime_without_a_leading_slash(pool: PgPool) {
        let draft = Draft::parse(&serde_json::json!({
            "name": "Water log", "description": "Counts your water.",
            "view": "<link rel=\"stylesheet\" href=\"/virtues.css\"><script src=\"virtues.js\"></script>",
        }))
        .unwrap();
        let findings = plan(&pool, &draft).await.err().expect("refused");
        assert!(findings.iter().any(|f| f["field"] == "view" && f["error"].as_str().unwrap().contains("/virtues.css")), "{findings:?}");
        assert!(!findings.iter().any(|f| f["error"].as_str().unwrap().contains("/virtues.js")), "{findings:?}");
    }

    /// A view that loads from another host is a finding that names the host.
    #[test]
    fn a_view_that_names_another_host_is_found() {
        let loads = |a: &str| Some(OtherHost::Loads(a.to_string()));
        let cdn = "<link rel=\"stylesheet\" href=\"virtues.css\"><script src=\"https://cdn.example.com/chart.js\"></script>";
        assert_eq!(other_host(cdn), loads("https://cdn.example.com/chart.js"));
        assert_eq!(other_host("<style>@import url(//fonts.example.com/a.css);</style>"), loads("//fonts.example.com/a.css"));
        assert_eq!(other_host("<link rel=\"stylesheet\" href=\"https://fonts.example.com/a.css\">"), loads("https://fonts.example.com/a.css"));
        let own = "<link rel=\"stylesheet\" href=\"virtues.css\"><script src=\"virtues.js\"></script><p>See https://example.com in the text.</p>";
        assert_eq!(other_host(own), None);
    }

    /// A module that imports from another host never runs, so the page stays
    /// empty: that is a load, as a `fetch` and a script-set `src` are. A plain
    /// link is told apart, and an address that is only parsed is left alone.
    #[test]
    fn a_view_that_imports_fetches_or_links_elsewhere_is_found() {
        let loads = |a: &str| Some(OtherHost::Loads(a.to_string()));
        let module = "<script type=\"module\">import { chart } from 'https://cdn.example.com/chart.mjs';</script>";
        assert_eq!(other_host(module), loads("https://cdn.example.com/chart.mjs"));
        let dynamic = "<script type=module>const m = await import(\"https://cdn.example.com/m.js\");</script>";
        assert_eq!(other_host(dynamic), loads("https://cdn.example.com/m.js"));
        let bare = "<script type=module>import 'https://cdn.example.com/side.js';</script>";
        assert_eq!(other_host(bare), loads("https://cdn.example.com/side.js"));
        let fetched = "<script>fetch('https://api.example.com/weather').then(draw);</script>";
        assert_eq!(other_host(fetched), loads("https://api.example.com/weather"));
        let set = "<script>img.src = 'https://img.example.com/a.png';</script>";
        assert_eq!(other_host(set), loads("https://img.example.com/a.png"));

        let link = "<p>From <a href=\"https://example.com/guide\">the guide</a>.</p>";
        assert_eq!(other_host(link), Some(OtherHost::Links("https://example.com/guide".to_string())));
        // A load is the worse of the two, so it is the one reported.
        let both = format!("{link}{module}");
        assert_eq!(other_host(&both), loads("https://cdn.example.com/chart.mjs"));

        let parsed = "<script>const u = new URL(\"https://example.com/x\"); show(u.pathname);</script>";
        assert_eq!(other_host(parsed), None);
        let own = "<script type=module>import { rows } from './rows.js'; const r = await virtues.query('select 1');</script>";
        assert_eq!(other_host(own), None);
        let prose = "<p>Data from the weather service, import or fetch as you like.</p>";
        assert_eq!(other_host(prose), None);
    }

    /// A plain script that awaits with no async function never runs, so the
    /// view stays empty. A module, an async function, a comment and a longer
    /// word are all left alone.
    #[test]
    fn a_view_whose_script_awaits_outside_async_is_found() {
        let head = "<script src=\"virtues.js\"></script>";
        let plain = format!("{head}<script>const rows = await virtues.query('select 1');</script>");
        assert!(awaits_outside_async(&plain));
        let upper = format!("{head}<SCRIPT>\n  const rows = await virtues.query('select 1');\n</SCRIPT>");
        assert!(awaits_outside_async(&upper));

        let module = format!("{head}<script type=\"module\">const rows = await virtues.query('select 1');</script>");
        assert!(!awaits_outside_async(&module));
        let unquoted = format!("{head}<script type=module>const rows = await virtues.query('select 1');</script>");
        assert!(!awaits_outside_async(&unquoted));
        let wrapped = format!("{head}<script>(async () => {{ const rows = await virtues.query('select 1'); }})();</script>");
        assert!(!awaits_outside_async(&wrapped));
        let comment = format!("{head}<script>// await the rows later\n/* await */ draw();</script>");
        assert!(!awaits_outside_async(&comment));
        let longer = format!("{head}<script>const awaiting = 1; show(awaiting);</script>");
        assert!(!awaits_outside_async(&longer));
        let data = format!("{head}<script type=\"application/json\">{{\"note\": \"await this\"}}</script>");
        assert!(!awaits_outside_async(&data));
        assert!(!awaits_outside_async("<p>No script at all, and an open <script"));
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
}

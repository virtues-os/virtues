//! The data verbs: `virtues query`, `search`, `schema`, `applet`, `page`,
//! and `applet check`, the dry run of the check chat runs before it creates.
//!
//! Each verb is a registry tool run through `ToolExecutor`, never SQL of its
//! own. The CLI connects as the `virtues` service user, a database superuser;
//! the executor is what drops to `virtues_face_reader` before a query runs, so
//! a verb that went around it could read `box_secrets` and the env file. The
//! plan is agents/plan/cli-data-verbs-plan.md.
//!
//! Output follows the `gh` convention: a table on a terminal, tab-separated
//! lines with no header when piped, the tool's own JSON with `--json`. Data
//! goes to stdout; notes and errors go to stderr.

use console::style;
use serde_json::{json, Value};
use sqlx::PgPool;

use super::types::{AppletCmd, OutputArgs, PageCmd};
use super::ui;
use crate::tools::{ToolContext, ToolExecutor, CLI_TOOLS};

/// The widest a cell may print on a terminal. Piped output is never cut.
const MAX_CELL: usize = 60;

pub struct Verbs {
    exec: ToolExecutor,
    pool: PgPool,
}

impl Verbs {
    pub fn new(pool: PgPool) -> Self {
        Self { exec: ToolExecutor::without_warmup(pool.clone()), pool }
    }

    /// Run one allowlisted tool and return its data, or its error as text.
    async fn call(&self, tool: &str, args: Value) -> Result<Value, String> {
        if !CLI_TOOLS.contains(&tool) {
            return Err(format!("{tool} is not available to the CLI"));
        }
        let result = self
            .exec
            .execute(tool, args, &ToolContext::default())
            .await
            .map_err(|e| e.to_string())?;
        if !result.success {
            return Err(match result.error {
                Some(message) => message,
                None => format!("{tool} failed"),
            });
        }
        Ok(result.data)
    }

    pub async fn query(&self, sql: String, limit: u32, out: OutputArgs) -> Result<(), String> {
        let sql = if sql == "-" {
            std::io::read_to_string(std::io::stdin()).map_err(|e| format!("reading stdin: {e}"))?
        } else {
            sql
        };
        let data = self
            .call("sql_query", json!({ "operation": "query", "sql": sql, "limit": limit }))
            .await?;
        if out.json {
            return print_json(&data);
        }
        let rows = array(&data, "rows");
        // The SELECT's order first, then what the tool adds (a row's `ref`).
        let mut cols = strings(&data, "columns");
        if let Some(first) = rows.first().and_then(Value::as_object) {
            for k in first.keys() {
                if !cols.contains(k) {
                    cols.push(k.clone());
                }
            }
        }
        let cols: Vec<&str> = cols.iter().map(String::as_str).collect();
        table(&cols, rows);
        if rows.is_empty() {
            note("no rows");
        }
        if let Some(t) = data.get("truncated") {
            note(&format!(
                "showing {} of {} rows: the result was too large. Aggregate, filter, or select fewer columns.",
                t["shown"], t["returned"]
            ));
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn search(
        &self,
        text: Vec<String>,
        entities: Vec<String>,
        domains: Vec<String>,
        after: Option<String>,
        before: Option<String>,
        limit: u32,
        out: OutputArgs,
    ) -> Result<(), String> {
        let mut args = json!({ "query": text.join(" "), "num_results": limit });
        if !entities.is_empty() {
            args["entities"] = json!(entities);
        }
        if !domains.is_empty() {
            args["domains"] = json!(domains);
        }
        if let Some(a) = after {
            args["date_after"] = json!(a);
        }
        if let Some(b) = before {
            args["date_before"] = json!(b);
        }
        let data = self.call("semantic_search", args).await?;
        if out.json {
            return print_json(&data);
        }
        let wiki = array(&data, "from_your_wiki");
        if !wiki.is_empty() {
            heading("From your wiki");
            table(&["kind", "id", "name", "date"], wiki);
            heading("Records");
        }
        let results = array(&data, "results");
        table(&["rank", "timestamp", "ontology", "title", "record_id"], results);
        if results.is_empty() && wiki.is_empty() {
            note("nothing found");
        }
        Ok(())
    }

    pub async fn schema(&self, tables: Vec<String>, out: OutputArgs) -> Result<(), String> {
        if tables.is_empty() {
            let data = self.call("sql_query", json!({ "operation": "list_tables" })).await?;
            if out.json {
                return print_json(&data);
            }
            table(&["name", "row_count", "description"], array(&data, "tables"));
            return Ok(());
        }
        let data = self
            .call("sql_query", json!({ "operation": "get_schema", "tables": tables }))
            .await?;
        if out.json {
            return print_json(&data);
        }
        for name in &tables {
            let t = &data["tables"][name];
            if ui::tty() {
                heading(name);
                for key in ["description", "row_count", "key_columns", "join_hint"] {
                    let v = cell(&t[key]);
                    if !v.is_empty() {
                        ui::kv(key, &v);
                    }
                }
                println!();
                table(&["name", "data_type", "is_nullable"], array(t, "columns"));
            } else {
                // One line per column, prefixed with its table, so a pipe can
                // grep across several tables at once.
                for c in array(t, "columns") {
                    println!(
                        "{name}\t{}\t{}\t{}",
                        cell(&c["name"]),
                        cell(&c["data_type"]),
                        cell(&c["is_nullable"])
                    );
                }
            }
        }
        Ok(())
    }

    pub async fn applet(&self, cmd: AppletCmd) -> Result<(), String> {
        match cmd {
            AppletCmd::Ls { all, out } => {
                let data = self.call("list_applets", json!({ "include_archived": all })).await?;
                if out.json {
                    return print_json(&data);
                }
                // Flatten the nested last run into the two cells worth a column.
                let rows: Vec<Value> = array(&data, "actions")
                    .iter()
                    .map(|a| {
                        let mut row = a.clone();
                        row["last_run"] = a["last_run"]["status"].clone();
                        row["last_run_at"] = a["last_run"]["started_at"].clone();
                        row
                    })
                    .collect();
                table(&["id", "name", "owner", "enabled", "schedule", "last_run", "last_run_at"], &rows);
                Ok(())
            }
            AppletCmd::Get { id, out } => {
                let data = self.call("get_applet", json!({ "id": id })).await?;
                if out.json {
                    return print_json(&data);
                }
                let a = &data["action"];
                for key in [
                    "id", "name", "owner", "enabled", "description", "schedule", "triggers",
                    "condition", "until", "archived_at",
                ] {
                    let v = cell(&a[key]);
                    if !v.is_empty() {
                        if ui::tty() {
                            ui::kv(key, &v);
                        } else {
                            println!("{key}\t{v}");
                        }
                    }
                }
                let runs = array(&data, "recent_runs");
                if !runs.is_empty() {
                    heading("Recent runs");
                    table(&["status", "trigger", "started_at", "completed_at", "result_summary", "error"], runs);
                }
                Ok(())
            }
            AppletCmd::Check { path, out } => {
                let (mut args, mut findings) = if path == "-" {
                    let text = std::io::read_to_string(std::io::stdin())
                        .map_err(|e| format!("reading stdin: {e}"))?;
                    let args: Value = serde_json::from_str(&text)
                        .map_err(|e| format!("stdin is not JSON: {e}"))?;
                    (args, Vec::new())
                } else {
                    folder_args(&self.pool, std::path::Path::new(&path)).await?
                };
                args["check_only"] = json!(true);
                let data = self.call("setup_applet", args).await?;
                findings.extend(array(&data, "findings").iter().cloned());
                if out.json {
                    print_json(&json!({
                        "status": if findings.is_empty() { "ok" } else { "check_failed" },
                        "findings": findings,
                    }))?;
                } else if findings.is_empty() {
                    if ui::tty() {
                        ui::ok("no findings");
                    }
                } else {
                    table(&["field", "error", "suggestion"], &findings);
                }
                if findings.is_empty() {
                    Ok(())
                } else {
                    Err(format!("{} finding(s)", findings.len()))
                }
            }
        }
    }

    pub async fn page(&self, cmd: PageCmd) -> Result<(), String> {
        match cmd {
            PageCmd::Get { id, out } => {
                let data = self.call("get_page_content", json!({ "page_id": id })).await?;
                if out.json {
                    return print_json(&data);
                }
                if ui::tty() {
                    heading(&cell(&data["title"]));
                    println!();
                }
                println!("{}", cell(&data["content"]));
                Ok(())
            }
        }
    }
}

/// The manifest keys an authored applet can set: what `setup_applet` writes.
/// Anything else in a folder's manifest (`command`, `credential`, …) belongs
/// to shipped or trusted applets, and would not survive being put.
const AUTHORED_KEYS: &[&str] = &[
    "name", "description", "owner", "agent", "instruction", "schedule", "triggers",
    "condition", "until", "default_enabled", "enabled", "config",
];

/// An applet folder as `setup_applet` arguments, plus the findings only the
/// folder can have (an unreadable or unauthorable manifest).
///
/// Schema: the versions on disk this box has not applied yet, joined in
/// order, so the dry-run sees an `ALTER` after the `CREATE` it depends on.
/// None pending means none submitted, which is what a re-put would send.
async fn folder_args(pool: &PgPool, dir: &std::path::Path) -> Result<(Value, Vec<Value>), String> {
    let manifest_path = dir.join("manifest.toml");
    let text = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let manifest: toml::Table = text
        .parse()
        .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let mut args = serde_json::to_value(&manifest).map_err(|e| e.to_string())?;
    let mut findings = Vec::new();

    for key in manifest.keys() {
        if !AUTHORED_KEYS.contains(&key.as_str()) {
            findings.push(json!({
                "field": key,
                "error": "not a field an authored applet can set",
                "suggestion": "agent, schedule, triggers, condition, until, config.limits",
            }));
        }
    }
    if let Some(limits) = manifest.get("config").and_then(|c| c.get("limits")) {
        args["limits"] = serde_json::to_value(limits).map_err(|e| e.to_string())?;
    }

    let face = dir.join("face").join("index.html");
    if face.is_file() {
        let html = std::fs::read_to_string(&face).map_err(|e| format!("{}: {e}", face.display()))?;
        args["face_html"] = json!(html);
    }

    let versions = crate::tools::applet_schema::versions_on_disk(dir);
    if !versions.is_empty() {
        let name = manifest.get("name").and_then(|v| v.as_str()).unwrap_or_default();
        let applet_id = format!(
            "{}{}",
            crate::scheduler::applets::USER_APPLET_PREFIX,
            crate::tools::applet_setup::slugify(name)
        );
        let latest = crate::tools::applet_schema::applied(pool, &applet_id)
            .await?
            .iter()
            .map(|a| a.version)
            .max()
            .unwrap_or(0);
        let pending: Vec<&str> =
            versions.iter().filter(|(v, _, _)| *v > latest).map(|(_, _, sql)| sql.as_str()).collect();
        if !pending.is_empty() {
            args["schema_sql"] = json!(pending.join("\n\n"));
        }
    }
    Ok((args, findings))
}

fn array<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

fn strings(v: &Value, key: &str) -> Vec<String> {
    array(v, key).iter().filter_map(|s| s.as_str().map(str::to_string)).collect()
}

/// A JSON value as one cell of text: strings bare, nulls empty, string lists
/// joined, anything else as compact JSON.
fn cell(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Array(items) if items.iter().all(Value::is_string) => items
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", "),
        other => other.to_string(),
    }
}

/// One line, however many the value had: a cell never breaks a row.
fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max - 1).collect();
    out.push('…');
    out
}

/// Rows as an aligned table on a terminal, or tab-separated lines when piped.
fn table(cols: &[&str], rows: &[Value]) {
    let cells: Vec<Vec<String>> = rows
        .iter()
        .map(|r| cols.iter().map(|c| one_line(&cell(&r[*c]))).collect())
        .collect();

    if !ui::tty() {
        for row in &cells {
            println!("{}", row.join("\t"));
        }
        return;
    }
    if cells.is_empty() {
        return;
    }
    let cells: Vec<Vec<String>> =
        cells.into_iter().map(|r| r.into_iter().map(|c| clip(&c, MAX_CELL)).collect()).collect();
    let widths: Vec<usize> = cols
        .iter()
        .enumerate()
        .map(|(i, c)| {
            cells.iter().map(|r| r[i].chars().count()).max().unwrap_or(0).max(c.chars().count())
        })
        .collect();
    let pad = |s: &str, w: usize| format!("{s}{}", " ".repeat(w - s.chars().count()));

    let header: Vec<String> = cols.iter().zip(&widths).map(|(c, w)| pad(c, *w)).collect();
    println!("  {}", style(header.join("  ").trim_end()).dim());
    for row in &cells {
        let line: Vec<String> = row.iter().zip(&widths).map(|(c, w)| pad(c, *w)).collect();
        println!("  {}", line.join("  ").trim_end());
    }
}

/// A section title on a terminal; nothing when piped, so pipes carry only data.
fn heading(title: &str) {
    if ui::tty() {
        ui::section(title);
        println!();
    }
}

/// A note about the output rather than the output itself.
fn note(msg: &str) {
    if ui::tty() {
        eprintln!("  {}  {}", style("·").dim(), style(msg).dim());
    } else {
        eprintln!("{msg}");
    }
}

fn print_json(data: &Value) -> Result<(), String> {
    let text = if ui::tty() { serde_json::to_string_pretty(data) } else { serde_json::to_string(data) };
    println!("{}", text.map_err(|e| e.to_string())?);
    Ok(())
}

//! The data verbs: `virtues query`, `search`, `schema`, `applet`, `page`,
//! and `applet check`, the dry run of the check chat runs before it creates.
//!
//! Each verb is a registry tool run through `ToolExecutor`, never SQL of its
//! own. The CLI connects as the `virtues` service user, a database superuser;
//! the executor is what drops to `virtues_face_reader` before a query runs, so
//! a verb that went around it could read `box_secrets` and the env file. The
//! plan is agents/plan/cli-data-verbs-plan.md.
//!
//! Reads run their tool in this process. Writes run theirs in the server,
//! through `POST /api/console/tool/:tool` over loopback (`server/api/console.rs`
//! says why), so a write needs the virtues server running on this machine.
//!
//! Output follows the `gh` convention: a table on a terminal, tab-separated
//! lines with no header when piped, the tool's own JSON with `--json`. Data
//! goes to stdout; notes and errors go to stderr.

use console::style;
use serde_json::{json, Value};
use sqlx::PgPool;

use super::types::{AppletCmd, OutputArgs, PageCmd};
use super::ui;
use super::types::Commands;
use crate::server::api::console::AGENT_KEY_HEADER;
use crate::tools::{ToolContext, ToolExecutor, CLI_TOOLS};

/// The widest a cell may print on a terminal. Piped output is never cut.
const MAX_CELL: usize = 60;

/// The daily spend limit an applet put from the CLI gets when it has a prompt
/// and sets none, in dollars. A scheduled applet written by an outside agent
/// runs with nobody watching; this is what stops it running up a bill
/// overnight. The owner can raise or remove it like any limit.
const DEFAULT_MAX_LLM_COST_PER_DAY: f64 = 1.0;

/// A tool's data says it failed even when the call itself succeeded:
/// `sql_write` returns `status: "error"`, `edit_applet` `"refused"`, the
/// check `"check_failed"`.
fn failed_status(data: &Value) -> Option<String> {
    let status = data.get("status").and_then(Value::as_str)?;
    if !matches!(status, "error" | "refused") {
        return None;
    }
    let error = cell(&data["error"]);
    let error = if error.is_empty() { status.to_string() } else { error };
    // The tool's hint names the fix (`sql_write`: "applet_* schemas only").
    Some(match data.get("hint").and_then(Value::as_str) {
        Some(hint) => format!("{error}\n{hint}"),
        None => error,
    })
}

/// The server on this machine: every write, and every read from an agent key.
struct Console {
    http: reqwest::Client,
    base: String,
    /// The agent key this call came in on, for the server's audit line.
    key: Option<String>,
}

impl Console {
    fn new(key: Option<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            base: format!("http://127.0.0.1:{}", super::types::default_port()),
            key,
        }
    }

    async fn send(&self, req: reqwest::RequestBuilder) -> Result<Value, String> {
        let req = match &self.key {
            Some(key) => req.header(AGENT_KEY_HEADER, key),
            None => req,
        };
        let resp = req.send().await.map_err(|e| {
            if e.is_connect() {
                format!(
                    "couldn't reach the virtues server at {}. Writes go through it; check that it is running (systemctl status virtues)",
                    self.base
                )
            } else {
                e.to_string()
            }
        })?;
        let status = resp.status();
        let body: Value = resp.json().await.map_err(|e| format!("the server answered {status} with no JSON: {e}"))?;
        if !status.is_success() {
            let msg = body.get("error").map(cell).filter(|m| !m.is_empty());
            return Err(msg.unwrap_or_else(|| format!("the server answered {status}")));
        }
        Ok(body)
    }

    /// Run a write tool in the server. Returns the tool's data.
    async fn tool(&self, tool: &str, args: Value) -> Result<Value, String> {
        let url = format!("{}/api/console/tool/{tool}", self.base);
        let body = self.send(self.http.post(url).json(&args)).await?;
        let data = body.get("data").cloned().unwrap_or(Value::Null);
        if let Some(err) = failed_status(&data) {
            return Err(err);
        }
        Ok(data)
    }

    /// Turn an applet on or off: the app's own Enable switch.
    async fn set_enabled(&self, id: &str, on: bool) -> Result<Value, String> {
        let url = format!("{}/api/applets/{id}", self.base);
        self.send(self.http.patch(url).json(&json!({ "enabled": on }))).await
    }
}

pub struct Verbs {
    /// Set when this process can open the database (the owner at the box, or
    /// a dev checkout): reads run here and need no server.
    local: Option<(ToolExecutor, PgPool)>,
    console: Console,
}

impl Verbs {
    pub fn new(pool: PgPool) -> Self {
        Self { local: Some((ToolExecutor::without_warmup(pool.clone()), pool)), console: Console::new(None) }
    }

    /// For an agent key: no database here, so every verb goes to the server.
    pub fn remote(key: String) -> Self {
        Self { local: None, console: Console::new(Some(key)) }
    }

    /// Run one allowlisted read tool and return its data, or its error as text.
    async fn call(&self, tool: &str, args: Value) -> Result<Value, String> {
        if !CLI_TOOLS.contains(&tool) {
            return Err(format!("{tool} is not available to the CLI"));
        }
        let Some((exec, _)) = &self.local else {
            return self.console.tool(tool, args).await;
        };
        let result = exec.execute(tool, args, &ToolContext::default()).await.map_err(|e| e.to_string())?;
        if !result.success {
            return Err(match result.error {
                Some(message) => message,
                None => format!("{tool} failed"),
            });
        }
        if let Some(err) = failed_status(&result.data) {
            return Err(err);
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

    pub async fn write(&self, sql: String, out: OutputArgs) -> Result<(), String> {
        let sql = if sql == "-" {
            std::io::read_to_string(std::io::stdin()).map_err(|e| format!("reading stdin: {e}"))?
        } else {
            sql
        };
        let data = self.console.tool("sql_write", json!({ "sql": sql })).await?;
        if out.json {
            return print_json(&data);
        }
        // A statement with RETURNING hands back rows; anything else a count.
        let rows = array(&data, "rows");
        if !rows.is_empty() {
            let cols: Vec<String> = rows[0].as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default();
            let cols: Vec<&str> = cols.iter().map(String::as_str).collect();
            table(&cols, rows);
        } else if ui::tty() {
            ui::ok(&format!("{} row(s) changed", cell(&data["rows_affected"])));
        } else {
            println!("{}", cell(&data["rows_affected"]));
        }
        Ok(())
    }

    async fn switch(&self, id: &str, on: bool) -> Result<(), String> {
        self.console.set_enabled(id, on).await?;
        if ui::tty() {
            ui::ok(&format!("{id} is {}", if on { "on" } else { "off" }));
        }
        Ok(())
    }

    /// `setup_applet` arguments from a folder, or from JSON on stdin (`-`).
    async fn draft_args(&self, path: &str) -> Result<(Value, Vec<Value>), String> {
        if path == "-" {
            let text = std::io::read_to_string(std::io::stdin()).map_err(|e| format!("reading stdin: {e}"))?;
            let args: Value = serde_json::from_str(&text).map_err(|e| format!("stdin is not JSON: {e}"))?;
            return Ok((args, Vec::new()));
        }
        let Some((_, pool)) = &self.local else {
            return Err("an applet folder is read on this machine; send its JSON on stdin with `-`".into());
        };
        folder_args(pool, std::path::Path::new(path)).await
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
            AppletCmd::Put { path, off, out } => {
                let (mut args, findings) = self.draft_args(&path).await?;
                if !findings.is_empty() {
                    // The write would drop these silently; say so instead.
                    table(&["field", "error", "suggestion"], &findings);
                    return Err(format!("{} finding(s); nothing was created", findings.len()));
                }
                let has_prompt = args.get("agent").or_else(|| args.get("instruction")).is_some();
                if has_prompt && args["limits"].get("max_llm_cost_per_day").is_none() {
                    if !args["limits"].is_object() {
                        args["limits"] = json!({});
                    }
                    args["limits"]["max_llm_cost_per_day"] = json!(DEFAULT_MAX_LLM_COST_PER_DAY);
                }
                let console = &self.console;
                let data = console.tool("setup_applet", args).await?;
                if data["status"] == "check_failed" {
                    let findings = array(&data, "findings");
                    table(&["field", "error", "suggestion"], findings);
                    return Err(format!("{} finding(s); nothing was created", findings.len()));
                }
                let id = cell(&data["applet_id"]);
                let mut enabled = data["enabled"].as_bool() == Some(true);
                if !enabled && !off {
                    console.set_enabled(&id, true).await?;
                    enabled = true;
                }
                if out.json {
                    let mut data = data;
                    data["enabled"] = json!(enabled);
                    return print_json(&data);
                }
                let state = if enabled { "on" } else { "off" };
                if ui::tty() {
                    ui::ok(&format!("{} {id} ({state})", cell(&data["status"])));
                } else {
                    println!("{id}\t{state}");
                }
                Ok(())
            }
            AppletCmd::On { id } => self.switch(&id, true).await,
            AppletCmd::Off { id } => self.switch(&id, false).await,
            AppletCmd::Run { id, date, out } => {
                let mut args = json!({ "id": id });
                if let Some(d) = date {
                    args["date"] = json!(d);
                }
                let data = self.console.tool("run_applet", args).await?;
                if out.json {
                    return print_json(&data);
                }
                if ui::tty() {
                    ui::ok(&format!("started run {}", cell(&data["run_id"])));
                } else {
                    println!("{}", cell(&data["run_id"]));
                }
                Ok(())
            }
            AppletCmd::Check { path, out } => {
                let (mut args, mut findings) = self.draft_args(&path).await?;
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
            PageCmd::New { title, out } => {
                let mut args = json!({ "title": title });
                if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
                    let body = std::io::read_to_string(std::io::stdin())
                        .map_err(|e| format!("reading stdin: {e}"))?;
                    if !body.trim().is_empty() {
                        args["content"] = json!(body);
                    }
                }
                let data = self.console.tool("create_page", args).await?;
                if out.json {
                    return print_json(&data);
                }
                println!("{}", cell(&data["page_id"]));
                Ok(())
            }
            PageCmd::Edit { id, find, replace, out } => {
                let replace = if replace == "-" {
                    std::io::read_to_string(std::io::stdin()).map_err(|e| format!("reading stdin: {e}"))?
                } else {
                    replace
                };
                let data = self.console
                    .tool("edit_page", json!({ "page_id": id, "find": find, "replace": replace }))
                    .await?;
                if out.json {
                    return print_json(&data);
                }
                if ui::tty() {
                    ui::ok(&format!("edited {id}"));
                }
                // Made, but not saved yet: a read of the page shows the old
                // text until the save lands. The server's sentence says so
                // and that the edit must not be run again, on stderr even
                // when piped, so a script sees it. Still a success: running
                // it again would make the edit twice.
                if data["saved"] == json!(false) {
                    note(&cell(&data["message"]));
                }
                Ok(())
            }
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

/// Run one data verb. The one dispatch for the owner's CLI and an agent key.
pub async fn run(verbs: &Verbs, command: Commands) -> Result<(), String> {
    match command {
        Commands::Query { sql, limit, out } => verbs.query(sql, limit, out).await,
        Commands::Search { text, entities, domains, after, before, limit, out } => {
            verbs.search(text, entities, domains, after, before, limit, out).await
        }
        Commands::Schema { tables, out } => verbs.schema(tables, out).await,
        Commands::Write { sql, out } => verbs.write(sql, out).await,
        Commands::Applet { cmd } => verbs.applet(cmd).await,
        Commands::Page { cmd } => verbs.page(cmd).await,
        _ => Err("not a data verb".into()),
    }
}

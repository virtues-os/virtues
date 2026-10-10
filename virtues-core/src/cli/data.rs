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
        let data = self.tool_data(tool, args).await?;
        if let Some(err) = failed_status(&data) {
            return Err(err);
        }
        Ok(data)
    }

    /// [`Console::tool`], returning the data of a result whose status says
    /// it failed too, for a verb that prints more of it than the error.
    async fn tool_data(&self, tool: &str, args: Value) -> Result<Value, String> {
        let url = format!("{}/api/console/tool/{tool}", self.base);
        let body = self.send(self.http.post(url).json(&args)).await?;
        Ok(body.get("data").cloned().unwrap_or(Value::Null))
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
                // What converting the markdown into blocks changed.
                for line in problem_lines(&data["notes"]) {
                    note(&line);
                }
                Ok(())
            }
            PageCmd::Edit { id, find, replace, base, ops, op, block, html, out } => {
                let stdin = || std::io::read_to_string(std::io::stdin()).map_err(|e| format!("reading stdin: {e}"));
                let replace = match replace {
                    Some(r) if r == "-" => Some(stdin()?),
                    r => r,
                };
                let html = match html {
                    Some(h) if h == "-" => Some(stdin()?),
                    h => h,
                };
                let ops = match ops {
                    Some(o) if o == "-" => Some(stdin()?),
                    // An agent key's verb runs on the box, where a path
                    // names the box's files, not the agent's.
                    Some(_) if self.local.is_none() => {
                        return Err("send the ops on stdin: `--ops -`. A path would name a file on the box, not on your machine".into())
                    }
                    Some(path) => Some(std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?),
                    None => None,
                };
                let args = edit_page_args(&id, find, replace, base, ops, op, block, html)?;
                let data = self.console.tool_data("edit_page", args).await?;
                if out.json {
                    print_json(&data)?;
                } else if data["status"] == json!("refused") {
                    // What to send next, and the base that has seen the
                    // blocks as they are now.
                    note(&cell(&data["message"]));
                    if let Some(read) = read_by_id(&id, &data) {
                        note(&read);
                    }
                }
                if let Some(err) = failed_status(&data) {
                    return Err(err);
                }
                if out.json {
                    return Ok(());
                }
                if ui::tty() {
                    ui::ok(&format!("edited {id}"));
                }
                for line in problem_lines(&data["notes"]) {
                    note(&line);
                }
                // Made, but not saved yet: a read of the page shows the old
                // text until the save lands. The server's sentence says so
                // and that the edit must not be run again, on stderr even
                // when piped, so a script sees it. Still a success: running
                // it again would make the edit twice.
                if data["saved"] == json!(false) {
                    note(&cell(&data["message"]));
                }
                // The base the next edit names, so edits chain with no read.
                if let Some(base) = data["base"].as_str() {
                    note(&format!("base {base}"));
                }
                Ok(())
            }
            PageCmd::Get { id, blocks, after, ids, base, out } => {
                let args = match (blocks, after) {
                    (true, Some(after)) => json!({ "page_id": id, "after": after, "base": base }),
                    (true, None) if !ids.is_empty() => json!({ "page_id": id, "ids": ids, "base": base }),
                    (true, None) => json!({ "page_id": id }),
                    (false, _) => json!({ "page_id": id, "view": "markdown" }),
                };
                let data = self.call("get_page_content", args).await?;
                if out.json {
                    return print_json(&data);
                }
                if ui::tty() {
                    heading(&cell(&data["title"]));
                    println!();
                }
                let read = page_read(&data, blocks);
                println!("{}", read.text);
                for line in read.notes {
                    note(&line);
                }
                Ok(())
            }
        }
    }
}

/// `page edit`'s arguments as `edit_page` takes them, from the flags with
/// stdin and files already read. Clap has kept the forms apart: find with
/// replace, `--ops`, or one `--op`.
#[allow(clippy::too_many_arguments)]
fn edit_page_args(
    id: &str,
    find: Option<String>,
    replace: Option<String>,
    base: Option<String>,
    ops: Option<String>,
    op: Option<String>,
    block: Option<String>,
    html: Option<String>,
) -> Result<Value, String> {
    let mut args = json!({ "page_id": id });
    if let Some(find) = find {
        args["find"] = json!(find);
        args["replace"] = json!(replace.unwrap_or_default());
        return Ok(args);
    }
    if let Some(base) = base {
        args["base"] = json!(base);
    }
    if let Some(ops) = ops {
        let ops: Value = serde_json::from_str(&ops)
            .map_err(|e| format!("--ops must hold a JSON array of {{op, id, html}} objects: {e}"))?;
        if !ops.is_array() {
            return Err("--ops must hold a JSON array of {op, id, html} objects".into());
        }
        args["ops"] = ops;
    } else if let Some(op) = op {
        let flag = op.replace('_', "-");
        let mut one = json!({ "op": op });
        match (op.as_str(), block) {
            ("append", Some(_)) => {
                return Err("--op append takes no --block: it adds at the end of the page".into())
            }
            ("append", None) => {}
            (_, Some(block)) => one["id"] = json!(block),
            (_, None) => {
                return Err(format!("--op {flag} needs --block, the id of the block it names"))
            }
        }
        match (op.as_str(), html) {
            ("delete", Some(_)) => return Err("--op delete takes no --html".into()),
            ("delete", None) => {}
            (_, Some(html)) => one["html"] = json!(html),
            (_, None) => return Err(format!("--op {flag} needs --html, the blocks it writes")),
        }
        args["ops"] = json!([one]);
    }
    Ok(args)
}

/// What `page get` prints: the page on stdout, and on stderr what goes
/// with it.
struct PageRead {
    text: String,
    notes: Vec<String>,
}

fn page_read(data: &Value, blocks: bool) -> PageRead {
    let text = ["html", "markdown", "content"]
        .iter()
        .map(|k| cell(&data[*k]))
        .find(|t| !t.is_empty())
        .unwrap_or_default();
    let mut notes = vec![];
    if blocks {
        match data["base"].as_str() {
            Some(base) => {
                if let Some(note) = data["note"].as_str() {
                    notes.push(note.to_string());
                }
                notes.push(format!("base {base}"));
                if let Some(next) = data["more_after"].as_str() {
                    notes.push(format!(
                        "more follows: page get {} --blocks --after {next} --base {base}",
                        cell(&data["page_id"])
                    ));
                }
                // The server says to read by id in its tool's words; this is
                // the same read as this command takes it.
                match read_by_id(&cell(&data["page_id"]), data) {
                    Some(read) => notes.push(read),
                    None if data["markdown"].is_string() => notes.push(format!(
                        "to edit a block exactly, read it as html: page get {} --blocks --ids <id,…> --base {base}",
                        cell(&data["page_id"])
                    )),
                    None => {}
                }
            }
            None => notes.push(
                "this page is markdown text, with no blocks: edit it with --find and --replace".into(),
            ),
        }
    }
    PageRead { text, notes }
}

/// The `page get` that reads the blocks a read or a refusal could not show
/// (`unread`), with the base it returned; none when it showed them all.
fn read_by_id(page_id: &str, data: &Value) -> Option<String> {
    let unread: Vec<&str> = array_of(&data["unread"]).iter().filter_map(Value::as_str).collect();
    let base = data["base"].as_str()?;
    (!unread.is_empty()).then(|| {
        format!("read them as html: page get {page_id} --blocks --ids {} --base {base}", unread.join(","))
    })
}

/// A tool's notes or refusals, one line each: "where: what".
fn problem_lines(problems: &Value) -> Vec<String> {
    array_of(problems)
        .iter()
        .map(|p| format!("{}: {}", cell(&p["at"]), cell(&p["message"])))
        .collect()
}

fn array_of(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::types::Cli;
    use clap::Parser;

    fn parse(words: &[&str]) -> Result<PageCmd, clap::error::Error> {
        let cli = Cli::try_parse_from(std::iter::once("virtues").chain(words.iter().copied()))?;
        match cli.command {
            Some(Commands::Page { cmd }) => Ok(cmd),
            _ => panic!("not a page verb"),
        }
    }

    /// The args `page edit` sends, from its words.
    fn edit_args(words: &[&str]) -> Result<Value, String> {
        let Ok(PageCmd::Edit { id, find, replace, base, ops, op, block, html, .. }) = parse(words) else {
            panic!("{words:?} did not parse as page edit");
        };
        edit_page_args(&id, find, replace, base, ops, op, block, html)
    }

    #[test]
    fn page_edit_takes_each_form_and_refuses_the_mixes() {
        assert_eq!(
            edit_args(&["page", "edit", "page_1", "--find", "Coffee.", "--replace", "Tea."]).unwrap(),
            json!({ "page_id": "page_1", "find": "Coffee.", "replace": "Tea." })
        );
        assert!(matches!(
            parse(&["page", "edit", "page_1", "--base", "b1", "--ops", "-"]),
            Ok(PageCmd::Edit { ops: Some(_), base: Some(_), .. })
        ));
        assert_eq!(
            edit_args(&["page", "edit", "page_1", "--base", "b1", "--op", "insert-after", "--block", "k3n1x0aa", "--html", "<p>Tea.</p>"])
                .unwrap(),
            json!({ "page_id": "page_1", "base": "b1",
                    "ops": [{ "op": "insert_after", "id": "k3n1x0aa", "html": "<p>Tea.</p>" }] })
        );
        assert_eq!(
            edit_args(&["page", "edit", "page_1", "--base", "b1", "--op", "delete", "--block", "k3n1x0aa"]).unwrap(),
            json!({ "page_id": "page_1", "base": "b1", "ops": [{ "op": "delete", "id": "k3n1x0aa" }] })
        );

        for bad in [
            // Nothing to change.
            &["page", "edit", "page_1"][..],
            // find without replace, and the other way round.
            &["page", "edit", "page_1", "--find", "Coffee."],
            &["page", "edit", "page_1", "--replace", "Tea."],
            // Both forms at once.
            &["page", "edit", "page_1", "--find", "a", "--replace", "b", "--op", "append", "--html", "<p>c</p>"],
            &["page", "edit", "page_1", "--find", "a", "--replace", "b", "--ops", "-"],
            &["page", "edit", "page_1", "--ops", "-", "--op", "append"],
            // A block or html with no op.
            &["page", "edit", "page_1", "--ops", "-", "--block", "k3n1x0aa"],
            // Not an op.
            &["page", "edit", "page_1", "--op", "move", "--block", "k3n1x0aa"],
        ] {
            assert!(parse(bad).is_err(), "{bad:?} parsed");
        }

        // What clap cannot see: each op's own needs.
        assert!(edit_args(&["page", "edit", "page_1", "--op", "replace", "--html", "<p>x</p>"])
            .unwrap_err()
            .contains("needs --block"));
        assert!(edit_args(&["page", "edit", "page_1", "--op", "insert-before", "--block", "k3n1x0aa"])
            .unwrap_err()
            .contains("--op insert-before needs --html"));
        assert!(edit_args(&["page", "edit", "page_1", "--op", "append", "--block", "k3n1x0aa", "--html", "<p>x</p>"])
            .is_err());
        assert!(edit_args(&["page", "edit", "page_1", "--op", "delete", "--block", "k3n1x0aa", "--html", "<p>x</p>"])
            .is_err());
    }

    /// An append is the one op that needs no read first.
    #[test]
    fn an_append_needs_no_base() {
        assert_eq!(
            edit_args(&["page", "edit", "page_1", "--op", "append", "--html", "<p>From the terminal.</p>"]).unwrap(),
            json!({ "page_id": "page_1", "ops": [{ "op": "append", "html": "<p>From the terminal.</p>" }] })
        );
    }

    #[test]
    fn ops_from_a_file_or_stdin_must_be_an_array() {
        let ops = r#"[{"op": "append", "html": "<p>Tea.</p>"}]"#;
        assert_eq!(
            edit_page_args("page_1", None, None, Some("b1".into()), Some(ops.into()), None, None, None).unwrap(),
            json!({ "page_id": "page_1", "base": "b1", "ops": [{ "op": "append", "html": "<p>Tea.</p>" }] })
        );
        assert!(edit_page_args("page_1", None, None, None, Some("{\"op\": \"append\"}".into()), None, None, None).is_err());
        assert!(edit_page_args("page_1", None, None, None, Some("not json".into()), None, None, None).is_err());
    }

    #[test]
    fn page_get_prints_the_markdown_and_with_blocks_the_base() {
        assert!(matches!(parse(&["page", "get", "page_1"]), Ok(PageCmd::Get { blocks: false, .. })));
        assert!(matches!(parse(&["page", "get", "page_1", "--blocks"]), Ok(PageCmd::Get { blocks: true, .. })));

        // A block page read plainly: the export, nothing on stderr.
        let plain = page_read(
            &json!({ "page_id": "page_1", "title": "Trip", "format": "tree", "markdown": "## Plan\n" }),
            false,
        );
        assert_eq!(plain.text, "## Plan\n");
        assert!(plain.notes.is_empty());

        // By block: the HTML, and the base on stderr.
        let blocks = page_read(
            &json!({ "page_id": "page_1", "format": "tree", "base": "3f9a0c2e71b4d8a6",
                     "html": "<h2 data-id=\"k3n1x0aa\">Plan</h2>", "how_to_edit": "…", "tags": "…" }),
            true,
        );
        assert_eq!(blocks.text, "<h2 data-id=\"k3n1x0aa\">Plan</h2>");
        assert_eq!(blocks.notes, ["base 3f9a0c2e71b4d8a6"]);

        // A long page read as far as one read holds: how to read on.
        let start = page_read(
            &json!({ "page_id": "page_1", "format": "tree", "base": "3f9a0c2e71b4d8a6",
                     "markdown": "<!-- k3n1x0aa -->\n## Plan\n", "note": "Long page.",
                     "more_after": "k3n1x0aa" }),
            true,
        );
        assert_eq!(
            start.notes,
            [
                "Long page.",
                "base 3f9a0c2e71b4d8a6",
                "more follows: page get page_1 --blocks --after k3n1x0aa --base 3f9a0c2e71b4d8a6",
                "to edit a block exactly, read it as html: page get page_1 --blocks --ids <id,…> --base 3f9a0c2e71b4d8a6",
            ]
        );
        assert!(matches!(
            parse(&["page", "get", "page_1", "--blocks", "--after", "k3n1x0aa", "--base", "3f9a"]),
            Ok(PageCmd::Get { after: Some(_), base: Some(_), .. })
        ));
        assert!(parse(&["page", "get", "page_1", "--blocks", "--after", "k3n1x0aa"]).is_err());

        // The server's notes send the reader to an ids read: the command
        // has one, and says it in its own terms.
        assert!(matches!(
            parse(&["page", "get", "page_1", "--blocks", "--ids", "k3n1x0aa,p0q9r8st", "--base", "3f9a"]),
            Ok(PageCmd::Get { ids, base: Some(_), .. }) if ids == ["k3n1x0aa", "p0q9r8st"]
        ));
        assert!(parse(&["page", "get", "page_1", "--blocks", "--ids", "k3n1x0aa"]).is_err());
        assert!(parse(&["page", "get", "page_1", "--ids", "k3n1x0aa", "--base", "3f9a"]).is_err());
        assert!(parse(&["page", "get", "page_1", "--blocks", "--base", "3f9a"]).is_err());
        let long = page_read(
            &json!({ "page_id": "page_1", "format": "tree", "base": "3f9a0c2e71b4d8a6",
                     "markdown": "<!-- k3n1x0aa -->\n## Plan\n", "note": "Long page, shown as markdown." }),
            true,
        );
        assert_eq!(
            long.notes.last().unwrap(),
            "to edit a block exactly, read it as html: page get page_1 --blocks --ids <id,…> --base 3f9a0c2e71b4d8a6"
        );
        let unread = page_read(
            &json!({ "page_id": "page_1", "format": "tree", "base": "3f9a0c2e71b4d8a6",
                     "html": "<ul data-id=\"l1\"></ul>", "unread": ["k3n1x0aa", "p0q9r8st"] }),
            true,
        );
        assert_eq!(
            unread.notes.last().unwrap(),
            "read them as html: page get page_1 --blocks --ids k3n1x0aa,p0q9r8st --base 3f9a0c2e71b4d8a6"
        );
        let refused = json!({ "applied": false, "status": "refused", "base": "7c1d", "unread": ["k3n1x0aa"] });
        assert_eq!(
            read_by_id("page_1", &refused).unwrap(),
            "read them as html: page get page_1 --blocks --ids k3n1x0aa --base 7c1d"
        );

        // A markdown page has no blocks to print.
        let text = page_read(&json!({ "format": "markdown", "content": "Coffee.\n" }), true);
        assert_eq!(text.text, "Coffee.\n");
        assert!(text.notes[0].contains("--find"));
    }

    /// A refused edit is a result, not an error, to the tool; to the CLI it
    /// is a failure, so a script stops on it.
    #[test]
    fn a_refused_edit_fails_the_verb() {
        let refused = json!({
            "applied": false, "status": "refused",
            "error": "Nothing was written. op 1: no block has id `zz81aa00`",
            "refused": [{ "at": "op 1", "message": "no block has id `zz81aa00`" }],
            "base": "5e0d2c7f9a81b3c4",
            "message": "Change the refused ops as the problems say, then send the whole batch again with base 5e0d2c7f9a81b3c4, the ops that were not refused as you wrote them.",
        });
        assert_eq!(
            failed_status(&refused).as_deref(),
            Some("Nothing was written. op 1: no block has id `zz81aa00`")
        );
        assert_eq!(problem_lines(&refused["refused"]), ["op 1: no block has id `zz81aa00`"]);
        assert_eq!(failed_status(&json!({ "applied": true, "saved": true })), None);
    }
}

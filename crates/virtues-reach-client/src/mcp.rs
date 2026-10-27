//! The MCP bridge for an agent on the owner's machine (agents/plan/mcp-plan.md,
//! tier 1).
//!
//! Claude Code, Claude Desktop and Cursor launch a local MCP server as a
//! process and speak newline-delimited JSON-RPC to it over stdio. This module
//! is that process's body: it relays each message to the box's `/api/mcp`
//! over iroh and writes the box's answers back to stdout.
//!
//! The bridge is its own device, kind `agent`, with its own iroh key. The box
//! lets an agent key reach `/api/mcp` and no other route, and the owner can
//! revoke it on its own. Its record lives in a file of its own under the
//! host's directory, so pairing it never touches the app's own pairing.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::model::PairedBox;
use crate::store::BoxStore;

/// The device kind the box records for a bridge.
pub const AGENT_KIND: &str = "agent";

/// What the bridge reports as `device_info.client`.
pub const CLIENT: &str = "virtues-mcp";

/// The box's MCP endpoint (`virtues-core/src/server/api/mcp.rs`).
const MCP_PATH: &str = "/api/mcp";

/// One agent's paired record: a `0600` JSON file at `<dir>/agents/<name>.json`.
pub struct AgentStore {
    path: PathBuf,
}

impl AgentStore {
    /// The store for the agent called `name` under the host's directory.
    pub fn new(dir: &Path, name: &str) -> Self {
        AgentStore { path: dir.join("agents").join(format!("{}.json", file_stem(name))) }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// A file name for an agent's name: lowercase letters, digits and dashes.
fn file_stem(name: &str) -> String {
    let mut stem = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            stem.push(c.to_ascii_lowercase());
        } else if !stem.ends_with('-') {
            stem.push('-');
        }
    }
    let stem = stem.trim_matches('-').to_string();
    if stem.is_empty() {
        "agent".to_string()
    } else {
        stem
    }
}

impl BoxStore for AgentStore {
    fn load(&self) -> Result<Option<PairedBox>> {
        match std::fs::read_to_string(&self.path) {
            Ok(json) => Ok(Some(serde_json::from_str(&json).context("read the agent's record")?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn save(&self, rec: &PairedBox) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(rec)?;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        use std::io::Write;
        options.open(&self.path)?.write_all(json.as_bytes())?;
        Ok(())
    }

    fn delete(&self) -> Result<()> {
        match std::fs::remove_file(&self.path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }
}

/// Pairs a new agent called `name` with a pair token minted for kind `agent`,
/// through `origin` (the app's loopback proxy, which reaches the box from
/// anywhere). Returns the stored record's path.
pub async fn pair(dir: &Path, origin: &str, token: &str, name: &str) -> Result<PathBuf> {
    let store = AgentStore::new(dir, name);
    let info = json!({ "client": CLIENT, "device_name": name.trim() });
    crate::pair::consume(&store, origin, token, AGENT_KIND, info).await?;
    Ok(store.path)
}

/// Serves MCP on stdio for the agent whose record `store` holds, until stdin
/// closes. Nothing but JSON-RPC is written to stdout; diagnostics go to
/// stderr.
pub async fn serve_stdio(store: &AgentStore) -> Result<()> {
    let rec = store.load()?.ok_or_else(|| {
        anyhow!(
            "this agent is not connected to your server. In the Virtues app, open Devices, \
             choose Add device, then choose An AI agent on this Mac."
        )
    })?;
    let client = crate::proxy::build_client(&rec).await?;
    let stdout = Arc::new(tokio::sync::Mutex::new(tokio::io::stdout()));
    let version: Arc<Mutex<Option<String>>> = Arc::default();
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut calls = tokio::task::JoinSet::new();

    while let Some(line) = lines.next_line().await? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let message: Value = match serde_json::from_str(line) {
            Ok(message) => message,
            Err(e) => {
                let reply = json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": { "code": -32700, "message": format!("Parse error: {e}") },
                });
                write_line(&stdout, &reply).await?;
                continue;
            }
        };
        let (client, stdout, version) = (client.clone(), stdout.clone(), version.clone());
        calls.spawn(async move {
            let known = version.lock().map(|v| v.clone()).unwrap_or(None);
            let raw = request_bytes(&message, known.as_deref());
            let replies = match client.request(&raw).await {
                Ok(response) => replies(&message, &response),
                Err(e) => error_reply(&message, &format!("Could not reach your server: {e:#}")).into_iter().collect(),
            };
            if let Some(negotiated) = negotiated_version(&message, &replies) {
                if let Ok(mut v) = version.lock() {
                    *v = Some(negotiated);
                }
            }
            for reply in replies {
                if let Err(e) = write_line(&stdout, &reply).await {
                    eprintln!("virtues mcp: could not write to stdout: {e}");
                }
            }
        });
    }
    while calls.join_next().await.is_some() {}
    Ok(())
}

async fn write_line(stdout: &tokio::sync::Mutex<tokio::io::Stdout>, message: &Value) -> Result<()> {
    let mut line = message.to_string();
    line.push('\n');
    let mut out = stdout.lock().await;
    out.write_all(line.as_bytes()).await?;
    out.flush().await?;
    Ok(())
}

/// The raw HTTP/1 request that carries `message` to `/api/mcp`.
///
/// Streamable HTTP routes by headers: `Mcp-Method` always, `Mcp-Name` for a
/// call that names its target, and `MCP-Protocol-Version` once a version is
/// known, from the message's own `_meta` (2026-07-28 clients) or from the
/// `initialize` answer (older clients).
fn request_bytes(message: &Value, known_version: Option<&str>) -> Vec<u8> {
    let body = message.to_string();
    let method = message.get("method").and_then(Value::as_str);
    let params = message.get("params");
    let name = match method {
        Some("tools/call") | Some("prompts/get") => params.and_then(|p| p.get("name")).and_then(Value::as_str),
        Some("resources/read") => params.and_then(|p| p.get("uri")).and_then(Value::as_str),
        _ => None,
    };
    let version = params
        .and_then(|p| p.get("_meta"))
        .and_then(|m| m.get("io.modelcontextprotocol/protocolVersion"))
        .and_then(Value::as_str)
        .or(known_version);

    let mut head = format!(
        "POST {MCP_PATH} HTTP/1.1\r\nHost: virtues\r\nContent-Type: application/json\r\n\
         Accept: application/json, text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (header, value) in [("Mcp-Method", method), ("Mcp-Name", name), ("MCP-Protocol-Version", version)] {
        // A value that cannot sit in a header line is left out; the body
        // still carries it.
        if let Some(value) = value.filter(|v| v.bytes().all(|b| (0x20..0x7f).contains(&b))) {
            head.push_str(&format!("{header}: {value}\r\n"));
        }
    }
    head.push_str("\r\n");
    let mut raw = head.into_bytes();
    raw.extend_from_slice(body.as_bytes());
    raw
}

/// The JSON-RPC messages in the box's answer to `message`. An answer that
/// carries none, for a request, becomes an error reply so the agent is not
/// left waiting.
fn replies(message: &Value, response: &[u8]) -> Vec<Value> {
    match read_response(response) {
        Ok((status, content_type, body)) => {
            let found = if content_type.starts_with("text/event-stream") {
                event_stream_messages(&body)
            } else {
                json_messages(&body)
            };
            if !found.is_empty() || (200..300).contains(&status) && body.trim().is_empty() {
                return found;
            }
            let why = format!("Your server answered {status}: {}", body.trim());
            error_reply(message, &why).into_iter().collect()
        }
        Err(e) => error_reply(message, &format!("{e:#}")).into_iter().collect(),
    }
}

fn json_messages(body: &str) -> Vec<Value> {
    match serde_json::from_str::<Value>(body.trim()) {
        Ok(Value::Array(batch)) => batch,
        Ok(one) if one.get("jsonrpc").is_some() => vec![one],
        _ => Vec::new(),
    }
}

/// The `data:` payload of each server-sent event, as JSON-RPC.
fn event_stream_messages(body: &str) -> Vec<Value> {
    body.replace("\r\n", "\n")
        .split("\n\n")
        .filter_map(|event| {
            let data: Vec<&str> = event
                .lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .map(|d| d.strip_prefix(' ').unwrap_or(d))
                .collect();
            serde_json::from_str::<Value>(&data.join("\n")).ok()
        })
        .filter(|m| m.get("jsonrpc").is_some())
        .collect()
}

/// An error reply for a request; a notification gets none.
fn error_reply(message: &Value, why: &str) -> Option<Value> {
    let id = message.get("id").filter(|_| message.get("method").is_some())?;
    Some(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32603, "message": why } }))
}

/// The version an `initialize` answer settled on.
fn negotiated_version(message: &Value, replies: &[Value]) -> Option<String> {
    if message.get("method").and_then(Value::as_str) != Some("initialize") {
        return None;
    }
    replies
        .iter()
        .find_map(|r| r.pointer("/result/protocolVersion").and_then(Value::as_str))
        .map(str::to_string)
}

/// Splits a raw HTTP/1 response into status, content type and body, undoing
/// chunked transfer encoding.
fn read_response(response: &[u8]) -> Result<(u16, String, String)> {
    let split = response
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| anyhow!("your server sent a malformed answer"))?;
    let head = String::from_utf8_lossy(&response[..split]);
    let mut lines = head.lines();
    let status = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse::<u16>().ok())
        .ok_or_else(|| anyhow!("your server sent a malformed status line"))?;
    let mut content_type = String::new();
    let mut chunked = false;
    for line in lines {
        let Some((key, value)) = line.split_once(':') else { continue };
        let (key, value) = (key.trim().to_ascii_lowercase(), value.trim().to_ascii_lowercase());
        if key == "content-type" {
            content_type = value;
        } else if key == "transfer-encoding" && value.contains("chunked") {
            chunked = true;
        }
    }
    let body = &response[split + 4..];
    let body = if chunked { dechunk(body)? } else { body.to_vec() };
    Ok((status, content_type, String::from_utf8_lossy(&body).into_owned()))
}

fn dechunk(mut body: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let line_end = body
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or_else(|| anyhow!("your server sent a malformed chunk"))?;
        let size_text = String::from_utf8_lossy(&body[..line_end]);
        let size = usize::from_str_radix(size_text.split(';').next().unwrap_or("").trim(), 16)
            .context("read a chunk size")?;
        body = &body[line_end + 2..];
        if size == 0 {
            return Ok(out);
        }
        if body.len() < size {
            bail!("your server's answer ended inside a chunk");
        }
        out.extend_from_slice(&body[..size]);
        body = body.get(size + 2..).unwrap_or(&[]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tool_call_carries_its_routing_headers() {
        let call = json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "sql_query" } });
        let raw = String::from_utf8(request_bytes(&call, Some("2025-11-25"))).unwrap();
        assert!(raw.starts_with("POST /api/mcp HTTP/1.1\r\n"), "{raw}");
        assert!(raw.contains("Mcp-Method: tools/call\r\n"), "{raw}");
        assert!(raw.contains("Mcp-Name: sql_query\r\n"), "{raw}");
        assert!(raw.contains("MCP-Protocol-Version: 2025-11-25\r\n"), "{raw}");
        assert!(raw.ends_with(&call.to_string()), "{raw}");
    }

    #[test]
    fn a_2026_message_names_its_own_version() {
        let ping = json!({ "jsonrpc": "2.0", "id": 1, "method": "ping",
            "params": { "_meta": { "io.modelcontextprotocol/protocolVersion": "2026-07-28" } } });
        let raw = String::from_utf8(request_bytes(&ping, Some("2025-11-25"))).unwrap();
        assert!(raw.contains("MCP-Protocol-Version: 2026-07-28\r\n"), "{raw}");
    }

    #[test]
    fn json_chunked_and_event_stream_answers_all_read() {
        let call = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" });
        let json_answer = b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 36\r\n\r\n{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}";
        assert_eq!(replies(&call, json_answer)[0]["id"], 1);

        let chunked = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n\
            10\r\n{\"jsonrpc\":\"2.0\"\r\n14\r\n,\"id\":1,\"result\":{}}\r\n0\r\n\r\n";
        assert_eq!(replies(&call, chunked)[0]["result"], json!({}));

        let stream = b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\r\n\
            data: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\"}\n\n\
            id: 7\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n\n";
        let found = replies(&call, stream);
        assert_eq!(found.len(), 2);
        assert_eq!(found[1]["id"], 1);
    }

    #[test]
    fn an_accepted_notification_gets_no_reply_and_a_refusal_answers_the_request() {
        let note = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        assert!(replies(&note, b"HTTP/1.1 202 Accepted\r\ncontent-length: 0\r\n\r\n").is_empty());

        let call = json!({ "jsonrpc": "2.0", "id": "a", "method": "tools/list" });
        let refused = replies(&call, b"HTTP/1.1 403 Forbidden\r\ncontent-type: application/json\r\n\r\n{\"error\":\"no\"}");
        assert_eq!(refused[0]["id"], "a");
        assert!(refused[0]["error"]["message"].as_str().unwrap().contains("403"));
    }

    #[test]
    fn the_initialize_answer_sets_the_version() {
        let init = json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize" });
        let answer = vec![json!({ "jsonrpc": "2.0", "id": 0, "result": { "protocolVersion": "2025-11-25" } })];
        assert_eq!(negotiated_version(&init, &answer).as_deref(), Some("2025-11-25"));
    }

    #[test]
    fn an_agent_name_becomes_a_file_name() {
        let store = AgentStore::new(Path::new("/x"), "Claude Code");
        assert_eq!(store.path(), Path::new("/x/agents/claude-code.json"));
        assert_eq!(file_stem("  "), "agent");
    }
}

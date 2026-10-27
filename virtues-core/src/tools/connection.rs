//! Connections: other services' MCP servers, whose tools the assistant can use
//! (agents/plan/mcp-plan.md, part 1). Tools only; a connection never syncs.
//!
//! Signing in is OAuth 2.1 with PKCE through rmcp, as a public client the box
//! registers with the service at sign-in (dynamic client registration). The
//! redirect comes back to the box at the address the owner's app reaches it
//! by, so no proxy ever holds the code or the token. The tokens live sealed in
//! `credentials` (kind `source`, id `mcp:<connection>`), and rmcp refreshes
//! them through the same row.
//!
//! The chat reaches a connection through one tool, `connection`, which runs a
//! worker loop with only that service's tools on the Lite slot, the way
//! `browse` does. The chat's own tool list stays the same whatever is
//! connected, so its cached prompt does too.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use futures::StreamExt;
use rmcp::model::{CallToolRequestParams, ClientConfig, Tool};
use rmcp::service::RunningService;
use rmcp::transport::auth::{
    AuthClient, AuthError, AuthorizationManager, AuthorizationRequest, CredentialStore, OAuthState,
    StoredCredentials,
};
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::{RoleClient, ServiceExt};
use serde_json::{json, Value};
use sqlx::PgPool;
use tokio::sync::Mutex;
use virtues_helpers::crypto::TokenEncryptor;

use super::{ToolContext, ToolResult};
use crate::agent::{AgentConfig, AgentEvent, AgentLoop, DelegatedUsage, Thinking, TurnBudget};
use crate::api::compaction::build_context_for_llm;

/// The chat's tool.
pub const TOOL: &str = "connection";

/// Where a sign-in comes back to: `GET` on the box, at whatever origin the
/// owner's app reaches it by (`server/api/connections.rs`).
pub const CALLBACK_PATH: &str = "/api/connections/callback";

/// One service the box can connect to.
pub struct Connection {
    pub id: &'static str,
    pub name: &'static str,
    /// What its tools reach, for the chat tool's description of it.
    pub about: &'static str,
    pub url: &'static str,
}

// TODO(2026-10-26): migrate this catalog into applets/sources.toml as an
// `mcp_oauth` source auth kind, as agents/plan/mcp-plan.md says.
pub const CATALOG: &[Connection] = &[Connection {
    id: "linear",
    name: "Linear",
    about: "issues, projects, cycles and comments",
    url: "https://mcp.linear.app/mcp",
}];

pub fn find(id: &str) -> Option<&'static Connection> {
    CATALOG.iter().find(|c| c.id == id)
}

/// A sign-in that has sent the owner to the service and waits for the code.
const SIGN_IN_WINDOW: Duration = Duration::from_secs(10 * 60);

/// The worker's ceilings: one task in one service.
const MAX_STEPS: u32 = 20;
const MAX_COST_MICROS: i64 = 250_000;
const MAX_WALL_CLOCK: Duration = Duration::from_secs(5 * 60);

const PROMPT: &str = "You act in one service the owner connected, using only its tools. Do the task you are given and nothing more.

Read before you change anything: find the exact item the task names. If two items could match, stop and say which ones.

Deleting, archiving or overwriting something needs the owner's OK. If the task needs it and those tools are not offered to you, stop and say exactly what you would change.

When you finish, answer in a few sentences: what you did or found, with each item's identifier and link as the service returned them.";

// ── Stored tokens ───────────────────────────────────────────────────────────

fn credential_id(connection: &Connection) -> String {
    format!("mcp:{}", connection.id)
}

/// rmcp's credential store over one sealed `credentials` row.
struct Stored {
    pool: PgPool,
    connection: &'static Connection,
}

fn store_error(e: impl std::fmt::Display) -> AuthError {
    AuthError::CredentialStoreError(e.to_string())
}

#[async_trait::async_trait]
impl CredentialStore for Stored {
    async fn load(&self) -> Result<Option<StoredCredentials>, AuthError> {
        let sealed: Option<String> = sqlx::query_scalar(
            "SELECT secrets_ciphertext FROM credentials WHERE id = $1 AND kind = 'source' AND status = 'active'",
        )
        .bind(credential_id(self.connection))
        .fetch_optional(&self.pool)
        .await
        .map_err(store_error)?;
        let Some(sealed) = sealed else { return Ok(None) };
        let json = TokenEncryptor::from_env().map_err(store_error)?.decrypt(&sealed).map_err(store_error)?;
        Ok(Some(serde_json::from_str(&json).map_err(store_error)?))
    }

    async fn save(&self, credentials: StoredCredentials) -> Result<(), AuthError> {
        let json = serde_json::to_string(&credentials).map_err(store_error)?;
        let sealed = TokenEncryptor::from_env().map_err(store_error)?.encrypt(&json).map_err(store_error)?;
        sqlx::query(
            "INSERT INTO credentials (id, kind, key, name, status, secrets_ciphertext, metadata)
             VALUES ($1, 'source', $2, $3, 'active', $4, $5)
             ON CONFLICT (id) DO UPDATE
               SET secrets_ciphertext = EXCLUDED.secrets_ciphertext, status = 'active',
                   metadata = EXCLUDED.metadata, updated_at = now()",
        )
        .bind(credential_id(self.connection))
        .bind(self.connection.id)
        .bind(self.connection.name)
        .bind(&sealed)
        .bind(json!({ "mcp": self.connection.url }))
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(())
    }

    async fn clear(&self) -> Result<(), AuthError> {
        disconnect(&self.pool, self.connection).await.map_err(store_error)
    }
}

/// Whether the box holds a live sign-in for `connection`.
pub async fn is_connected(pool: &PgPool, connection: &Connection) -> Result<bool> {
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM credentials WHERE id = $1 AND kind = 'source' AND status = 'active'",
    )
    .bind(credential_id(connection))
    .fetch_one(pool)
    .await?;
    Ok(n > 0)
}

/// Forgets the sign-in. The service keeps its own record of the grant until
/// the owner removes it there.
pub async fn disconnect(pool: &PgPool, connection: &Connection) -> Result<()> {
    sqlx::query("DELETE FROM credentials WHERE id = $1 AND kind = 'source'")
        .bind(credential_id(connection))
        .execute(pool)
        .await?;
    clients().lock().await.remove(connection.id);
    Ok(())
}

// ── Signing in ──────────────────────────────────────────────────────────────

struct Pending {
    connection: &'static Connection,
    state: OAuthState,
    started: Instant,
}

fn pending() -> &'static Mutex<HashMap<String, Pending>> {
    static PENDING: OnceLock<Mutex<HashMap<String, Pending>>> = OnceLock::new();
    PENDING.get_or_init(Default::default)
}

/// Starts a sign-in and returns the service's authorization URL for the
/// owner's browser. `redirect_uri` is [`CALLBACK_PATH`] at the origin the
/// owner's app uses.
pub async fn start(pool: &PgPool, connection: &'static Connection, redirect_uri: &str) -> Result<String> {
    let mut state = OAuthState::new(connection.url, None).await.context("reach the service")?;
    if let OAuthState::Unauthorized(manager) = &mut state {
        manager.set_credential_store(Stored { pool: pool.clone(), connection });
    }
    let request = AuthorizationRequest::new(redirect_uri)
        .with_client_name("Virtues")
        .with_application_type("native");
    state.start_authorization(request).await.context("start the sign-in")?;
    let url = state.get_authorization_url().await.context("build the sign-in address")?;
    let csrf = url::Url::parse(&url)?
        .query_pairs()
        .find(|(k, _)| k == "state")
        .map(|(_, v)| v.into_owned())
        .ok_or_else(|| anyhow!("the service's sign-in address carries no state"))?;

    let mut waiting = pending().lock().await;
    waiting.retain(|_, p| p.started.elapsed() < SIGN_IN_WINDOW);
    waiting.insert(csrf, Pending { connection, state, started: Instant::now() });
    Ok(url)
}

/// Finishes a sign-in from the redirect's query. Returns the connection.
pub async fn finish(code: &str, csrf: &str, issuer: Option<&str>) -> Result<&'static Connection> {
    let Some(mut sign_in) = pending().lock().await.remove(csrf) else {
        bail!("this sign-in is unknown or more than ten minutes old. Start it again.");
    };
    if sign_in.started.elapsed() >= SIGN_IN_WINDOW {
        bail!("this sign-in took more than ten minutes. Start it again.");
    }
    sign_in
        .state
        .handle_callback_with_issuer(code, csrf, issuer)
        .await
        .context("exchange the code for a token")?;
    clients().lock().await.remove(sign_in.connection.id);
    Ok(sign_in.connection)
}

// ── The client ──────────────────────────────────────────────────────────────

type Client = RunningService<RoleClient, ClientConfig>;

fn clients() -> &'static Mutex<HashMap<&'static str, Arc<Client>>> {
    static CLIENTS: OnceLock<Mutex<HashMap<&'static str, Arc<Client>>>> = OnceLock::new();
    CLIENTS.get_or_init(Default::default)
}

/// A live MCP session with `connection`, signed in from its stored tokens.
async fn client(pool: &PgPool, connection: &'static Connection) -> Result<Arc<Client>> {
    if let Some(client) = clients().lock().await.get(connection.id) {
        if !client.is_transport_closed() {
            return Ok(client.clone());
        }
    }
    let mut manager = AuthorizationManager::new(connection.url).await.context("reach the service")?;
    manager.set_credential_store(Stored { pool: pool.clone(), connection });
    if !manager.initialize_from_store().await.context("read the sign-in")? {
        bail!("{} is not connected. Connect it in Settings, then try again.", connection.name);
    }
    let transport = StreamableHttpClientTransport::with_client(
        AuthClient::new(reqwest_rmcp::Client::default(), manager),
        StreamableHttpClientTransportConfig::with_uri(connection.url),
    );
    let client = Arc::new(
        ClientConfig::default()
            .serve(transport)
            .await
            .with_context(|| format!("open a session with {}", connection.name))?,
    );
    clients().lock().await.insert(connection.id, client.clone());
    Ok(client)
}

/// The tool name the worker sees for one of a connection's tools.
fn prefixed(connection: &Connection, tool: &str) -> String {
    format!("{}__{tool}", connection.id)
}

/// Splits a prefixed tool name into its connection and the service's name.
pub fn split(name: &str) -> Option<(&'static Connection, &str)> {
    let (id, tool) = name.split_once("__")?;
    Some((find(id)?, tool))
}

/// Whether a tool declares that it destroys or overwrites. Only an explicit
/// `true` counts: the owner decided assistant actions do not ask, and deletes
/// and real money do.
fn destructive(tool: &Tool) -> bool {
    tool.annotations.as_ref().and_then(|a| a.destructive_hint) == Some(true)
}

/// The connection's tools as LLM definitions, leaving out the destructive
/// ones unless the owner confirmed.
async fn tool_definitions(client: &Client, connection: &Connection, confirmed: bool) -> Result<Vec<Value>> {
    let tools = client.list_all_tools().await.context("list the service's tools")?;
    Ok(tools
        .iter()
        .filter(|t| confirmed || !destructive(t))
        .map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": prefixed(connection, &t.name),
                    "description": t.description.as_deref().unwrap_or(""),
                    "parameters": Value::Object((*t.input_schema).clone()),
                }
            })
        })
        .collect())
}

/// Runs one of a connection's tools, for the worker. A destructive tool runs
/// only when the owner confirmed in chat.
pub async fn call_tool(pool: &PgPool, name: &str, arguments: Value, context: &ToolContext) -> ToolResult {
    let Some((connection, tool)) = split(name) else {
        return ToolResult::error(format!("{name} is not a connected service's tool."));
    };
    let client = match client(pool, connection).await {
        Ok(client) => client,
        Err(e) => return ToolResult::error(format!("{e:#}")),
    };
    if !context.owner_confirmed {
        let listed = client.list_all_tools().await.unwrap_or_default();
        if listed.iter().any(|t| t.name == tool && destructive(t)) {
            return ToolResult::error(format!(
                "{tool} deletes or overwrites something in {}. Stop and say what it would change; the owner has to agree first.",
                connection.name
            ));
        }
    }
    let arguments = match arguments {
        Value::Object(map) => Some(map),
        Value::Null => None,
        other => return ToolResult::error(format!("{tool} takes an object of arguments, not {other}")),
    };
    let mut request = CallToolRequestParams::new(tool.to_string());
    request.arguments = arguments;
    let outcome = client.call_tool(request).await;
    tracing::info!(audit = "connection_tool", connection = connection.id, tool, ok = outcome.is_ok(), "connection tool call");
    match outcome {
        Ok(result) => {
            let text: Vec<String> = result
                .content
                .iter()
                .filter_map(|c| c.as_text().map(|t| t.text.clone()))
                .collect();
            let data = json!({
                "content": text.join("\n"),
                "structured": result.structured_content,
            });
            if result.is_error == Some(true) {
                ToolResult::error(text.join("\n"))
            } else {
                ToolResult::success(data)
            }
        }
        Err(e) => {
            // A dead session is dropped so the next call opens a fresh one.
            clients().lock().await.remove(connection.id);
            ToolResult::error(format!("{} did not answer: {e}", connection.name))
        }
    }
}

// ── The chat's tool ─────────────────────────────────────────────────────────

/// The chat's `connection` call: run the task in one connected service on
/// the Lite slot's model, and hand the chat the answer and the spend.
pub async fn run_tool(pool: &PgPool, args: &Value, context: &ToolContext) -> ToolResult {
    let name = args.get("name").and_then(Value::as_str).unwrap_or("").trim().to_lowercase();
    let Some(task) = args.get("task").and_then(Value::as_str).filter(|t| !t.trim().is_empty()) else {
        return ToolResult::error("connection needs a `task`.");
    };
    let confirmed = args.get("confirmed").and_then(Value::as_bool) == Some(true);

    let mut connected = Vec::new();
    for c in CATALOG {
        match is_connected(pool, c).await {
            Ok(true) => connected.push(c),
            Ok(false) => {}
            Err(e) => return ToolResult::error(format!("Could not read the connections: {e}")),
        }
    }
    let Some(connection) = connected.iter().copied().find(|c| c.id == name || c.name.to_lowercase() == name) else {
        let list: Vec<String> = connected.iter().map(|c| format!("{} ({})", c.id, c.about)).collect();
        return ToolResult::error(if list.is_empty() {
            "No services are connected. The owner can connect one in Settings.".to_string()
        } else {
            format!("`{name}` is not connected. Connected: {}.", list.join(", "))
        });
    };

    let model = match crate::virtues_api::completion::background_model_for_slot(
        pool,
        virtues_registry::models::ModelSlot::Lite,
    )
    .await
    {
        Ok(m) if !m.trim().is_empty() => m,
        Ok(_) => crate::api::model_catalog::model_for_slot(virtues_registry::models::ModelSlot::Lite),
        Err(e) => return ToolResult::error(format!("Could not read which model works connections: {e}")),
    };
    let client = match client(pool, connection).await {
        Ok(client) => client,
        Err(e) => return ToolResult::error(format!("{e:#}")),
    };
    let tools = match tool_definitions(&client, connection, confirmed).await {
        Ok(tools) => tools,
        Err(e) => return ToolResult::error(format!("{e:#}")),
    };

    let mut messages = build_context_for_llm(&[], None, 0, Some(PROMPT), None, None);
    messages.push(json!({ "role": "user", "content": format!("Service: {}\n\nTask: {task}", connection.name) }));
    let agent = AgentLoop::new(pool.clone())
        .with_config(AgentConfig { max_steps: MAX_STEPS, thinking: Thinking::Low, ..Default::default() })
        .with_budget(TurnBudget {
            max_cost_micros: Some(MAX_COST_MICROS),
            max_wall_clock: Some(MAX_WALL_CLOCK),
            tool_caps: &[],
        });
    // Its own cache session beside the chat's, as `browse` keeps.
    let worker = ToolContext {
        user_id: Some(TOOL.to_string()),
        chat_id: context.chat_id.as_ref().map(|chat| format!("{chat}:{TOOL}")),
        cancel_token: context.cancel_token.clone(),
        owner_confirmed: confirmed,
        ..Default::default()
    };
    let cancel = context.cancel_token.clone();
    let mut stream = agent.run(model.clone(), messages, tools, worker, cancel);

    let mut answer = String::new();
    let mut changed: Vec<String> = Vec::new();
    let mut usage = DelegatedUsage { model: model.clone(), ..Default::default() };
    while let Some(event) = stream.next().await {
        match event {
            AgentEvent::TextDelta { content } => answer.push_str(&content),
            AgentEvent::ToolCallStart { name, .. } => {
                answer.clear();
                if let Some((_, tool)) = split(&name) {
                    changed.push(tool.to_string());
                }
            }
            AgentEvent::Usage { prompt_tokens, completion_tokens, reasoning_tokens, cache_read_tokens, cost_micros, .. } => {
                usage.input_tokens += u64::from(prompt_tokens);
                usage.output_tokens += u64::from(completion_tokens);
                usage.reasoning_tokens += u64::from(reasoning_tokens.unwrap_or(0));
                usage.cache_read_tokens += u64::from(cache_read_tokens.unwrap_or(0));
                usage.cost_micros += cost_micros.unwrap_or(0);
            }
            AgentEvent::Error { message, .. } => tracing::warn!(error = %message, "connection worker error"),
            _ => {}
        }
    }
    let usage = DelegatedUsage { feature: Some(TOOL.to_string()), ..usage };
    ToolResult::success(json!({
        "service": connection.name,
        "answer": answer.trim(),
        "tools_used": changed,
        "usage": [usage],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prefixed_name_splits_back_to_its_connection() {
        let linear = find("linear").unwrap();
        assert_eq!(prefixed(linear, "create_issue"), "linear__create_issue");
        let (c, tool) = split("linear__create_issue").unwrap();
        assert_eq!((c.id, tool), ("linear", "create_issue"));
        assert!(split("nobody__x").is_none());
        assert!(split("sql_query").is_none());
    }

    #[test]
    fn only_an_explicit_destructive_hint_counts() {
        let schema = Arc::new(serde_json::Map::new());
        let plain = Tool::new("list_issues", "", schema.clone());
        assert!(!destructive(&plain));
        let delete = Tool::new("delete_issue", "", schema)
            .with_annotations(rmcp::model::ToolAnnotations::new().destructive(true));
        assert!(destructive(&delete));
    }

    /// The chat is told which services are connected when it names another.
    #[sqlx::test(migrations = "./migrations")]
    async fn an_unconnected_service_is_refused_with_the_list(pool: PgPool) {
        let result = run_tool(&pool, &json!({ "name": "linear", "task": "list my issues" }), &ToolContext::default()).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("No services are connected"));
    }
}

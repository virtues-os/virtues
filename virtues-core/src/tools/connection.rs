//! Connections: other services' MCP servers, whose tools the assistant can use
//! (agents/plan/mcp-plan.md, part 1). Tools only; a connection never syncs.
//!
//! Any server in the public MCP Registry can be connected, or any server
//! address the owner pastes (`connection_registry.rs`). Signing in is OAuth
//! 2.1 with PKCE through rmcp, as a public client the box registers with the
//! service at sign-in (dynamic client registration).
//!
//! The service sends the owner's browser back to the OAuth proxy
//! (`services/virtues-api/src/routes/connections.rs`), never to the box. On a
//! phone the app reaches the box through a listener inside the app, and
//! Safari cannot reach it, so a sign-in that returned there failed. The proxy
//! keeps the code, and the box claims it server to server and exchanges it.
//! The proxy cannot use the code: the exchange needs the PKCE verifier, which
//! never leaves the box. The tokens live sealed in `credentials` (kind
//! `connection`, key the connection's short name), with the server's address
//! in the row's metadata, and rmcp refreshes them through the same row.
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
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use tokio::sync::Mutex;
use virtues_helpers::crypto::TokenEncryptor;

use super::{ToolContext, ToolResult};
use crate::agent::{AgentConfig, AgentEvent, AgentLoop, DelegatedUsage, Thinking, TurnBudget};
use crate::api::compaction::build_context_for_llm;

/// The chat's tool.
pub const TOOL: &str = "connection";

/// Where every sign-in comes back to: the OAuth proxy's return route.
/// `sheet` asks the proxy to close the iOS app's sign-in sheet once the
/// sign-in lands there.
fn redirect_uri(sheet: bool) -> String {
    let base = format!("{}/connections/callback", virtues_helpers::auth::proxy_url());
    if sheet {
        format!("{base}?sheet=1")
    } else {
        base
    }
}

/// How often the box asks the proxy whether a sign-in's code has arrived.
const CLAIM_EVERY: Duration = Duration::from_secs(2);

/// The `credentials` kind a connection's sign-in is stored under.
const KIND: &str = "connection";

/// One service's MCP server, connected or being connected.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Connection {
    /// Short name: the prefix of its tool names and what the chat calls it.
    pub id: String,
    pub name: String,
    pub about: String,
    pub url: String,
    /// Its registry name, when it came from the registry.
    pub registry: Option<String>,
    pub icon: Option<String>,
}

impl Connection {
    /// A registry server, under a short name no other connection holds.
    pub fn listed(listing: super::connection_registry::Listing, taken: &[Connection]) -> Self {
        let id = short_name(&listing.title, &listing.url, taken);
        Connection {
            id,
            name: listing.title,
            about: listing.about,
            url: listing.url,
            registry: Some(listing.registry),
            icon: listing.icon,
        }
    }

    /// A server the owner gave by address, named for its host.
    pub fn pasted(url: &str, taken: &[Connection]) -> Result<Self> {
        let Ok(parsed) = url::Url::parse(url.trim()) else { bail!("That is not a web address.") };
        let host = parsed.host_str().filter(|_| parsed.scheme() == "https");
        let Some(host) = host else { bail!("A server address starts with https://.") };
        let name = host
            .split('.')
            .find(|w| !matches!(*w, "mcp" | "api" | "www" | "app" | "server"))
            .unwrap_or(host)
            .to_string();
        let url = parsed.to_string();
        Ok(Connection {
            id: short_name(&name, &url, taken),
            name: host.to_string(),
            about: String::new(),
            url,
            registry: None,
            icon: None,
        })
    }
}

/// `title` as a tool-name prefix: lowercase letters, digits and dashes. A
/// connection to the same address keeps its name; another server with the
/// same title gets a number.
fn short_name(title: &str, url: &str, taken: &[Connection]) -> String {
    if let Some(same) = taken.iter().find(|c| c.url == url) {
        return same.id.clone();
    }
    let lower = title.to_lowercase();
    let mut base = String::new();
    for word in lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty() && !matches!(*w, "mcp" | "server" | "official" | "the"))
    {
        if base.len() + word.len() + 1 > 20 {
            if base.is_empty() {
                base = word.chars().take(20).collect();
            }
            break;
        }
        if !base.is_empty() {
            base.push('-');
        }
        base.push_str(word);
    }
    let base = if base.is_empty() { "service" } else { base.as_str() };
    (1..)
        .map(|n| if n == 1 { base.to_string() } else { format!("{base}-{n}") })
        .find(|id| !taken.iter().any(|c| &c.id == id))
        .unwrap_or_default()
}

/// A sign-in that has sent the owner to the service and waits for the code.
const SIGN_IN_WINDOW: Duration = Duration::from_secs(10 * 60);

/// The worker's ceilings: one task in one service.
const MAX_STEPS: u32 = 20;
const MAX_COST_MICROS: i64 = 250_000;
const MAX_WALL_CLOCK: Duration = Duration::from_secs(5 * 60);

const PROMPT: &str = "You act in one service the owner connected, using only its tools. Do the task you are given and nothing more.

Read before you change anything: find the exact item the task names. If two items could match, stop and say which ones.

Tools the service labels as deleting or overwriting are held back until the owner agrees, and the task lists them. If the task needs one, stop, name the tool, and say exactly what you would change with it.

When you finish, answer in a few sentences: what you did or found, with each item's identifier and link as the service returned them.";

// ── Stored tokens ───────────────────────────────────────────────────────────

fn credential_id(connection: &Connection) -> String {
    format!("{KIND}:{}", connection.id)
}

/// rmcp's credential store over one sealed `credentials` row.
struct Stored {
    pool: PgPool,
    connection: Connection,
}

fn store_error(e: impl std::fmt::Display) -> AuthError {
    AuthError::CredentialStoreError(e.to_string())
}

#[async_trait::async_trait]
impl CredentialStore for Stored {
    async fn load(&self) -> Result<Option<StoredCredentials>, AuthError> {
        let sealed: Option<String> = sqlx::query_scalar(
            "SELECT secrets_ciphertext FROM credentials WHERE id = $1 AND kind = $2 AND status = 'active'",
        )
        .bind(credential_id(&self.connection))
        .bind(KIND)
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
             VALUES ($1, $2, $3, $4, 'active', $5, $6)
             ON CONFLICT (id) DO UPDATE
               SET secrets_ciphertext = EXCLUDED.secrets_ciphertext, status = 'active',
                   name = EXCLUDED.name, metadata = EXCLUDED.metadata, updated_at = now()",
        )
        .bind(credential_id(&self.connection))
        .bind(KIND)
        .bind(&self.connection.id)
        .bind(&self.connection.name)
        .bind(&sealed)
        .bind(json!({
            "url": self.connection.url,
            "about": self.connection.about,
            "registry": self.connection.registry,
            "icon": self.connection.icon,
        }))
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(())
    }

    async fn clear(&self) -> Result<(), AuthError> {
        disconnect(&self.pool, &self.connection.id).await.map_err(store_error)
    }
}

/// Every connection with a live sign-in, by name.
pub async fn connected(pool: &PgPool) -> Result<Vec<Connection>, sqlx::Error> {
    let rows: Vec<(String, String, Value)> = sqlx::query_as(
        "SELECT key, name, metadata FROM credentials WHERE kind = $1 AND status = 'active' ORDER BY name",
    )
    .bind(KIND)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, name, metadata)| {
            let text = |key: &str| metadata.get(key).and_then(Value::as_str).map(str::to_string);
            Some(Connection {
                url: text("url")?,
                about: text("about").unwrap_or_default(),
                registry: text("registry"),
                icon: text("icon"),
                id,
                name,
            })
        })
        .collect())
}

/// The connected service with this short name.
async fn load(pool: &PgPool, id: &str) -> Result<Option<Connection>> {
    Ok(connected(pool).await?.into_iter().find(|c| c.id == id))
}

/// Forgets the sign-in. The service keeps its own record of the grant until
/// the owner removes it there.
pub async fn disconnect(pool: &PgPool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM credentials WHERE kind = $1 AND key = $2")
        .bind(KIND)
        .bind(id)
        .execute(pool)
        .await?;
    forget_session(id).await;
    Ok(())
}

// ── Signing in ──────────────────────────────────────────────────────────────

struct Pending {
    connection: Connection,
    state: OAuthState,
    started: Instant,
}

fn pending() -> &'static Mutex<HashMap<String, Pending>> {
    static PENDING: OnceLock<Mutex<HashMap<String, Pending>>> = OnceLock::new();
    PENDING.get_or_init(Default::default)
}

/// Starts a sign-in and returns the service's authorization URL for the
/// owner's browser. A task then waits for the code at the proxy and finishes
/// the sign-in when it arrives. `sheet` is set when the owner's app opens the
/// sign-in in its sign-in sheet.
pub async fn start(pool: &PgPool, connection: Connection, sheet: bool) -> Result<String> {
    let mut state = OAuthState::new(&connection.url, None).await.context("reach the service")?;
    if let OAuthState::Unauthorized(manager) = &mut state {
        manager.set_credential_store(Stored { pool: pool.clone(), connection: connection.clone() });
    }
    let request = AuthorizationRequest::new(&redirect_uri(sheet))
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
    waiting.insert(csrf.clone(), Pending { connection, state, started: Instant::now() });
    drop(waiting);
    tokio::spawn(claim(csrf));
    Ok(url)
}

/// What the proxy hands back for a sign-in.
#[derive(serde::Deserialize)]
struct Returned {
    code: Option<String>,
    iss: Option<String>,
    error: Option<String>,
}

/// Asks the proxy for the sign-in's code until it arrives, the sign-in is
/// gone, or its ten minutes pass, then finishes it. Nothing waits on this
/// task; the Sources catalog shows the connection once it is stored.
async fn claim(csrf: String) {
    let url = format!(
        "{}/connections/claim/{}",
        virtues_helpers::auth::proxy_url(),
        urlencoding::encode(&csrf)
    );
    let started = Instant::now();
    while started.elapsed() < SIGN_IN_WINDOW {
        tokio::time::sleep(CLAIM_EVERY).await;
        if !pending().lock().await.contains_key(&csrf) {
            return;
        }
        let response = match http().post(&url).send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::debug!(error = %e, "connection sign-in: proxy unreachable, asking again");
                continue;
            }
        };
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            continue;
        }
        if !response.status().is_success() {
            tracing::warn!(status = %response.status(), "connection sign-in: the proxy refused the claim");
            continue;
        }
        let returned: Returned = match response.json().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "connection sign-in: the proxy's answer did not parse");
                continue;
            }
        };
        match (returned.code, returned.error) {
            (Some(code), None) => match finish(&code, &csrf, returned.iss.as_deref()).await {
                Ok(c) => tracing::info!(connection = %c.id, "connection signed in"),
                Err(e) => tracing::warn!(error = %format!("{e:#}"), "connection sign-in did not finish"),
            },
            (_, error) => {
                pending().lock().await.remove(&csrf);
                tracing::warn!(error = ?error, "connection sign-in refused by the service");
            }
        }
        return;
    }
}

fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_default()
    })
}

/// Finishes a sign-in with the code the service sent. Returns the connection.
async fn finish(code: &str, csrf: &str, issuer: Option<&str>) -> Result<Connection> {
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
    forget_session(&sign_in.connection.id).await;
    Ok(sign_in.connection)
}

// ── The client ──────────────────────────────────────────────────────────────

type Client = RunningService<RoleClient, ClientConfig>;

fn clients() -> &'static Mutex<HashMap<String, Arc<Client>>> {
    static CLIENTS: OnceLock<Mutex<HashMap<String, Arc<Client>>>> = OnceLock::new();
    CLIENTS.get_or_init(Default::default)
}

/// Each connection's tool list, read once per session: the worker's
/// definitions, the destructive check on each call and the receipts all read
/// it. Dropped with the session.
fn tool_lists() -> &'static Mutex<HashMap<String, Arc<Vec<Tool>>>> {
    static LISTS: OnceLock<Mutex<HashMap<String, Arc<Vec<Tool>>>>> = OnceLock::new();
    LISTS.get_or_init(Default::default)
}

/// Drops the cached session and tool list, so the next call opens fresh.
async fn forget_session(id: &str) {
    clients().lock().await.remove(id);
    tool_lists().lock().await.remove(id);
}

async fn tools(client: &Client, connection: &Connection) -> Result<Arc<Vec<Tool>>> {
    if let Some(tools) = tool_lists().lock().await.get(&connection.id) {
        return Ok(tools.clone());
    }
    let tools = Arc::new(client.list_all_tools().await.context("list the service's tools")?);
    tool_lists().lock().await.insert(connection.id.clone(), tools.clone());
    Ok(tools)
}

/// A live MCP session with `connection`, signed in from its stored tokens.
async fn client(pool: &PgPool, connection: &Connection) -> Result<Arc<Client>> {
    if let Some(client) = clients().lock().await.get(&connection.id) {
        if !client.is_transport_closed() {
            return Ok(client.clone());
        }
    }
    tool_lists().lock().await.remove(&connection.id);
    let mut manager = AuthorizationManager::new(&connection.url).await.context("reach the service")?;
    manager.set_credential_store(Stored { pool: pool.clone(), connection: connection.clone() });
    if !manager.initialize_from_store().await.context("read the sign-in")? {
        bail!("{} is not connected. Connect it in Settings, then try again.", connection.name);
    }
    let transport = StreamableHttpClientTransport::with_client(
        AuthClient::new(reqwest_rmcp::Client::default(), manager),
        StreamableHttpClientTransportConfig::with_uri(connection.url.clone()),
    );
    let client = Arc::new(
        ClientConfig::default()
            .serve(transport)
            .await
            .with_context(|| format!("open a session with {}", connection.name))?,
    );
    clients().lock().await.insert(connection.id.clone(), client.clone());
    Ok(client)
}

/// The tool name the worker sees for one of a connection's tools.
fn prefixed(connection: &Connection, tool: &str) -> String {
    format!("{}__{tool}", connection.id)
}

/// Splits a prefixed tool name into its connection's short name and the
/// service's name for the tool.
pub fn split(name: &str) -> Option<(&str, &str)> {
    let (id, tool) = name.split_once("__")?;
    let short = !id.is_empty() && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    (short && !tool.is_empty()).then_some((id, tool))
}

// What a tool does comes only from the labels its server publishes (MCP tool
// annotations). The box never guesses from a tool's name or description.

/// The server labels the tool as destroying or overwriting. Only an explicit
/// `true` counts: the owner decided assistant actions do not ask, and deletes
/// do.
fn destructive(tool: &Tool) -> bool {
    tool.annotations.as_ref().and_then(|a| a.destructive_hint) == Some(true)
}

/// The server labels the tool as only reading. Every other call is a write,
/// and gets a receipt.
fn read_only(tool: &Tool) -> bool {
    tool.annotations.as_ref().and_then(|a| a.read_only_hint) == Some(true)
}

/// The connection's tools as LLM definitions, leaving out the destructive
/// ones unless the owner confirmed.
fn tool_definitions(tools: &[Tool], connection: &Connection, confirmed: bool) -> Vec<Value> {
    tools
        .iter()
        .filter(|t| confirmed || !destructive(t))
        .map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": prefixed(connection, &t.name),
                    "description": t.description.as_deref().unwrap_or(""),
                    "parameters": Value::Object(optional_nullable(&t.input_schema)),
                }
            })
        })
        .collect()
}

/// `schema` with each optional top-level property also taking `null`. Some
/// models fill every property they are shown; given `null` they leave an
/// unused one empty instead of inventing a value, and [`without_unset`]
/// drops it before the service sees it.
fn optional_nullable(schema: &serde_json::Map<String, Value>) -> serde_json::Map<String, Value> {
    let mut schema = schema.clone();
    let required: Vec<String> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|r| r.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default();
    let Some(Value::Object(properties)) = schema.get_mut("properties") else { return schema };
    for (name, property) in properties.iter_mut() {
        if required.contains(name) {
            continue;
        }
        let Value::Object(property) = property else { continue };
        match property.get("type").cloned() {
            Some(Value::String(t)) if t != "null" => {
                property.insert("type".into(), json!([t, "null"]));
            }
            Some(Value::Array(mut types)) if !types.contains(&json!("null")) => {
                types.push(json!("null"));
                property.insert("type".into(), Value::Array(types));
            }
            _ => {}
        }
        if let Some(Value::Array(values)) = property.get_mut("enum") {
            if !values.contains(&Value::Null) {
                values.push(Value::Null);
            }
        }
    }
    schema
}

/// `arguments` without the optional properties the worker left unset: `null`,
/// or an empty string where the schema does not require the property.
fn without_unset(tool: &Tool, mut arguments: serde_json::Map<String, Value>) -> serde_json::Map<String, Value> {
    let required: Vec<&str> = tool
        .input_schema
        .get("required")
        .and_then(Value::as_array)
        .map(|r| r.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    arguments.retain(|name, value| {
        required.contains(&name.as_str()) || !(value.is_null() || value.as_str() == Some(""))
    });
    arguments
}

/// A write the worker made in a connected service, for the chat's receipt.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Receipt {
    pub service: String,
    /// What was done: the tool's title as its server gives it, or its name
    /// as a sentence.
    pub action: String,
    /// The item it touched, such as `VIR-412 Fix the sync`, when the
    /// service's answer names one.
    pub item: Option<String>,
    pub link: Option<String>,
}

fn receipt(connection: &Connection, tool: &Tool, result: &Value) -> Receipt {
    let action = tool
        .title
        .clone()
        .or_else(|| tool.annotations.as_ref().and_then(|a| a.title.clone()))
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| as_sentence(&tool.name));
    // The service's answer: structured content, or JSON in its text.
    let body = result
        .get("structured")
        .filter(|v| v.is_object())
        .cloned()
        .or_else(|| result.get("content").and_then(Value::as_str).and_then(|t| serde_json::from_str(t).ok()));
    let (item, link) = body.as_ref().map(item_and_link).unwrap_or_default();
    Receipt { service: connection.name.clone(), action, item, link }
}

/// `save_comment` as `Save comment`.
fn as_sentence(name: &str) -> String {
    let words = name.replace(['_', '-'], " ");
    let mut chars = words.trim().chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// The item a service's answer names, and its link. Only the answer itself
/// and the objects in it that carry their own `identifier` (a comment's issue,
/// say) count; a nested author's name is not the item. With no identifier or
/// title anywhere, the item is what was written: the answer's `body`.
fn item_and_link(body: &Value) -> (Option<String>, Option<String>) {
    let Some(top) = body.as_object() else { return (None, None) };
    let text = |o: &serde_json::Map<String, Value>, key: &str| {
        o.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
    };
    let objects: Vec<&serde_json::Map<String, Value>> = std::iter::once(top)
        .chain(top.values().filter_map(Value::as_object).filter(|o| text(o, "identifier").is_some()))
        .collect();
    let link = objects.iter().find_map(|o| text(o, "url").filter(|u| u.starts_with("https://")));
    let named = objects.iter().find_map(|o| {
        let name = text(o, "title").or_else(|| text(o, "name"));
        match (text(o, "identifier"), name) {
            (Some(id), Some(name)) => Some(format!("{id} {name}")),
            (id, name) => id.or(name),
        }
    });
    let item = named.or_else(|| text(top, "body").map(|b| clip(&b, 280)));
    (item, link)
}

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max).collect();
    format!("{}…", cut.trim_end())
}

/// Runs one of a connection's tools, for the worker. A destructive tool runs
/// only when the owner confirmed in chat.
pub async fn call_tool(pool: &PgPool, name: &str, arguments: Value, context: &ToolContext) -> ToolResult {
    let Some((id, tool)) = split(name) else {
        return ToolResult::error(format!("{name} is not a connected service's tool."));
    };
    let connection = match load(pool, id).await {
        Ok(Some(c)) => c,
        Ok(None) => return ToolResult::error(format!("{id} is not connected.")),
        Err(e) => return ToolResult::error(format!("{e:#}")),
    };
    let connection = &connection;
    let client = match client(pool, connection).await {
        Ok(client) => client,
        Err(e) => return ToolResult::error(format!("{e:#}")),
    };
    let listed = match tools(&client, connection).await {
        Ok(listed) => listed,
        Err(e) => return ToolResult::error(format!("{e:#}")),
    };
    let Some(described) = listed.iter().find(|t| t.name == tool) else {
        return ToolResult::error(format!("{} has no tool called {tool}.", connection.name));
    };
    if !context.owner_confirmed {
        if destructive(described) {
            return ToolResult::error(format!(
                "{tool} deletes or overwrites something in {}. Stop and say what it would change; the owner has to agree first.",
                connection.name
            ));
        }
    }
    let arguments = match arguments {
        Value::Object(map) => Some(without_unset(described, map)),
        Value::Null => None,
        other => return ToolResult::error(format!("{tool} takes an object of arguments, not {other}")),
    };
    let mut request = CallToolRequestParams::new(tool.to_string());
    request.arguments = arguments;
    let outcome = client.call_tool(request).await;
    tracing::info!(audit = "connection_tool", connection = %connection.id, tool, ok = outcome.is_ok(), "connection tool call");
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
            forget_session(&connection.id).await;
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

    let connected = match connected(pool).await {
        Ok(c) => c,
        Err(e) => return ToolResult::error(format!("Could not read the connections: {e}")),
    };
    let Some(connection) = connected.iter().find(|c| c.id == name || c.name.to_lowercase() == name) else {
        let list: Vec<String> = connected.iter().map(|c| c.id.clone()).collect();
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
    let listed = match tools(&client, connection).await {
        Ok(listed) => listed,
        Err(e) => return ToolResult::error(format!("{e:#}")),
    };
    let definitions = tool_definitions(&listed, connection, confirmed);
    // Named to the worker and the chat, so a held-back tool reads as waiting
    // for the owner, not as missing.
    let held_back: Vec<String> = if confirmed {
        Vec::new()
    } else {
        listed.iter().filter(|t| destructive(t)).map(|t| t.name.to_string()).collect()
    };

    let mut messages = build_context_for_llm(&[], None, 0, Some(PROMPT), None, None);
    let held = if held_back.is_empty() {
        String::new()
    } else {
        format!("\n\nHeld back until the owner agrees: {}.", held_back.join(", "))
    };
    messages.push(json!({ "role": "user", "content": format!("Service: {}\n\nTask: {task}{held}", connection.name) }));
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
    let mut stream = agent.run(model.clone(), messages, definitions, worker, cancel);

    let mut answer = String::new();
    let mut used: Vec<String> = Vec::new();
    let mut calls: HashMap<String, String> = HashMap::new();
    let mut receipts: Vec<Receipt> = Vec::new();
    let mut usage = DelegatedUsage { model: model.clone(), ..Default::default() };
    while let Some(event) = stream.next().await {
        match event {
            AgentEvent::TextDelta { content } => answer.push_str(&content),
            AgentEvent::ToolCallStart { id, name, .. } => {
                answer.clear();
                if let Some((_, tool)) = split(&name) {
                    used.push(tool.to_string());
                    calls.insert(id, tool.to_string());
                }
            }
            AgentEvent::ToolCallResult { id, result, success: true, .. } => {
                let tool = calls.get(&id).and_then(|name| listed.iter().find(|t| t.name == *name));
                if let Some(tool) = tool.filter(|t| !read_only(t)) {
                    let r = receipt(connection, tool, &result);
                    tracing::info!(
                        audit = "connection_write",
                        connection = %connection.id,
                        tool = %tool.name,
                        item = r.item.as_deref().unwrap_or(""),
                        link = r.link.as_deref().unwrap_or(""),
                        "connection write"
                    );
                    receipts.push(r);
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
        "tools_used": used,
        "receipts": receipts,
        "held_back": held_back,
        "usage": [usage],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listed(title: &str, url: &str) -> super::super::connection_registry::Listing {
        super::super::connection_registry::Listing {
            registry: format!("com.example/{title}"),
            title: title.to_string(),
            about: String::new(),
            url: url.to_string(),
            icon: None,
            website: None,
        }
    }

    #[test]
    fn a_prefixed_name_splits_back_to_its_connection() {
        let linear = Connection::listed(listed("Linear", "https://mcp.linear.app/mcp"), &[]);
        assert_eq!(prefixed(&linear, "create_issue"), "linear__create_issue");
        assert_eq!(split("linear__create_issue"), Some(("linear", "create_issue")));
        assert!(split("Linear__x").is_none());
        assert!(split("linear__").is_none());
        assert!(split("sql_query").is_none());
    }

    #[test]
    fn short_names_are_tool_safe_and_unique() {
        let notion = Connection::listed(listed("Notion", "https://mcp.notion.com/mcp"), &[]);
        assert_eq!(notion.id, "notion");
        let other = Connection::listed(listed("Notion", "https://notion.example.com/mcp"), &[notion.clone()]);
        assert_eq!(other.id, "notion-2");
        let again = Connection::listed(listed("Notion MCP", "https://mcp.notion.com/mcp"), &[notion.clone()]);
        assert_eq!(again.id, "notion", "the same address keeps its name");
        let odd = Connection::listed(listed("Atlassian Rovo MCP Server!", "https://mcp.atlassian.com/v2/mcp"), &[]);
        assert_eq!(odd.id, "atlassian-rovo");
        assert!(split(&prefixed(&odd, "search")).is_some());
    }

    #[test]
    fn a_pasted_address_is_named_for_its_host() {
        let c = Connection::pasted("https://mcp.example.com/mcp", &[]).unwrap();
        assert_eq!((c.id.as_str(), c.name.as_str()), ("example", "mcp.example.com"));
        assert!(Connection::pasted("http://mcp.example.com/mcp", &[]).is_err());
        assert!(Connection::pasted("not an address", &[]).is_err());
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

    fn linear() -> Connection {
        Connection::listed(listed("Linear", "https://mcp.linear.app/mcp"), &[])
    }

    #[test]
    fn only_the_servers_read_only_label_makes_a_read() {
        let schema = Arc::new(serde_json::Map::new());
        let unlabeled = Tool::new("list_issues", "", schema.clone());
        assert!(!read_only(&unlabeled), "an unlabeled tool is a write, whatever its name");
        let read = Tool::new("list_issues", "", schema)
            .with_annotations(rmcp::model::ToolAnnotations::new().read_only(true));
        assert!(read_only(&read));
    }

    #[test]
    fn a_receipt_names_the_item_and_link_the_service_returned() {
        let schema = Arc::new(serde_json::Map::new());
        let save = Tool::new("save_comment", "", schema.clone());
        let answer = json!({ "id": "c1", "url": "https://linear.app/x/issue/VIR-1#comment-c1",
            "issue": { "identifier": "VIR-1", "title": "Fix the sync", "url": "https://linear.app/x/issue/VIR-1" } });
        let r = receipt(&linear(), &save, &json!({ "content": answer.to_string(), "structured": null }));
        assert_eq!(r.action, "Save comment");
        assert_eq!(r.item.as_deref(), Some("VIR-1 Fix the sync"));
        assert_eq!(r.link.as_deref(), Some("https://linear.app/x/issue/VIR-1#comment-c1"));
        assert_eq!(r.service, "Linear");

        // A comment answers with itself: the author's name is not the item,
        // and with no issue named, what was written is.
        let comment = json!({ "id": "c2", "body": "Looks good to ship.", "author": { "id": "u1", "name": "Nick" } });
        let r = receipt(&linear(), &save, &json!({ "content": comment.to_string(), "structured": null }));
        assert_eq!((r.item.as_deref(), r.link), (Some("Looks good to ship."), None));

        let titled = Tool::new("create_page", "", schema).with_title("Create a page");
        let plain = receipt(&linear(), &titled, &json!({ "content": "Done.", "structured": null }));
        assert_eq!((plain.action.as_str(), plain.item, plain.link), ("Create a page", None, None));
    }

    #[test]
    fn unused_optional_arguments_are_dropped_before_the_service_sees_them() {
        let schema: serde_json::Map<String, Value> = serde_json::from_value(json!({
            "type": "object",
            "required": ["body"],
            "properties": {
                "body": { "type": "string" },
                "issueId": { "type": "string" },
                "kind": { "type": "string", "enum": ["project", "initiative"] }
            }
        }))
        .unwrap();
        let shown = optional_nullable(&schema);
        assert_eq!(shown["properties"]["body"]["type"], json!("string"));
        assert_eq!(shown["properties"]["issueId"]["type"], json!(["string", "null"]));
        assert_eq!(shown["properties"]["kind"]["enum"], json!(["project", "initiative", null]));

        let tool = Tool::new("save_comment", "", Arc::new(schema));
        let sent: serde_json::Map<String, Value> =
            serde_json::from_value(json!({ "body": "", "issueId": "i1", "kind": null, "projectId": "" })).unwrap();
        let kept = without_unset(&tool, sent);
        assert_eq!(Value::Object(kept), json!({ "body": "", "issueId": "i1" }));
    }

    /// A connection is listed as one and never as a data source, which
    /// would count toward setup and the sources list.
    #[sqlx::test(migrations = "./migrations")]
    async fn a_connection_is_not_a_source(pool: PgPool) {
        sqlx::query(
            "INSERT INTO credentials (id, kind, key, name, status, secrets_ciphertext, metadata)
             VALUES ('connection:notion', 'connection', 'notion', 'Notion', 'active', 'sealed', $1)",
        )
        .bind(json!({ "url": "https://mcp.notion.com/mcp", "registry": "com.notion/mcp" }))
        .execute(&pool)
        .await
        .unwrap();
        let listed = connected(&pool).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!((listed[0].id.as_str(), listed[0].url.as_str()), ("notion", "https://mcp.notion.com/mcp"));
        assert_eq!(listed[0].registry.as_deref(), Some("com.notion/mcp"));
        let sources = crate::api::credentials::list_credentials(&pool).await.unwrap();
        assert!(sources.iter().all(|c| c.id != "connection:notion"));

        disconnect(&pool, "notion").await.unwrap();
        assert!(connected(&pool).await.unwrap().is_empty());
    }

    /// The chat is told which services are connected when it names another.
    #[sqlx::test(migrations = "./migrations")]
    async fn an_unconnected_service_is_refused_with_the_list(pool: PgPool) {
        let result = run_tool(&pool, &json!({ "name": "linear", "task": "list my issues" }), &ToolContext::default()).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("No services are connected"));
    }
}

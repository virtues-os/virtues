//! `/api/mcp`: the box as an MCP server, for an agent the owner runs on their
//! own machine (Claude Code, Claude Desktop, Cursor).
//!
//! Streamable HTTP through rmcp, served statelessly: every request is answered
//! on its own and nothing about a client is kept between calls. rmcp
//! negotiates the protocol version, so clients on the 2026-07-28 spec and
//! clients still on the older handshake both work.
//!
//! The tools are registry rows run through the same `ToolExecutor` as chat and
//! the CLI, allowed by `tools::mcp_allows`. The route sits in
//! `protected_routes`, behind the blanket `AuthUser` layer; its own layer
//! below also puts the caller into the request, so each call's audit line
//! names the device it came from. The plan is agents/plan/mcp-plan.md.

use std::sync::Arc;

use axum::{
    extract::Request,
    http::request::Parts,
    middleware::{self, Next},
    response::Response,
    Router,
};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation, JsonObject,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::{
    session::never::NeverSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use sqlx::PgPool;

use crate::middleware::auth::AuthUser;
use crate::server::yjs::YjsState;
use crate::server::AppState;
use crate::tools::{applet_management, applet_setup, mcp_allows, tools_for_mcp, ToolContext, ToolExecutor};

pub const PATH: &str = "/api/mcp";

/// What a client is told about the server when it connects.
const INSTRUCTIONS: &str = "This is the owner's Virtues server: their own record of their life \
(messages, calendar, health, places, pages and more) in Postgres. Before writing SQL, call \
sql_query with operation \"list_tables\", then \"get_schema\" for the tables you need. \
Pages and applets are the owner's; change them only when asked.";

pub fn routes(state: &AppState) -> Router<AppState> {
    let pool = state.db.pool().clone();
    let yjs = state.yjs_state.clone();
    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        // Callers reach the box under several names (the Mac's loopback
        // proxy, the LAN name, a stdio bridge), and the iroh key is what
        // authenticates them, so a Host check adds nothing. DNS rebinding is
        // driven from a browser, and a browser sends an Origin with every
        // POST: refusing any Origin keeps web pages off this route.
        .disable_allowed_hosts()
        .enforce_origin_validation();
    let service = StreamableHttpService::new(
        move || Ok(BoxServer { pool: pool.clone(), yjs: yjs.clone() }),
        Arc::new(NeverSessionManager::default()),
        config,
    );
    Router::new()
        .route_service(PATH, service)
        .route_layer(middleware::from_fn_with_state(state.clone(), stamp_caller))
}

/// Puts the authenticated caller into the request's extensions, where rmcp
/// passes them to the handler inside the request's `Parts`.
async fn stamp_caller(user: AuthUser, mut request: Request, next: Next) -> Response {
    request.extensions_mut().insert(user);
    next.run(request).await
}

#[derive(Clone)]
struct BoxServer {
    pool: PgPool,
    yjs: YjsState,
}

impl ServerHandler for BoxServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("virtues", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(listed_tools()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let parts = context.extensions.get::<Parts>();
        let device = parts
            .and_then(|parts| parts.extensions.get::<AuthUser>())
            .map(|user| user.device_id.clone())
            .unwrap_or_default();
        // The agent key a console call came in on (`virtues mcp` behind
        // `agent-exec`), for the audit line only, as in `console.rs`.
        let agent_key = parts
            .and_then(|parts| parts.headers.get(super::console::AGENT_KEY_HEADER))
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let arguments = serde_json::Value::Object(request.arguments.unwrap_or_default());
        let result = call(&self.pool, &self.yjs, &request.name, arguments, &device, agent_key).await?;
        Ok(result.into())
    }
}

/// The registry's MCP tools as MCP tool definitions. The description is the
/// one the chat model reads.
fn listed_tools() -> Vec<Tool> {
    tools_for_mcp()
        .into_iter()
        .map(|(tool, reads_only)| {
            let schema = match tool.parameters {
                serde_json::Value::Object(schema) => schema,
                _ => JsonObject::new(),
            };
            Tool::new(tool.id, tool.llm_description, Arc::new(schema))
                .with_title(tool.name)
                .with_annotations(ToolAnnotations::new().read_only(reads_only))
        })
        .collect()
}

/// Runs one tool call and logs its audit line. A tool `/api/mcp` does not
/// offer is a protocol error; a tool that runs and fails is a result with
/// `isError`, which the calling model reads.
async fn call(
    pool: &PgPool,
    yjs: &YjsState,
    tool: &str,
    mut arguments: serde_json::Value,
    device: &str,
    agent_key: &str,
) -> Result<CallToolResult, McpError> {
    if !mcp_allows(tool, &arguments) {
        return Err(McpError::invalid_params(format!("{tool} is not offered over MCP"), None));
    }
    if tool == "applet" && applet_management::action(&arguments) == Some("setup") {
        applet_setup::with_default_spend_cap(&mut arguments);
    }
    let args_sha = super::console::args_sha(&arguments);
    let exec = ToolExecutor::new_with_yjs(pool.clone(), yjs.clone());
    let context = ToolContext { console: true, ..Default::default() };
    let result = match exec.execute(tool, arguments, &context).await {
        Ok(outcome) if outcome.success => {
            let mut result = CallToolResult::success(vec![ContentBlock::text(outcome.data.to_string())]);
            if outcome.data.is_object() {
                result.structured_content = Some(outcome.data);
            }
            result
        }
        Ok(outcome) => {
            let why = outcome.error.unwrap_or_else(|| outcome.data.to_string());
            CallToolResult::error(vec![ContentBlock::text(why)])
        }
        Err(e) => CallToolResult::error(vec![ContentBlock::text(e.to_string())]),
    };
    tracing::info!(
        audit = "mcp_tool",
        tool = %tool,
        device = %device,
        agent_key = %agent_key,
        args_sha = %args_sha,
        is_error = result.is_error.unwrap_or(false),
        "mcp tool call"
    );
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The list is the allowlist: the CLI's verbs and `applet`, with the
    /// reads marked read-only, and nothing that sends, browses or spends.
    #[test]
    fn lists_the_cli_verbs_and_nothing_that_reaches_out() {
        let tools = listed_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        for expected in ["sql_query", "semantic_search", "get_page_content", "create_page", "edit_page", "applet"] {
            assert!(names.contains(&expected), "{expected} missing from {names:?}");
        }
        for never in ["send_imessage", "browse", "shell", "generate_image", "web_search"] {
            assert!(!names.contains(&never), "{never} must not be listed");
        }
        let read_only = |name: &str| {
            tools.iter().find(|t| t.name == name).and_then(|t| t.annotations.as_ref()).and_then(|a| a.read_only_hint)
        };
        assert_eq!(read_only("sql_query"), Some(true));
        assert_eq!(read_only("create_page"), Some(false));
        assert!(tools.iter().all(|t| !t.input_schema.is_empty()), "every tool has an input schema");
    }

    /// An applet can be listed, read, checked, put and run over MCP, as from
    /// the CLI. Editing and deleting one is not offered.
    #[test]
    fn applet_actions_follow_the_cli() {
        let allows = |action: &str| mcp_allows("applet", &serde_json::json!({ "action": action }));
        assert!(allows("list"));
        assert!(allows("get"));
        assert!(allows("setup"));
        assert!(allows("run"));
        assert!(!allows("edit"));
        assert!(!allows("delete"));
        assert!(!mcp_allows("send_imessage", &serde_json::json!({})));
    }

    /// A call to a tool MCP does not offer is refused before anything runs.
    #[sqlx::test]
    async fn a_tool_not_offered_is_refused(pool: PgPool) {
        let yjs = YjsState::new(pool.clone());
        let refused = call(&pool, &yjs, "shell", serde_json::json!({ "command": "true" }), "test", "").await;
        assert!(refused.is_err());
    }

    /// A read runs through the executor and comes back as text the calling
    /// model can read, with the object as structured content.
    #[sqlx::test]
    async fn a_read_runs_through_the_executor(pool: PgPool) {
        let yjs = YjsState::new(pool.clone());
        let args = serde_json::json!({ "operation": "query", "sql": "SELECT 1 AS one" });
        let result = call(&pool, &yjs, "sql_query", args, "test", "").await.unwrap();
        assert_eq!(result.is_error, Some(false), "{result:?}");
        assert!(result.structured_content.is_some(), "{result:?}");
    }
}

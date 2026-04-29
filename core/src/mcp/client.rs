//! MCP Client Manager
//!
//! Manages connections to external MCP servers via StreamableHTTP transport.
//! Provides lazy reconnection — tool metadata is cached in SQLite, connections
//! are established on-demand when a tool is actually called.

use std::collections::HashMap;
use std::sync::Arc;

use rmcp::model::{CallToolRequestParam, Tool as McpToolDef};
use rmcp::service::{RoleClient, RunningService, RunningServiceCancellationToken};
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::Peer;
use rmcp::ServiceExt;
use sqlx::SqlitePool;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::tools::{ToolError, ToolResult};

/// A live MCP connection to an external server
struct McpConnection {
    peer: Peer<RoleClient>,
    cancel: RunningServiceCancellationToken,
}

/// Manages MCP client connections and tool discovery
#[derive(Clone)]
pub struct McpClientManager {
    pool: Arc<SqlitePool>,
    connections: Arc<Mutex<HashMap<String, McpConnection>>>,
}

impl McpClientManager {
    pub fn new(pool: Arc<SqlitePool>) -> Self {
        Self {
            pool,
            connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Connect to an MCP server, discover its tools, and persist them to the database.
    /// Returns the number of tools discovered.
    pub async fn connect(&self, server_id: &str) -> Result<usize, ToolError> {
        // Skip if already connected (prevents race from concurrent connect calls)
        if self.connections.lock().await.contains_key(server_id) {
            info!(server_id, "Already connected, skipping");
            // Return current tool count from DB
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM app_mcp_tools WHERE server_id = $1")
                .bind(server_id)
                .fetch_one(self.pool.as_ref())
                .await
                .unwrap_or(0);
            return Ok(count as usize);
        }

        // Load server config from DB
        let row = sqlx::query_as::<_, (String, String, Option<String>)>(
            "SELECT url, name, auth_token FROM app_mcp_servers WHERE id = $1",
        )
        .bind(server_id)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("DB error: {e}")))?
        .ok_or_else(|| ToolError::ExecutionFailed(format!("MCP server not found: {server_id}")))?;

        let (url, server_name, auth_token) = row;

        // Update status to connecting
        let _ = sqlx::query("UPDATE app_mcp_servers SET status = 'connecting', last_error = NULL WHERE id = $1")
            .bind(server_id)
            .execute(self.pool.as_ref())
            .await;

        // Create transport
        let transport = if let Some(ref token) = auth_token {
            use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
            let config = StreamableHttpClientTransportConfig::with_uri(url.clone()).auth_header(token.clone());
            StreamableHttpClientTransport::from_config(config)
        } else {
            StreamableHttpClientTransport::from_uri(url.clone())
        };

        // Establish connection
        let running_service: RunningService<RoleClient, ()> = ()
            .serve(transport)
            .await
            .map_err(|e| {
                let msg = format!("Failed to connect to MCP server {server_id}: {e}");
                error!("{}", msg);
                // Update DB status
                let pool = self.pool.clone();
                let sid = server_id.to_string();
                let emsg = msg.clone();
                tokio::spawn(async move {
                    let _ = sqlx::query("UPDATE app_mcp_servers SET status = 'error', last_error = $1 WHERE id = $2")
                        .bind(&emsg)
                        .bind(&sid)
                        .execute(pool.as_ref())
                        .await;
                });
                ToolError::ExecutionFailed(msg)
            })?;

        let peer = running_service.peer().clone();
        let cancel = running_service.cancellation_token();

        // Spawn background task to hold the RunningService alive and detect disconnects
        let pool_bg = self.pool.clone();
        let sid_bg = server_id.to_string();
        let conns_bg = self.connections.clone();
        tokio::spawn(async move {
            // This blocks until the service shuts down
            let _ = running_service.waiting().await;
            info!("MCP server {} disconnected", sid_bg);
            // Clean up connection from map
            conns_bg.lock().await.remove(&sid_bg);
            // Update DB status
            let _ = sqlx::query("UPDATE app_mcp_servers SET status = 'disconnected' WHERE id = $1")
                .bind(&sid_bg)
                .execute(pool_bg.as_ref())
                .await;
        });

        // Discover tools — if this fails, cancel the spawned connection to avoid leaks
        let tools = match peer.list_all_tools().await {
            Ok(tools) => tools,
            Err(e) => {
                cancel.cancel();
                let msg = format!("Failed to list tools from {server_id}: {e}");
                let _ = sqlx::query("UPDATE app_mcp_servers SET status = 'error', last_error = $1 WHERE id = $2")
                    .bind(&msg)
                    .bind(server_id)
                    .execute(self.pool.as_ref())
                    .await;
                return Err(ToolError::ExecutionFailed(msg));
            }
        };

        let tool_count = tools.len();

        // Persist tools to database — if this fails, cancel the connection
        if let Err(e) = self.persist_tools(server_id, &server_name, &tools).await {
            cancel.cancel();
            let msg = format!("Failed to persist tools for {server_id}: {e}");
            let _ = sqlx::query("UPDATE app_mcp_servers SET status = 'error', last_error = $1 WHERE id = $2")
                .bind(&msg)
                .bind(server_id)
                .execute(self.pool.as_ref())
                .await;
            return Err(e);
        }

        // Update DB status to connected
        let _ = sqlx::query(
            "UPDATE app_mcp_servers SET status = 'connected', last_connected_at = datetime('now'), last_error = NULL WHERE id = $1",
        )
        .bind(server_id)
        .execute(self.pool.as_ref())
        .await;

        // Store connection
        self.connections
            .lock()
            .await
            .insert(server_id.to_string(), McpConnection { peer, cancel });

        info!(
            server_id,
            tool_count, "Connected to MCP server and discovered tools"
        );

        Ok(tool_count)
    }

    /// Call a tool on a connected MCP server. Lazy-reconnects if needed.
    pub async fn call_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<ToolResult, ToolError> {
        // Get or lazy-reconnect
        let peer = {
            let conns = self.connections.lock().await;
            conns.get(server_id).map(|c| c.peer.clone())
        };

        let peer = match peer {
            Some(p) => p,
            None => {
                info!(server_id, "Lazy-reconnecting to MCP server for tool call");
                self.connect(server_id).await?;
                let conns = self.connections.lock().await;
                conns
                    .get(server_id)
                    .map(|c| c.peer.clone())
                    .ok_or_else(|| {
                        ToolError::ExecutionFailed(format!(
                            "Failed to reconnect to MCP server {server_id}"
                        ))
                    })?
            }
        };

        // Build call params
        let args = arguments.as_object().cloned();
        let params = CallToolRequestParam {
            name: tool_name.to_string().into(),
            arguments: args,
        };

        // Execute the tool call with a timeout (spec: clients SHOULD implement timeouts)
        let call_future = peer.call_tool(params);
        let result = tokio::time::timeout(std::time::Duration::from_secs(120), call_future)
            .await
            .map_err(|_| {
                warn!(server_id, tool_name, "MCP tool call timed out after 120s");
                ToolError::ExecutionFailed(format!(
                    "MCP tool call timed out ({server_id}/{tool_name}): no response after 120 seconds"
                ))
            })?
            .map_err(|e| {
                let msg = format!("{e}");
                // If transport closed, remove stale connection so next call will reconnect
                if msg.contains("closed") || msg.contains("Closed") || msg.contains("transport") {
                    warn!(server_id, "MCP connection appears stale, removing");
                    let conns = self.connections.clone();
                    let sid = server_id.to_string();
                    tokio::spawn(async move {
                        conns.lock().await.remove(&sid);
                    });
                }
                ToolError::ExecutionFailed(format!("MCP tool call failed ({server_id}/{tool_name}): {e}"))
            })?;

        // Convert CallToolResult to our ToolResult
        let is_error = result.is_error.unwrap_or(false);

        // Extract text content from the result
        // Annotated<RawContent> has a `raw` field and Derefs to RawContent
        let text_parts: Vec<String> = result
            .content
            .iter()
            .filter_map(|c| {
                if let rmcp::model::RawContent::Text(t) = &c.raw {
                    Some(t.text.clone())
                } else {
                    None
                }
            })
            .collect();

        let output = text_parts.join("\n");

        if is_error {
            Ok(ToolResult::error(output))
        } else {
            Ok(ToolResult::success(serde_json::json!({ "output": output })))
        }
    }

    /// Disconnect from an MCP server and clean up its tools.
    pub async fn disconnect(&self, server_id: &str) -> Result<(), ToolError> {
        // Remove and cancel connection
        if let Some(conn) = self.connections.lock().await.remove(server_id) {
            conn.cancel.cancel();
        }

        // Delete cached tools from DB
        let _ = sqlx::query("DELETE FROM app_mcp_tools WHERE server_id = $1")
            .bind(server_id)
            .execute(self.pool.as_ref())
            .await;

        // Update status
        let _ = sqlx::query(
            "UPDATE app_mcp_servers SET status = 'disconnected', last_error = NULL WHERE id = $1",
        )
        .bind(server_id)
        .execute(self.pool.as_ref())
        .await;

        info!(server_id, "Disconnected from MCP server");
        Ok(())
    }

    /// Check if a server is currently connected.
    pub async fn is_connected(&self, server_id: &str) -> bool {
        self.connections.lock().await.contains_key(server_id)
    }

    /// Persist discovered tools to the database (replace all for this server).
    /// Uses a transaction so partial failures don't leave inconsistent state.
    async fn persist_tools(
        &self,
        server_id: &str,
        server_name: &str,
        tools: &[McpToolDef],
    ) -> Result<(), ToolError> {
        let mut tx = self.pool.begin().await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to begin transaction: {e}")))?;

        // Delete existing tools for this server
        sqlx::query("DELETE FROM app_mcp_tools WHERE server_id = $1")
            .bind(server_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("DB error deleting tools: {e}")))?;

        // Insert new tools
        for tool in tools {
            let tool_name = tool.name.as_ref();
            let tool_id = format!("{server_id}__{tool_name}");
            let description = tool.description.as_deref().map(|s| s.to_string());
            let input_schema = serde_json::to_string(tool.input_schema.as_ref()).ok();

            sqlx::query(
                "INSERT INTO app_mcp_tools (id, server_id, server_name, tool_name, description, input_schema) VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(&tool_id)
            .bind(server_id)
            .bind(server_name)
            .bind(tool_name)
            .bind(&description)
            .bind(&input_schema)
            .execute(&mut *tx)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("DB error inserting tool: {e}")))?;
        }

        tx.commit().await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to commit tools: {e}")))?;

        Ok(())
    }
}

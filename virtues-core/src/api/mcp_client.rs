//! API functions for MCP server management
//!
//! CRUD operations for external MCP server connections.
//! These are user-configured tool servers (GitHub MCP, Slack MCP, etc.)

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::error::{Error, Result};
use crate::mcp::McpClientManager;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    pub id: String,
    pub name: String,
    pub url: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub status: String,
    pub last_error: Option<String>,
    pub last_connected_at: Option<String>,
    pub tool_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerDetail {
    #[serde(flatten)]
    pub server: McpServer,
    pub tools: Vec<McpTool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub id: String,
    pub tool_name: String,
    pub description: Option<String>,
    pub input_schema: Option<serde_json::Value>,
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateMcpServerRequest {
    pub name: String,
    pub url: String,
    pub description: Option<String>,
    pub auth_token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateMcpServerRequest {
    pub name: Option<String>,
    pub url: Option<String>,
    pub description: Option<String>,
    pub auth_token: Option<String>,
    pub enabled: Option<bool>,
}

// ============================================================================
// API Functions
// ============================================================================

/// List all MCP servers with their tool counts
pub async fn list_mcp_servers(db: &PgPool) -> Result<Vec<McpServer>> {
    let rows = sqlx::query(
        r#"
        SELECT s.id, s.name, s.url, s.description, s.enabled, s.status,
               s.last_error, s.last_connected_at, s.created_at, s.updated_at,
               COUNT(t.id) as tool_count
        FROM app_mcp_servers s
        LEFT JOIN app_mcp_tools t ON t.server_id = s.id
        GROUP BY s.id
        ORDER BY s.created_at
        "#,
    )
    .fetch_all(db)
    .await
    .unwrap_or_default();

    let servers = rows
        .iter()
        .map(|row| {
            use sqlx::Row;
            McpServer {
                id: row.get("id"),
                name: row.get("name"),
                url: row.get("url"),
                description: row.get("description"),
                enabled: row.get("enabled"),
                status: row.get("status"),
                last_error: row.get("last_error"),
                last_connected_at: row.get("last_connected_at"),
                tool_count: row.get("tool_count"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            }
        })
        .collect();

    Ok(servers)
}

/// Get a single MCP server with its tools
pub async fn get_mcp_server(db: &PgPool, id: &str) -> Result<McpServerDetail> {
    let row = sqlx::query(
        "SELECT id, name, url, description, enabled, status, last_error, last_connected_at, created_at, updated_at FROM app_mcp_servers WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch MCP server: {e}")))?
    .ok_or_else(|| Error::NotFound(format!("MCP server not found: {id}")))?;

    use sqlx::Row;
    let server = McpServer {
        id: row.get("id"),
        name: row.get("name"),
        url: row.get("url"),
        description: row.get("description"),
        enabled: row.get("enabled"),
        status: row.get("status"),
        last_error: row.get("last_error"),
        last_connected_at: row.get("last_connected_at"),
        tool_count: 0, // filled below
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    };

    let tool_rows = sqlx::query(
        "SELECT id, tool_name, description, input_schema, enabled FROM app_mcp_tools WHERE server_id = $1 ORDER BY tool_name",
    )
    .bind(id)
    .fetch_all(db)
    .await
    .unwrap_or_default();

    let tools: Vec<McpTool> = tool_rows
        .iter()
        .map(|r| McpTool {
            id: r.get("id"),
            tool_name: r.get("tool_name"),
            description: r.get("description"),
            input_schema: r
                .get::<Option<String>, _>("input_schema")
                .and_then(|s| serde_json::from_str(&s).ok()),
            enabled: r.get("enabled"),
        })
        .collect();

    let mut server = server;
    server.tool_count = tools.len() as i64;

    Ok(McpServerDetail { server, tools })
}

/// Create a new MCP server and auto-connect
pub async fn create_mcp_server(
    db: &PgPool,
    manager: &McpClientManager,
    req: CreateMcpServerRequest,
) -> Result<McpServerDetail> {
    // Generate server ID from name (lowercase, alphanumeric + underscore only)
    let id = slugify_server_id(&req.name);

    // Validate server ID format
    if !id.chars().next().map(|c| c.is_ascii_lowercase()).unwrap_or(false) {
        return Err(Error::InvalidInput("Server name must start with a letter".to_string()));
    }
    if id.contains("__") {
        return Err(Error::InvalidInput("Server name cannot contain consecutive underscores".to_string()));
    }
    // Cap at 20 chars so LLM tool names ({id}__{tool}) fit within 64-char limit
    if id.len() > 20 {
        return Err(Error::InvalidInput("Server name is too long (max ~20 characters after slugifying)".to_string()));
    }

    // Check for duplicate
    let exists: bool = sqlx::query_scalar("SELECT COUNT(*) > 0 FROM app_mcp_servers WHERE id = $1")
        .bind(&id)
        .fetch_one(db)
        .await
        .unwrap_or(false);

    if exists {
        return Err(Error::InvalidInput(format!("MCP server already exists: {id}")));
    }

    // Insert
    sqlx::query(
        "INSERT INTO app_mcp_servers (id, name, url, description, auth_token) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&id)
    .bind(&req.name)
    .bind(&req.url)
    .bind(&req.description)
    .bind(&req.auth_token)
    .execute(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to create MCP server: {e}")))?;

    // Auto-connect (don't fail the create if connect fails)
    if let Err(e) = manager.connect(&id).await {
        tracing::warn!(server_id = %id, "Auto-connect failed: {e}");
    }

    get_mcp_server(db, &id).await
}

/// Update an MCP server's configuration
pub async fn update_mcp_server(
    db: &PgPool,
    id: &str,
    req: UpdateMcpServerRequest,
) -> Result<McpServerDetail> {
    // Check exists
    let _ = sqlx::query("SELECT id FROM app_mcp_servers WHERE id = $1")
        .bind(id)
        .fetch_optional(db)
        .await
        .map_err(|e| Error::Database(format!("DB error: {e}")))?
        .ok_or_else(|| Error::NotFound(format!("MCP server not found: {id}")))?;

    if let Some(name) = &req.name {
        sqlx::query("UPDATE app_mcp_servers SET name = $1 WHERE id = $2")
            .bind(name)
            .bind(id)
            .execute(db)
            .await
            .map_err(|e| Error::Database(format!("Failed to update: {e}")))?;
    }
    if let Some(url) = &req.url {
        sqlx::query("UPDATE app_mcp_servers SET url = $1 WHERE id = $2")
            .bind(url)
            .bind(id)
            .execute(db)
            .await
            .map_err(|e| Error::Database(format!("Failed to update: {e}")))?;
    }
    if let Some(description) = &req.description {
        sqlx::query("UPDATE app_mcp_servers SET description = $1 WHERE id = $2")
            .bind(description)
            .bind(id)
            .execute(db)
            .await
            .map_err(|e| Error::Database(format!("Failed to update: {e}")))?;
    }
    if let Some(auth_token) = &req.auth_token {
        sqlx::query("UPDATE app_mcp_servers SET auth_token = $1 WHERE id = $2")
            .bind(auth_token)
            .bind(id)
            .execute(db)
            .await
            .map_err(|e| Error::Database(format!("Failed to update: {e}")))?;
    }
    if let Some(enabled) = req.enabled {
        sqlx::query("UPDATE app_mcp_servers SET enabled = $1 WHERE id = $2")
            .bind(enabled)
            .bind(id)
            .execute(db)
            .await
            .map_err(|e| Error::Database(format!("Failed to update: {e}")))?;
    }

    get_mcp_server(db, id).await
}

/// Delete an MCP server (disconnects first, CASCADE deletes tools)
pub async fn delete_mcp_server(
    db: &PgPool,
    manager: &McpClientManager,
    id: &str,
) -> Result<()> {
    // Disconnect first
    let _ = manager.disconnect(id).await;

    sqlx::query("DELETE FROM app_mcp_servers WHERE id = $1")
        .bind(id)
        .execute(db)
        .await
        .map_err(|e| Error::Database(format!("Failed to delete MCP server: {e}")))?;

    Ok(())
}

/// Connect to an MCP server
pub async fn connect_mcp_server(
    manager: &McpClientManager,
    id: &str,
) -> Result<usize> {
    manager
        .connect(id)
        .await
        .map_err(|e| Error::Other(format!("Failed to connect: {e}")))
}

/// Disconnect from an MCP server
pub async fn disconnect_mcp_server(
    manager: &McpClientManager,
    id: &str,
) -> Result<()> {
    manager
        .disconnect(id)
        .await
        .map_err(|e| Error::Other(format!("Failed to disconnect: {e}")))
}

/// Toggle an MCP tool's enabled state
pub async fn toggle_mcp_tool(db: &PgPool, tool_id: &str) -> Result<bool> {
    let row = sqlx::query("SELECT enabled FROM app_mcp_tools WHERE id = $1")
        .bind(tool_id)
        .fetch_optional(db)
        .await
        .map_err(|e| Error::Database(format!("DB error: {e}")))?
        .ok_or_else(|| Error::NotFound(format!("MCP tool not found: {tool_id}")))?;

    use sqlx::Row;
    let current: bool = row.get("enabled");
    let new_state = !current;

    sqlx::query("UPDATE app_mcp_tools SET enabled = $1 WHERE id = $2")
        .bind(new_state)
        .bind(tool_id)
        .execute(db)
        .await
        .map_err(|e| Error::Database(format!("Failed to toggle tool: {e}")))?;

    Ok(new_state)
}

// ============================================================================
// Helpers
// ============================================================================

/// Convert a server name to a valid ID (lowercase, alphanumeric + single underscores)
fn slugify_server_id(name: &str) -> String {
    let raw: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();

    // Collapse consecutive underscores into one, then trim
    let mut result = String::with_capacity(raw.len());
    let mut prev_underscore = false;
    for c in raw.chars() {
        if c == '_' {
            if !prev_underscore {
                result.push('_');
            }
            prev_underscore = true;
        } else {
            result.push(c);
            prev_underscore = false;
        }
    }

    result.trim_matches('_').to_string()
}

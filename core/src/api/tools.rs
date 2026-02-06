//! API functions for tool management
//!
//! There are two types of tools:
//! - **Built-in tools**: Read from virtues-registry (web_search, sql_query, edit_page)
//! - **MCP tools**: Dynamically discovered from connected MCP servers (stored in app_mcp_tools)
//!
//! This module handles listing/getting tools for the API.
//! Tool execution is handled by the tools module (core/src/tools/).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use sqlx::SqlitePool;

use crate::error::{Error, Result};

/// Tool information returned by API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    /// Detailed description for LLM
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_description: Option<String>,
    pub tool_type: String,
    pub category: Option<String>,
    pub icon: Option<String>,
    pub display_order: Option<i32>,
    pub enabled: bool,
    /// For MCP tools
    pub server_name: Option<String>,
    /// JSON Schema for parameters
    pub input_schema: Option<serde_json::Value>,
}

impl Tool {
    fn from_config(config: virtues_registry::tools::ToolConfig, enabled: bool) -> Self {
        let tool_type = match config.tool_type {
            virtues_registry::tools::ToolType::Builtin => "builtin".to_string(),
            virtues_registry::tools::ToolType::Mcp => "mcp".to_string(),
        };

        let category = match config.category {
            virtues_registry::tools::ToolCategory::Search => "search",
            virtues_registry::tools::ToolCategory::Data => "data",
            virtues_registry::tools::ToolCategory::Edit => "edit",
        };

        Self {
            id: config.id,
            name: config.name,
            description: Some(config.description),
            llm_description: Some(config.llm_description),
            tool_type,
            category: Some(category.to_string()),
            icon: Some(config.icon),
            display_order: Some(config.display_order),
            enabled,
            server_name: None,
            input_schema: Some(config.parameters),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ListToolsQuery {
    pub category: Option<String>,
}

/// Get the enabled tools map from the assistant profile
async fn get_enabled_tools_map(db: &SqlitePool) -> Result<HashMap<String, bool>> {
    let row = sqlx::query!(
        r#"
        SELECT enabled_tools FROM app_assistant_profile
        WHERE id = '00000000-0000-0000-0000-000000000001'
        "#
    )
    .fetch_one(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch assistant profile: {}", e)))?;

    let enabled_tools: HashMap<String, bool> = serde_json::from_str(&row.enabled_tools.unwrap_or_else(|| "{}".to_string()))
        .map_err(|e| Error::Database(format!("Failed to parse enabled_tools JSON: {}", e)))?;

    Ok(enabled_tools)
}

/// List all tools (Built-in + MCP) with enablement state
pub async fn list_tools(db: &SqlitePool, params: ListToolsQuery) -> Result<Vec<Tool>> {
    let enabled_map = get_enabled_tools_map(db).await?;

    // 1. Get built-in tools from registry
    let mut tools: Vec<Tool> = virtues_registry::tools::default_tools()
        .into_iter()
        .map(|config| {
            let enabled = *enabled_map.get(&config.id).unwrap_or(&true);
            Tool::from_config(config, enabled)
        })
        .collect();

    // 2. Get MCP tools from SQLite
    // Note: We use a raw query here to avoid SQLx offline issues until migrations are run
    let mcp_rows = sqlx::query(
        r#"
        SELECT id, server_name, tool_name, description, input_schema, enabled
        FROM app_mcp_tools
        "#
    )
    .fetch_all(db)
    .await
    .unwrap_or_default(); // Fallback if table doesn't exist yet

    for row in mcp_rows {
        use sqlx::Row;
        let id: String = row.get("id");
        let server_name: String = row.get("server_name");
        let tool_name: String = row.get("tool_name");
        let description: Option<String> = row.get("description");
        let input_schema_str: Option<String> = row.get("input_schema");
        let enabled: bool = row.get("enabled");

        let user_enabled = *enabled_map.get(&id).unwrap_or(&true);
        let is_enabled = enabled && user_enabled;

        tools.push(Tool {
            id,
            name: tool_name,
            description: description.clone(),
            llm_description: description, // MCP tools use same description for LLM
            tool_type: "mcp".to_string(),
            category: Some("mcp".to_string()),
            icon: Some("ri:plug-line".to_string()),
            display_order: Some(100),
            enabled: is_enabled,
            server_name: Some(server_name),
            input_schema: input_schema_str.and_then(|s| serde_json::from_str(&s).ok()),
        });
    }

    // Filter by category if specified
    if let Some(category) = params.category {
        tools.retain(|t| t.category.as_ref() == Some(&category));
    }

    // Sort by display_order
    tools.sort_by_key(|t| t.display_order.unwrap_or(999));

    Ok(tools)
}

/// Get a tool by ID (Built-in or MCP)
pub async fn get_tool(db: &SqlitePool, id: String) -> Result<Tool> {
    let enabled_map = get_enabled_tools_map(db).await?;

    // Try built-in first
    if let Some(config) = virtues_registry::tools::default_tools().into_iter().find(|t| t.id == id) {
        let enabled = *enabled_map.get(&id).unwrap_or(&true);
        return Ok(Tool::from_config(config, enabled));
    }

    // Try MCP tool
    let row = sqlx::query(
        r#"
        SELECT id, server_name, tool_name, description, input_schema, enabled
        FROM app_mcp_tools
        WHERE id = $1
        "#
    )
    .bind(&id)
    .fetch_optional(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch MCP tool: {}", e)))?;

    if let Some(row) = row {
        use sqlx::Row;
        let id: String = row.get("id");
        let server_name: String = row.get("server_name");
        let tool_name: String = row.get("tool_name");
        let description: Option<String> = row.get("description");
        let input_schema_str: Option<String> = row.get("input_schema");
        let enabled: bool = row.get("enabled");

        let user_enabled = *enabled_map.get(&id).unwrap_or(&true);
        let is_enabled = enabled && user_enabled;

        return Ok(Tool {
            id,
            name: tool_name,
            description: description.clone(),
            llm_description: description, // MCP tools use same description for LLM
            tool_type: "mcp".to_string(),
            category: Some("mcp".to_string()),
            icon: Some("ri:plug-line".to_string()),
            display_order: Some(100),
            enabled: is_enabled,
            server_name: Some(server_name),
            input_schema: input_schema_str.and_then(|s| serde_json::from_str(&s).ok()),
        });
    }

    Err(Error::NotFound(format!("Tool not found: {}", id)))
}

// Note: Built-in tools cannot be updated. They are read-only from the registry.
// MCP tools can be managed via separate endpoints (to be implemented).

//! Built-in tool registry
//!
//! This module defines BUILT-IN tools that are part of Virtues core.
//! These are executed as native Rust functions.
//!
//! MCP tools (user-connected) are stored in SQLite `app_mcp_tools` table
//! and executed via the MCP protocol.
//!
//! # Tool Types
//!
//! - `builtin` - Native Rust implementation (web_search, query_ontology, semantic_search)
//! - `mcp` - MCP protocol (user-connected servers, stored in SQLite)

use serde::{Deserialize, Serialize};

/// Tool type - distinguishes built-in vs MCP tools
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolType {
    /// Built-in tool - native Rust implementation
    Builtin,
    /// MCP tool - executed via MCP protocol
    Mcp,
}

/// Built-in tool configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolConfig {
    /// Unique tool identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Description of what this tool does
    pub description: String,
    /// Tool type (builtin for registry tools)
    pub tool_type: ToolType,
    /// Category for grouping in UI
    pub category: String,
    /// Iconify icon name
    pub icon: String,
    /// Display order in UI
    pub display_order: i32,
}

/// Get default built-in tool configurations
///
/// These are the core tools that ship with Virtues:
/// - web_search: Search the web using Exa AI
/// - virtues_query_ontology: Execute SQL queries against ontology tables
/// - virtues_semantic_search: Natural language search using embeddings
pub fn default_tools() -> Vec<ToolConfig> {
    vec![
        ToolConfig {
            id: "web_search".to_string(),
            name: "Web Search".to_string(),
            description: "Search the web using Exa AI for recent information, research, and domain knowledge".to_string(),
            tool_type: ToolType::Builtin,
            category: "shared".to_string(),
            icon: "ri:search-line".to_string(),
            display_order: 1,
        },
        ToolConfig {
            id: "virtues_query_ontology".to_string(),
            name: "Query Ontology".to_string(),
            description: "Execute SQL queries against the ontology database tables (supports operations: query, list_tables, get_schema)".to_string(),
            tool_type: ToolType::Builtin,
            category: "shared".to_string(),
            icon: "ri:database-2-line".to_string(),
            display_order: 2,
        },
        ToolConfig {
            id: "virtues_semantic_search".to_string(),
            name: "Semantic Search".to_string(),
            description: "Natural language search across emails, messages, calendar, AI conversations, and documents using embeddings".to_string(),
            tool_type: ToolType::Builtin,
            category: "shared".to_string(),
            icon: "ri:search-eye-line".to_string(),
            display_order: 3,
        },
    ]
}

/// Get default enabled tools configuration (for assistant profile)
pub fn default_enabled_tools() -> serde_json::Value {
    serde_json::json!({
        "web_search": true,
        "virtues_query_ontology": true,
        "virtues_semantic_search": true
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_tools() {
        let tools = default_tools();
        assert!(!tools.is_empty(), "Tools should not be empty");

        // Verify all tools have required fields
        for tool in &tools {
            assert!(!tool.id.is_empty());
            assert!(!tool.name.is_empty());
            assert_eq!(tool.tool_type, ToolType::Builtin, "Registry tools should be builtin type");
        }
    }

    #[test]
    fn test_default_enabled_tools() {
        let enabled = default_enabled_tools();
        assert!(enabled.is_object());
        assert_eq!(enabled.get("web_search"), Some(&serde_json::json!(true)));
    }
}

//! Tool execution module
//!
//! This module provides:
//! - Tool definitions with JSON schemas (from virtues-registry)
//! - Unified tool executor for chat API
//! - Individual tool implementations (web_search, sql_query, edit_page)
//!
//! # Architecture
//!
//! Tools are defined in virtues-registry with:
//! - ID, name, descriptions (for UI and LLM)
//! - JSON Schema for parameters
//! - Category and display metadata
//!
//! Tool execution happens through the ToolExecutor, which:
//! - Validates tool parameters against schema
//! - Routes to appropriate tool implementation
//! - Returns structured results
//!
//! # Available Tools
//!
//! - `web_search`: Search the web using Exa AI
//! - `sql_query`: Read-only SQL queries against user data
//! - `edit_page`: AI-assisted page editing with accept/reject

mod executor;
mod web_search;
mod sql_query;
mod page_editor;

pub use executor::{ToolExecutor, ToolContext, ToolResult, ToolError};
pub use web_search::WebSearchTool;
pub use sql_query::SqlQueryTool;
pub use page_editor::PageEditorTool;

/// Get tool definitions for the LLM (OpenAI/Anthropic format)
///
/// Returns tool definitions in the format expected by LLM APIs,
/// with the detailed `llm_description` as the tool description.
pub fn get_tool_definitions_for_llm() -> Vec<serde_json::Value> {
    virtues_registry::tools::default_tools()
        .into_iter()
        .map(|tool| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": tool.id,
                    "description": tool.llm_description,
                    "parameters": tool.parameters,
                }
            })
        })
        .collect()
}

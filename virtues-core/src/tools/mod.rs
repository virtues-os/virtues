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
//! - `edit_page`: AI-assisted page editing (applied immediately via Yjs)

mod executor;
mod web_search;
pub(crate) mod sql_query;
mod page_editor;
mod semantic_search;
pub mod action_setup;
pub mod action_management;
pub mod dayline_events;

pub use executor::{
    SubagentStatus, SubagentUpdate, ToolContext, ToolError, ToolExecutor, ToolResult,
};
pub use web_search::WebSearchTool;
pub use sql_query::SqlQueryTool;
pub use page_editor::PageEditorTool;
pub use semantic_search::SemanticSearchTool;

/// Get tool definitions for the LLM (OpenAI/Anthropic format)
///
/// Returns tool definitions in the format expected by LLM APIs,
/// with the detailed `llm_description` as the tool description.
pub fn get_tool_definitions_for_llm() -> Vec<serde_json::Value> {
    virtues_registry::tools::default_tools()
        .into_iter()
        .filter(|tool| !tool.is_system)
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

/// Get ALL tool definitions including system tools (for action runners).
pub fn get_all_tool_definitions_for_llm() -> Vec<serde_json::Value> {
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

/// The read-only research tools a Deep Research **subagent** (worker) may use. Explicit allow-list
/// (not a category filter) so workers get pure research capability — no memory/profile writes, and
/// crucially no `dispatch_subagents` (recursion guard).
const SUBAGENT_TOOLS: &[&str] = &[
    "think",
    "web_search",
    "semantic_search",
    "sql_query",
    "code_interpreter",
];

/// Get tool definitions for a Deep Research subagent (worker).
pub fn get_tools_for_subagent() -> Vec<serde_json::Value> {
    virtues_registry::tools::default_tools()
        .into_iter()
        .filter(|tool| SUBAGENT_TOOLS.contains(&tool.id.as_str()))
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

/// Onboarding-only tools: just naming + memory (no search, no data, no edit).
///
/// Prevents the AI from running web searches, SQL queries, or other tools
/// during the onboarding conversation before names are set.
const ONBOARDING_TOOLS: &[&str] = &["think", "update_memory", "set_user_name", "set_assistant_name"];

/// Get tool definitions for onboarding (naming + memory only)
pub fn get_tools_for_onboarding() -> Vec<serde_json::Value> {
    virtues_registry::tools::default_tools()
        .into_iter()
        .filter(|tool| ONBOARDING_TOOLS.contains(&tool.id.as_str()))
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

/// Get tool definitions filtered by agent mode
///
/// Agent modes:
/// - "chat": All tools (smart default; write/act tools confirm before running)
/// - "deep_research": Read-only research tools (search + data) plus `dispatch_subagents`
///   (fan-out) and `create_page` (to write the final report). No other edit/act tools.
pub fn get_tools_for_agent_mode(agent_mode: &str) -> Vec<serde_json::Value> {
    use virtues_registry::tools::ToolCategory;

    match agent_mode {
        "deep_research" => {
            // Read-only research tools (search + data) + create_page for the report artifact.
            // `dispatch_subagents` is a Data-category tool, so it's included automatically.
            virtues_registry::tools::default_tools()
                .into_iter()
                .filter(|tool| {
                    matches!(tool.category, ToolCategory::Search | ToolCategory::Data)
                        || tool.id == "create_page"
                })
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
        _ => {
            // "chat" mode or default: all tools
            get_tool_definitions_for_llm()
        }
    }
}

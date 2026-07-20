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
pub(crate) mod sql_write;
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

/// The explicit allowlist for a headless applet run — the runtime capability
/// table from docs/applet-authoring-plan.md §B, enforced. What an applet's
/// agent may do: think, read (sql_query/semantic_search/web_search), write
/// its own applet_* tables (sql_write), keep notes (update_applet_memory),
/// write pages, compute in the jail, and introspect applets read-only. Its
/// run RESULT posts to the linked chat — that's the delivery verb, not a
/// tool. Everything else — applet management (self-modification), memory/
/// profile/name writes, fan-out, image spend — is absent by construction.
const APPLET_RUN_ALLOWED_TOOLS: &[&str] = &[
    "think",
    "sql_query",
    "sql_write",
    "semantic_search",
    "web_search",
    "update_applet_memory",
    "create_page",
    "get_page_content",
    "edit_page",
    "code_interpreter",
    "list_applets",
    "get_applet",
];

/// Tools for an autonomous **applet** run: the explicit allowlist above.
pub fn get_tools_for_action() -> Vec<serde_json::Value> {
    virtues_registry::tools::default_tools()
        .into_iter()
        .filter(|tool| APPLET_RUN_ALLOWED_TOOLS.contains(&tool.id.as_str()))
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

/// A Council voice reasons from its vantage; it does not investigate or cite. `think` only.
const COUNCIL_VOICE_TOOLS: &[&str] = &["think"];

/// Get tool definitions for a Council voice worker (think-only — voices reason, they don't research).
pub fn get_tools_for_council_voice() -> Vec<serde_json::Value> {
    virtues_registry::tools::default_tools()
        .into_iter()
        .filter(|tool| COUNCIL_VOICE_TOOLS.contains(&tool.id.as_str()))
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

/// The orchestrator's tools in Deep Research mode: the read-only research set, plus the fan-out
/// tool and `create_page` for the report artifact. Explicit allow-list (not a category filter) so
/// genuinely read-write Data-category tools (`update_memory`, `set_user_name`, `set_assistant_name`)
/// can't leak into a mode that's meant to be read-only.
const DEEP_RESEARCH_TOOLS: &[&str] = &[
    "think",
    "web_search",
    "semantic_search",
    "sql_query",
    "code_interpreter",
    "dispatch_subagents",
    "create_page",
];

/// The Council orchestrator's tools: read-only grounding (`semantic_search`, `sql_query`) plus the
/// fan-out tool to convene voices. No `create_page` (Council replies in chat, not a page), no
/// `web_search`/`code_interpreter` (Council is about perspectives, not facts). Explicit allow-list,
/// for the same read-only-safety reason as `DEEP_RESEARCH_TOOLS`.
const COUNCIL_TOOLS: &[&str] = &["think", "semantic_search", "sql_query", "dispatch_subagents"];

/// Get tool definitions filtered by agent mode
///
/// Agent modes:
/// - "chat": All tools (smart default; write/act tools confirm before running)
/// - "deep_research": read-only research tools + `dispatch_subagents` (fan-out) + `create_page`
///   (the report artifact). No other edit/act tools — see `DEEP_RESEARCH_TOOLS`.
/// - "council": read-only grounding + `dispatch_subagents` (fan-out voices). No page — see `COUNCIL_TOOLS`.
pub fn get_tools_for_agent_mode(agent_mode: &str) -> Vec<serde_json::Value> {
    let allowlist = match agent_mode {
        "deep_research" => Some(DEEP_RESEARCH_TOOLS),
        "council" => Some(COUNCIL_TOOLS),
        _ => None,
    };
    match allowlist {
        Some(allowed) => virtues_registry::tools::default_tools()
            .into_iter()
            .filter(|tool| allowed.contains(&tool.id.as_str()))
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
            .collect(),
        None => {
            // "chat" mode or default: all tools
            get_tool_definitions_for_llm()
        }
    }
}

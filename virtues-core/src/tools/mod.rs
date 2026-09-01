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
pub mod applet_schema;
pub mod applet_setup;
pub mod applet_management;
pub mod dayline_events;

pub use executor::{
    SubagentStatus, SubagentUpdate, ToolAttachment, ToolContext, ToolError, ToolExecutor,
    ToolResult,
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
/// table from agents/plan/applet-authoring-plan.md §B, enforced. What an applet's
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
pub fn get_tools_for_applet() -> Vec<serde_json::Value> {
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
        // The narrative interview: a listener, not an agent. No tools at all —
        // it must not read the record mid-confession or claim capabilities.
        "interview" => Some(&[] as &[&str]),
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

#[cfg(test)]
mod slot_model_smoke {
    //! Does the Chat slot's model actually drive OUR tool set?
    //!
    //! The gateway's `tool-use` tag proves nothing about behaviour through its
    //! OpenAI-compatible shim — Gemini 3 advertises it and 400s on parallel
    //! calls. A two-tool toy probe proves little more: the real question is
    //! whether a model picks correctly from ~40 tools with our descriptions.
    //!
    //! Networked and key-gated, so it is `#[ignore]`d and skips cleanly when
    //! `AI_GATEWAY_API_KEY` is unset. Run before promoting any model to a slot:
    //!
    //!     cargo test -p virtues --lib slot_model_smoke -- --ignored --nocapture

    use super::get_tool_definitions_for_llm;
    use serde_json::{json, Value};

    const GATEWAY: &str = "https://ai-gateway.vercel.sh/v1/chat/completions";

    /// Prompts a competent agent should answer with a tool call. Kept
    /// deliberately generic: this asserts the model *reaches for tools at all*
    /// under our real schema list, not that it picks one specific tool — that
    /// would be asserting on model taste and would flake on every model swap.
    const MUST_CALL_A_TOOL: &[&str] = &[
        "Search my notes for anything about the Q3 budget.",
        "What did I do yesterday?",
    ];

    async fn call(client: &reqwest::Client, key: &str, model: &str, prompt: &str) -> Value {
        let body = json!({
            "model": model,
            "max_tokens": 1024,
            "tools": get_tool_definitions_for_llm(),
            "messages": [{ "role": "user", "content": prompt }],
        });
        let resp = client
            .post(GATEWAY)
            .bearer_auth(key)
            .json(&body)
            .send()
            .await
            .expect("gateway request");
        let status = resp.status();
        let payload: Value = resp.json().await.expect("gateway returned non-JSON");
        assert!(
            status.is_success(),
            "{model} rejected our tool set ({status}): {payload}"
        );
        payload
    }

    fn tool_calls(payload: &Value) -> &[Value] {
        payload["choices"][0]["message"]["tool_calls"]
            .as_array()
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Does the Gemini 3 exclusion still hold?
    ///
    /// Gemini 3 is kept out of the chat slots because it 400s on parallel tool
    /// calls through the gateway's OpenAI-compat shim — it wants a
    /// `thought_signature` the gateway never forwards (vercel/ai
    /// #11590/#10344). That exclusion is the last hand-held capability
    /// correction in the system, and the failure mode of a hand-held
    /// correction is that it outlives its reason: Vercel or Google fix the
    /// passthrough, and nobody thinks to re-check for a year.
    ///
    /// So it is a check rather than a comment. It **reports** and never fails:
    /// the exclusion holding is the expected state, and a red build for
    /// "third-party bug still present" is noise. What matters is the day it
    /// starts working — then the Gemini 3 line opens up for the slots.
    #[tokio::test]
    #[ignore = "network + AI_GATEWAY_API_KEY: spends real money on the live gateway"]
    async fn report_whether_the_gemini_3_exclusion_still_holds() {
        let Ok(key) = std::env::var("AI_GATEWAY_API_KEY") else {
            eprintln!("AI_GATEWAY_API_KEY unset — skipping");
            return;
        };
        let _ = rustls::crypto::ring::default_provider().install_default();

        // The Omni slot's model — a Gemini 3, safe there because transcription
        // uses no tools at all.
        let model = virtues_registry::models::default_model_for_slot(
            virtues_registry::models::ModelSlot::Omni,
        );
        let client = reqwest::Client::new();
        let body = json!({
            "model": model,
            "max_tokens": 1024,
            "tools": get_tool_definitions_for_llm(),
            "messages": [{
                "role": "user",
                "content": "Do two separate things: search my notes for \
                            'budget', and separately look up what I did \
                            yesterday. Call both tools now.",
            }],
        });
        let resp = client
            .post(GATEWAY)
            .bearer_auth(&key)
            .json(&body)
            .send()
            .await
            .expect("gateway request");
        let status = resp.status();
        let payload: Value = resp.json().await.unwrap_or(Value::Null);

        eprintln!("\n=== Gemini 3 parallel-tool-call exclusion ===");
        if !status.is_success() {
            eprintln!(
                "  {model} still rejects our tool set ({status}) — exclusion HOLDS.\n  {}",
                payload["error"]["message"].as_str().unwrap_or("(no message)")
            );
            return;
        }
        let n = tool_calls(&payload).len();
        if n >= 2 {
            eprintln!(
                "  {model} emitted {n} parallel tool calls — the exclusion may be \n  \
                 STALE. Re-evaluate the Gemini 3 line for the chat slots, and \n  \
                 update the note in virtues-registry::models."
            );
        } else {
            eprintln!("  {model} accepted the tools but emitted {n} call(s) — exclusion HOLDS.");
        }
    }

    #[tokio::test]
    #[ignore = "network + AI_GATEWAY_API_KEY: spends real money on the live gateway"]
    async fn chat_slot_model_drives_our_real_tool_set() {
        let Ok(key) = std::env::var("AI_GATEWAY_API_KEY") else {
            eprintln!("AI_GATEWAY_API_KEY unset — skipping");
            return;
        };
        let _ = rustls::crypto::ring::default_provider().install_default();

        // Defaults to whatever fills the Chat slot today; set SMOKE_MODEL to
        // drive a CANDIDATE through the same gate before promoting it. The
        // registry tells you to run this first, and "first" means before the
        // id is in the registry at all.
        let model = std::env::var("SMOKE_MODEL").unwrap_or_else(|_| {
            crate::api::model_catalog::model_for_slot(
                virtues_registry::models::ModelSlot::Chat,
            )
        });
        let tools = get_tool_definitions_for_llm();
        assert!(
            tools.len() > 10,
            "expected our full tool set, got {}",
            tools.len()
        );
        eprintln!("model={model}  tools={}", tools.len());

        let client = reqwest::Client::new();

        // 1. It accepts our schemas at all, and reaches for a tool.
        for prompt in MUST_CALL_A_TOOL {
            let payload = call(&client, &key, &model, prompt).await;
            let calls = tool_calls(&payload);
            let names: Vec<&str> = calls
                .iter()
                .filter_map(|c| c["function"]["name"].as_str())
                .collect();
            eprintln!("  {prompt:?} -> {names:?}");
            assert!(
                !calls.is_empty(),
                "{model} answered from memory instead of calling a tool: {prompt:?}"
            );
            // Every call must name a tool we actually shipped, with parseable
            // arguments — a hallucinated name or malformed JSON is a hard fail
            // in the agent loop, not a quality nit.
            for c in calls {
                let name = c["function"]["name"].as_str().unwrap_or_default();
                assert!(
                    tools
                        .iter()
                        .any(|t| t["function"]["name"].as_str() == Some(name)),
                    "{model} invented a tool named {name:?}"
                );
                let args = c["function"]["arguments"].as_str().unwrap_or_default();
                serde_json::from_str::<Value>(args)
                    .unwrap_or_else(|e| panic!("{model} sent unparseable args for {name}: {e}"));
            }
        }

        // 2. Parallel tool calls in one turn — the exact failure that keeps
        //    Gemini 3 out of the slots.
        let payload = call(
            &client,
            &key,
            &model,
            "Do two separate things: search my notes for 'budget', and \
             separately look up what I did yesterday. Call both tools now.",
        )
        .await;
        let calls = tool_calls(&payload);
        eprintln!("  parallel -> {} call(s)", calls.len());
        assert!(
            calls.len() >= 2,
            "{model} could not emit parallel tool calls (got {}) — the agent \
             loop depends on this; see Gemini 3",
            calls.len()
        );
    }
}

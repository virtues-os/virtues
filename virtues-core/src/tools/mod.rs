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
pub(crate) mod send_imessage;
pub(crate) mod connection;
pub(crate) mod connection_registry;
pub(crate) mod sql_query;
pub(crate) mod sql_write;
pub(crate) mod tasks;
pub(crate) mod sudo_shell;
pub(crate) mod sql_sudo;
pub(crate) mod sudo_gate;
mod page_editor;
mod semantic_search;
mod show;
pub mod applet_schema;
pub mod applet_setup;
pub mod applet_management;
pub(crate) mod use_applet;

pub use executor::{
    SubagentStatus, SubagentUpdate, ToolAttachment, ToolContext, ToolError, ToolExecutor, WorkerStep,
    ToolResult,
};
pub use web_search::WebSearchTool;
pub use sql_query::SqlQueryTool;
pub use page_editor::PageEditorTool;
/// The chat's page context says what the page editor's reads say about suggestions.
pub(crate) use page_editor::{fits_one_read, holds_proposal, SUGGESTIONS_NOTE};
pub use semantic_search::SemanticSearchTool;

/// One registry tool as the LLM reads it (OpenAI/Anthropic function format),
/// with the detailed `llm_description` as its description.
pub(crate) fn llm_definition(tool: virtues_registry::tools::ToolConfig) -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": tool.id,
            "description": tool.llm_description,
            "parameters": tool.parameters,
        }
    })
}

/// The named tools, in registry order. An allow-list, not a category filter.
pub(crate) fn tools_named(ids: &[&str]) -> Vec<serde_json::Value> {
    virtues_registry::tools::default_tools()
        .into_iter()
        .filter(|tool| ids.contains(&tool.id.as_str()))
        .map(llm_definition)
        .collect()
}

/// Get tool definitions for the LLM (OpenAI/Anthropic format)
///
/// Returns tool definitions in the format expected by LLM APIs,
/// with the detailed `llm_description` as the tool description.
pub fn get_tool_definitions_for_llm() -> Vec<serde_json::Value> {
    virtues_registry::tools::default_tools()
        .into_iter()
        .filter(|tool| !tool.is_system)
        .filter(|tool| !SUDO_ONLY_TOOLS.contains(&tool.id.as_str()))
        .filter(|tool| !is_browser_tool(&tool.id))
        .map(llm_definition)
        .collect()
}

/// What the Mac app does for the chat: `browse` and `send_imessage`, which the
/// chat is offered only while the app is connected (`ChatMode::tools`), and
/// the browsing agent's own tools.
fn is_browser_tool(id: &str) -> bool {
    id == crate::browse::TOOL || id == send_imessage::TOOL || crate::browser::TOOLS.contains(&id)
}

/// Get ALL tool definitions including system tools (for action runners).
/// Still not the sudo-only ones: "all" has never meant "a root shell".
pub fn get_all_tool_definitions_for_llm() -> Vec<serde_json::Value> {
    virtues_registry::tools::default_tools()
        .into_iter()
        .filter(|tool| !SUDO_ONLY_TOOLS.contains(&tool.id.as_str()))
        // An applet runs with nobody watching; the browser is the owner's.
        .filter(|tool| !is_browser_tool(&tool.id))
        .map(llm_definition)
        .collect()
}

/// Tools only the admin applet lists: the owner's admin shell. Every other
/// tool list is built by filtering these out or by an allowlist that never
/// names them, and the executor refuses them outside a sudo turn as well.
pub(crate) const SUDO_ONLY_TOOLS: &[&str] = &["sudo"];

/// The explicit allowlist for a headless applet run — the runtime capability
/// table from agents/record/applets.md, enforced. What an applet's
/// agent may do: think, read (sql_query/semantic_search/web_search), write
/// its own applet_* tables (sql_write), keep notes (update_applet_memory),
/// write pages, run commands in the sandbox (`shell`), and introspect applets read-only. Its
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
    // The wiki editor's write door. `edit_page` is also here, and an agent
    // handed both will reach for the familiar one — which is how the first
    // real run revised an article through the generic editor and skipped every
    // check this tool exists to enforce. The brief says which to use; the
    // allowlist is what makes it available at all.
    "revise_article",
    "shell",
];

/// The tools an applet's `allowed-tools` may name.
pub(crate) fn applet_run_tool_ids() -> &'static [&'static str] {
    APPLET_RUN_ALLOWED_TOOLS
}

/// Tools for an autonomous **applet** run: the explicit allowlist above, and
/// the applet tool with only its reads (`applet_management::execute` refuses
/// the rest from a run as well). `has_chat` is whether the run has a chat id;
/// without one, sql_query is offered without `save_as`, which the tool
/// refuses when there is no chat to keep the file in.
///
/// `allowed` is the applet's own list (`allowed-tools` in a `SKILL.md`), which
/// narrows the allowlist; the tools in `ALWAYS_ALLOWED_TOOLS` stay. `None`
/// gives the whole allowlist. This is the list the model is offered: the
/// agent loop does not refuse a call to a tool that was not offered.
pub fn get_tools_for_applet(has_chat: bool, allowed: Option<&[String]>) -> Vec<serde_json::Value> {
    let ids: Vec<&str> = match allowed {
        Some(list) => APPLET_RUN_ALLOWED_TOOLS
            .iter()
            .copied()
            .filter(|id| {
                crate::applet_templates::ALWAYS_ALLOWED_TOOLS.contains(id) || list.iter().any(|t| t == id)
            })
            .collect(),
        None => APPLET_RUN_ALLOWED_TOOLS.to_vec(),
    };
    let mut tools = tools_named(&ids);
    if !has_chat {
        if let Some(sql) = tools.iter_mut().find(|t| t["function"]["name"] == "sql_query") {
            *sql = llm_definition(virtues_registry::tools::sql_query_tool(false));
        }
    }
    tools.push(llm_definition(virtues_registry::tools::applet_tool(
        &virtues_registry::tools::APPLET_READ_ACTIONS,
    )));
    tools
}

/// The allowlist for a request's background run (`crate::task_runner`):
/// read, research, compute, write pages, and touch the task list. It has no
/// `sql_write` and no applet or memory tools, because a request answers a
/// question and keeps no state of its own. Its answer is the run's reply,
/// which becomes the request's result.
const REQUEST_RUN_ALLOWED_TOOLS: &[&str] = &[
    "think",
    "sql_query",
    "semantic_search",
    "web_search",
    "create_page",
    "get_page_content",
    "edit_page",
    "shell",
    "create_task",
    "update_task",
];

/// Tools for a request's background run: the allowlist above.
pub fn get_tools_for_request() -> Vec<serde_json::Value> {
    tools_named(REQUEST_RUN_ALLOWED_TOOLS)
}

/// The tools behind the CLI's data verbs (`virtues query`, `search`,
/// `schema`, `applet`, `page`). Each verb is one of these run through the
/// executor, so it gets the same role drop, timeout and arguments as chat;
/// `cli::data` refuses anything [`cli_reads`] does not allow.
const CLI_TOOLS: &[&str] = &["sql_query", "semantic_search", "get_page_content"];

/// The tools behind the CLI's write verbs. These run in the server, not the
/// CLI process (`server/api/console.rs` says why), and that door refuses
/// anything [`cli_reads`] or [`cli_writes`] does not allow.
const CLI_WRITE_TOOLS: &[&str] = &["create_page", "edit_page", "sql_write"];

/// Whether a CLI read verb may make this call. For `applet` that is
/// `virtues applet ls` and `get`, and `check`, which always sends
/// `check_only` and so creates nothing.
pub(crate) fn cli_reads(tool: &str, arguments: &serde_json::Value) -> bool {
    match tool {
        "applet" => {
            applet_management::is_read(arguments)
                || (applet_management::action(arguments) == Some("setup") && applet_setup::wants_check_only(arguments))
        }
        _ => CLI_TOOLS.contains(&tool),
    }
}

/// Whether a CLI write verb may make this call. For `applet` that is
/// `virtues applet put` (setup) and `run`; edit and delete have no verb.
pub(crate) fn cli_writes(tool: &str, arguments: &serde_json::Value) -> bool {
    match tool {
        "applet" => matches!(applet_management::action(arguments), Some("setup" | "run")),
        _ => CLI_WRITE_TOOLS.contains(&tool),
    }
}

/// The tools `/api/mcp` lists (`server/api/mcp.rs`). Its callers are agents
/// the owner runs on their own machine, the same caller as the CLI's verbs on
/// another transport, so these are the CLI's reads and writes and `applet`.
/// agents/plan/mcp-plan.md.
const MCP_TOOLS: &[&str] = &[
    "sql_query",
    "semantic_search",
    "get_page_content",
    "create_page",
    "edit_page",
    "sql_write",
    "applet",
];

/// The registry rows `/api/mcp` lists, in registry order, each with whether
/// it only reads.
pub(crate) fn tools_for_mcp() -> Vec<(virtues_registry::tools::ToolConfig, bool)> {
    virtues_registry::tools::default_tools()
        .into_iter()
        .filter(|tool| MCP_TOOLS.contains(&tool.id.as_str()))
        .map(|tool| {
            let reads_only = CLI_TOOLS.contains(&tool.id.as_str());
            (tool, reads_only)
        })
        .collect()
}

/// Whether `/api/mcp` may make this call: a listed tool, with arguments a CLI
/// verb could send. For `applet` that rules out edit and delete.
pub(crate) fn mcp_allows(tool: &str, arguments: &serde_json::Value) -> bool {
    MCP_TOOLS.contains(&tool) && (cli_reads(tool, arguments) || cli_writes(tool, arguments))
}

/// The read-only research tools a Deep Research **subagent** (worker) may use. Explicit allow-list
/// (not a category filter) so workers get pure research capability — no memory/profile writes, and
/// crucially no `dispatch_subagents` (recursion guard).
const SUBAGENT_TOOLS: &[&str] = &[
    "think",
    "web_search",
    "semantic_search",
    "sql_query",
    "shell",
];

/// Get tool definitions for a Deep Research subagent (worker).
pub fn get_tools_for_subagent() -> Vec<serde_json::Value> {
    tools_named(SUBAGENT_TOOLS)
}

#[cfg(test)]
mod slot_model_smoke {
    //! Does the Standard slot's model actually drive OUR tool set?
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
    async fn standard_slot_model_drives_our_real_tool_set() {
        let Ok(key) = std::env::var("AI_GATEWAY_API_KEY") else {
            eprintln!("AI_GATEWAY_API_KEY unset — skipping");
            return;
        };
        let _ = rustls::crypto::ring::default_provider().install_default();

        // Defaults to whatever fills the Standard slot today; set SMOKE_MODEL to
        // drive a CANDIDATE through the same gate before promoting it. The
        // registry tells you to run this first, and "first" means before the
        // id is in the registry at all.
        let model = std::env::var("SMOKE_MODEL").unwrap_or_else(|_| {
            crate::api::model_catalog::model_for_slot(
                virtues_registry::models::ModelSlot::Standard,
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

#[cfg(test)]
mod sudo_scope {
    use super::*;
    use crate::api::chat_mode::ChatMode;

    fn names(tools: &[serde_json::Value]) -> Vec<String> {
        tools
            .iter()
            .filter_map(|t| t["function"]["name"].as_str().map(str::to_string))
            .collect()
    }

    /// `allowed-tools` narrows an applet run's tools and keeps the ones
    /// every prompt applet has.
    #[test]
    fn an_applets_allowed_tools_narrow_what_its_run_is_offered() {
        let allowed = vec!["sql_query".to_string(), "sudo".to_string()];
        let listed = names(&get_tools_for_applet(true, Some(&allowed)));
        assert!(listed.contains(&"sql_query".to_string()));
        assert!(listed.contains(&"think".to_string()));
        assert!(listed.contains(&"update_applet_memory".to_string()));
        assert!(!listed.contains(&"create_page".to_string()), "not listed, not offered");
        assert!(!listed.contains(&"sudo".to_string()), "never beyond the allowlist");
        assert!(names(&get_tools_for_applet(true, None)).contains(&"create_page".to_string()));
    }

    #[test]
    fn only_the_admin_applet_lists_the_shell() {
        let mode = |name| ChatMode::from_wire(name).tools();
        assert!(names(&mode("admin")).contains(&"sudo".to_string()));
        for listing in [
            mode("chat"),
            mode("deep-research"),
            mode("interview"),
            mode("getting_started"),
            mode("anything-else"),
            get_tool_definitions_for_llm(),
            get_all_tool_definitions_for_llm(),
            get_tools_for_applet(true, None),
            get_tools_for_applet(false, None),
            get_tools_for_subagent(),
        ] {
            assert!(!names(&listing).contains(&"sudo".to_string()));
        }
    }
}

//! Deep Research fan-out — the `dispatch_subagents` tool implementation.
//!
//! The orchestrator (Deep Research mode) calls `dispatch_subagents` with a list of independent
//! missions. Each mission runs as its own **read-only** nested agent loop, in parallel, and returns
//! a compressed findings summary plus the sources it touched (for citations). Workers do not get the
//! `dispatch_subagents` tool, so there is no recursion.
//!
//! Each worker runs inside its own `tokio::spawn` with an **owned** `AgentLoop`, so the loop's
//! `Send` stream (which borrows the loop) lives entirely within the task and never forces the
//! parent's future to be non-`Send`.

use std::collections::HashMap;
use std::sync::Arc;

use futures::StreamExt;
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::agent::{AgentConfig, AgentEvent, AgentLoop};
use crate::api::compaction::build_context_for_llm;
use crate::tools::{ToolError, ToolResult};

/// Hard cap on workers per dispatch (matches the tool schema's maxItems).
const MAX_MISSIONS: usize = 5;
/// Step budget per worker — enough to query, compute, and summarize; not unbounded.
const WORKER_MAX_STEPS: u32 = 12;
/// Tool names whose results are worth surfacing as citations in the final answer.
const CITABLE_TOOLS: &[&str] = &["web_search", "semantic_search", "sql_query"];

/// One worker's result, collected back at the orchestrator.
struct MissionResult {
    title: String,
    model: String,
    findings: String,
    sources: Vec<Value>,
    tokens: u32,
    ok: bool,
}

/// Execute the `dispatch_subagents` tool: fan out missions in parallel, collect findings + sources.
pub async fn dispatch(pool: Arc<PgPool>, arguments: Value) -> Result<ToolResult, ToolError> {
    let missions = arguments
        .get("missions")
        .and_then(|m| m.as_array())
        .ok_or_else(|| ToolError::InvalidParameters("`missions` array is required".into()))?;

    if missions.is_empty() {
        return Err(ToolError::InvalidParameters(
            "`missions` must contain at least one mission".into(),
        ));
    }

    // Resolve the three model tiers once (each is a cheap profile read).
    let fast = crate::api::assistant_profile::get_background_model(&pool)
        .await
        .unwrap_or_else(|_| default_tier_model("fast"));
    let balanced = crate::api::assistant_profile::get_chat_model(&pool)
        .await
        .unwrap_or_else(|_| default_tier_model("balanced"));
    let strong = crate::api::assistant_profile::get_reasoning_model(&pool)
        .await
        .unwrap_or_else(|_| default_tier_model("strong"));

    let mut handles = Vec::new();
    for mission in missions.iter().take(MAX_MISSIONS) {
        let title = mission
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Research")
            .to_string();
        let objective = mission
            .get("objective")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if objective.trim().is_empty() {
            continue;
        }
        let model = match mission.get("model").and_then(|v| v.as_str()) {
            Some("fast") => fast.clone(),
            Some("strong") => strong.clone(),
            _ => balanced.clone(),
        };

        let pool = pool.clone();
        handles.push(tokio::spawn(async move {
            run_one_worker(pool, model, title, objective).await
        }));
    }

    if handles.is_empty() {
        return Err(ToolError::InvalidParameters(
            "no mission had a non-empty objective".into(),
        ));
    }

    let joined = futures::future::join_all(handles).await;

    let mut out_missions = Vec::new();
    for result in joined {
        match result {
            Ok(m) => out_missions.push(json!({
                "title": m.title,
                "model": m.model,
                "findings": m.findings,
                "sources": m.sources,
                "tokens": m.tokens,
                "ok": m.ok,
            })),
            Err(e) => {
                tracing::warn!(error = %e, "Subagent task panicked");
            }
        }
    }

    Ok(ToolResult::success(json!({ "missions": out_missions })))
}

/// Run one worker mission to completion in an isolated read-only agent loop.
async fn run_one_worker(
    pool: Arc<PgPool>,
    model: String,
    title: String,
    objective: String,
) -> MissionResult {
    let system_prompt = build_worker_prompt(&objective);
    let messages = build_context_for_llm(&[], None, 0, Some(&system_prompt));
    let tools = crate::tools::get_tools_for_subagent();

    let context = crate::tools::ToolContext {
        page_id: None,
        user_id: Some("subagent".to_string()),
        space_id: None,
        chat_id: None,
        action_id: None,
    };

    let agent_loop = AgentLoop::new((*pool).clone()).with_config(AgentConfig {
        max_steps: WORKER_MAX_STEPS,
        ..Default::default()
    });

    let mut stream = agent_loop.run(model.clone(), messages, tools, context, None, None);

    let mut findings = String::new();
    let mut tokens: u32 = 0;
    let mut had_error = false;
    // id → (tool_name, args) captured from the call lifecycle, completed into a source on result.
    let mut pending: HashMap<String, (String, Value)> = HashMap::new();
    let mut sources: Vec<Value> = Vec::new();

    while let Some(event) = stream.next().await {
        match event {
            AgentEvent::TextDelta { content } => findings.push_str(&content),
            AgentEvent::ToolCallStart { id, name, args } => {
                pending.insert(id, (name, args.unwrap_or(Value::Null)));
            }
            AgentEvent::ToolCallArgsComplete { id, args } => {
                if let Some(entry) = pending.get_mut(&id) {
                    entry.1 = args;
                }
            }
            AgentEvent::ToolCallResult { id, result, .. } => {
                if let Some((tool_name, args)) = pending.get(&id) {
                    if CITABLE_TOOLS.contains(&tool_name.as_str()) {
                        sources.push(json!({
                            "tool_name": tool_name,
                            "args": args,
                            "data": result,
                        }));
                    }
                }
            }
            AgentEvent::Usage { completion_tokens, .. } => {
                tokens = tokens.saturating_add(completion_tokens);
            }
            AgentEvent::Error { message, .. } => {
                tracing::warn!(title = %title, error = %message, "Subagent worker error");
                had_error = true;
            }
            _ => {}
        }
    }

    let ok = !had_error && !findings.trim().is_empty();
    MissionResult {
        title,
        model,
        findings,
        sources,
        tokens,
        ok,
    }
}

/// System prompt for a single read-only research worker.
fn build_worker_prompt(objective: &str) -> String {
    format!(
        "You are a focused research worker with ONE objective. Pursue it, then report back.\n\n\
         <objective>\n{objective}\n</objective>\n\n\
         Rules:\n\
         - You are READ-ONLY: use sql_query, semantic_search, web_search, code_interpreter, and think. \
         You cannot edit anything or spawn other workers.\n\
         - Compute real statistics with code_interpreter where the objective calls for it; report the \
         sample size (n) and how strong or weak any pattern is.\n\
         - Cite the specific records, tables, or web sources you used.\n\
         - Report correlations and observations only — never assert causation.\n\
         - Return a CONCISE findings summary (a few tight paragraphs or bullets), NOT a transcript. \
         Give the orchestrator signal, not noise."
    )
}

/// Fallback model id for a tier when the profile can't be read.
fn default_tier_model(tier: &str) -> String {
    use virtues_registry::models::{default_model_for_slot, ModelSlot};
    let slot = match tier {
        "fast" => ModelSlot::Lite,
        "strong" => ModelSlot::Reasoning,
        _ => ModelSlot::Chat,
    };
    default_model_for_slot(slot).to_string()
}

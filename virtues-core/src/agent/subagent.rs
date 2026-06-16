//! Deep Research fan-out — the `dispatch_subagents` tool implementation.
//!
//! The orchestrator (Deep Research mode) calls `dispatch_subagents` with a list of independent
//! missions. Each mission runs as its own **read-only** nested agent loop, in parallel, and returns
//! a compressed findings summary plus (bounded) sources for citations. Workers do not get the
//! `dispatch_subagents` tool, so there is no recursion.
//!
//! Each worker runs inside its own `tokio::spawn` with an **owned** `AgentLoop`, so the loop's
//! `Send` stream (which borrows the loop) lives entirely within the task and never forces the
//! parent's future to be non-`Send`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use chrono::Utc;
use futures::StreamExt;
use serde_json::{json, Value};
use sqlx::PgPool;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

use crate::agent::{AgentConfig, AgentEvent, AgentLoop};
use crate::api::compaction::build_context_for_llm;
use crate::tools::{SubagentStatus, SubagentUpdate, ToolContext, ToolError, ToolResult};

/// Hard cap on workers per dispatch (matches the tool schema's maxItems).
const MAX_MISSIONS: usize = 5;
/// Step budget per worker — enough to query, compute, and summarize; not unbounded.
const WORKER_MAX_STEPS: u32 = 12;
/// Tool names whose results are worth surfacing as citations in the final answer.
const CITABLE_TOOLS: &[&str] = &["web_search", "semantic_search", "sql_query"];
/// Per-source caps so the (raw) tool payloads handed back to the orchestrator stay bounded.
const MAX_SOURCE_ROWS: usize = 12;
const MAX_SOURCE_WEB_RESULTS: usize = 5;
const MAX_SOURCE_TEXT: usize = 600;

/// Monotonic dispatch counter so each `dispatch_subagents` call gets a unique id. The live panel
/// keys workers by `(dispatch_id, worker_id)` so multiple dispatch rounds in one turn don't collide.
static DISPATCH_COUNTER: AtomicU64 = AtomicU64::new(1);

/// How a dispatched worker is framed. Deep Research workers are READ-ONLY researchers; Council
/// workers are VOICES that speak as a perspective. Defaults to `Research` when a mission omits `style`,
/// so existing Deep Research calls are unaffected.
#[derive(Clone, Copy)]
enum WorkerStyle {
    Research,
    Voice,
}

impl WorkerStyle {
    fn from_mission(mission: &Value) -> Self {
        match mission.get("style").and_then(|v| v.as_str()) {
            Some("voice") => WorkerStyle::Voice,
            _ => WorkerStyle::Research,
        }
    }
}

/// One worker's result, collected back at the orchestrator.
struct MissionResult {
    title: String,
    model: String,
    findings: String,
    sources: Vec<Value>,
    input_tokens: u32,
    output_tokens: u32,
    ok: bool,
}

/// Execute the `dispatch_subagents` tool: fan out missions in parallel, collect findings + sources.
pub async fn dispatch(
    pool: Arc<PgPool>,
    arguments: Value,
    context: &ToolContext,
) -> Result<ToolResult, ToolError> {
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

    let dispatch_id = DISPATCH_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tx = context.subagent_tx.clone();
    let cancel = context.cancel_token.clone();
    let mut budget_exhausted = false;

    let mut handles = Vec::new();
    for mission in missions.iter().take(MAX_MISSIONS) {
        let objective = mission
            .get("objective")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if objective.trim().is_empty() {
            continue;
        }

        // Reserve a slot from the shared per-turn worker budget, if one is set. Once the turn's
        // budget is spent, stop fanning out (the orchestrator can't run unbounded workers).
        if let Some(b) = &context.worker_budget {
            if b.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |c| c.checked_sub(1))
                .is_err()
            {
                budget_exhausted = true;
                break;
            }
        }

        let title = mission
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Research")
            .to_string();
        let model = match mission.get("model").and_then(|v| v.as_str()) {
            Some("fast") => fast.clone(),
            Some("strong") => strong.clone(),
            _ => balanced.clone(),
        };
        let style = WorkerStyle::from_mission(mission);

        let worker_id = handles.len();
        let pool = pool.clone();
        let tx = tx.clone();
        let cancel = cancel.clone();
        handles.push(tokio::spawn(async move {
            run_one_worker(
                pool,
                dispatch_id,
                worker_id,
                model,
                title,
                objective,
                style,
                tx,
                cancel,
            )
            .await
        }));
    }

    if handles.is_empty() {
        if budget_exhausted {
            // Carry the note in `data` (not `error`): the orchestrator reads the serialized data.
            return Ok(ToolResult::success(json!({
                "missions": [],
                "note": "This research turn has reached its worker budget. Synthesize from the findings you already have."
            })));
        }
        return Err(ToolError::InvalidParameters(
            "no mission had a non-empty objective".into(),
        ));
    }

    let joined = futures::future::join_all(handles).await;

    let mut out_missions = Vec::new();
    let mut ok_count = 0usize;
    let mut total_input: u32 = 0;
    let mut total_output: u32 = 0;
    for result in joined {
        match result {
            Ok(m) => {
                if m.ok {
                    ok_count += 1;
                }
                total_input = total_input.saturating_add(m.input_tokens);
                total_output = total_output.saturating_add(m.output_tokens);
                out_missions.push(json!({
                    "title": m.title,
                    "model": m.model,
                    "findings": m.findings,
                    "sources": m.sources,
                    "ok": m.ok,
                }));
            }
            Err(e) => {
                tracing::warn!(error = %e, "Subagent task panicked");
            }
        }
    }

    // Survivor guard: if every worker failed or returned nothing, tell the orchestrator plainly
    // (in `data`, so it's read) rather than handing it a hollow success to confabulate from.
    let note = if ok_count == 0 {
        Some(format!(
            "All {} research workers failed or returned no findings. Try fewer workers with simpler, self-contained objectives — or answer from what you already know.",
            out_missions.len()
        ))
    } else {
        None
    };

    Ok(ToolResult::success(json!({
        "missions": out_missions,
        "note": note,
        // Aggregate worker token usage so the chat handler can bill it (the gateway already
        // charged per call; this keeps the app's own accounting honest).
        "usage": { "input_tokens": total_input, "output_tokens": total_output },
    })))
}

/// Run one worker mission to completion in an isolated read-only agent loop.
#[allow(clippy::too_many_arguments)]
async fn run_one_worker(
    pool: Arc<PgPool>,
    dispatch_id: u64,
    worker_id: usize,
    model: String,
    title: String,
    objective: String,
    style: WorkerStyle,
    tx: Option<Sender<SubagentUpdate>>,
    cancel: Option<CancellationToken>,
) -> MissionResult {
    // Announce the worker as thinking so the panel shows it immediately.
    emit(&tx, dispatch_id, worker_id, &title, &model, SubagentStatus::Thinking, 0).await;

    // Research workers investigate read-only; Council voices speak as a perspective (think-only).
    let (system_prompt, tools) = match style {
        WorkerStyle::Research => (
            build_worker_prompt(&objective),
            crate::tools::get_tools_for_subagent(),
        ),
        WorkerStyle::Voice => (
            build_council_voice_prompt(&objective),
            crate::tools::get_tools_for_council_voice(),
        ),
    };
    let messages = build_context_for_llm(&[], None, 0, Some(&system_prompt));

    let context = ToolContext {
        user_id: Some("subagent".to_string()),
        // Workers don't re-emit panel updates or spawn sub-workers.
        ..Default::default()
    };

    let agent_loop = AgentLoop::new((*pool).clone()).with_config(AgentConfig {
        max_steps: WORKER_MAX_STEPS,
        ..Default::default()
    });

    // Pass the turn's cancel token so a stopped/disconnected chat actually halts the worker.
    let mut stream = agent_loop.run(model.clone(), messages, tools, context, None, cancel);

    let mut findings = String::new();
    let mut input_tokens: u32 = 0;
    let mut output_tokens: u32 = 0;
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
                            // Bound the payload: the orchestrator only needs findings; full rows/
                            // results would bloat its context (and cost) on every subsequent step.
                            "data": truncate_source_data(tool_name, result),
                        }));
                    }
                }
            }
            AgentEvent::Usage { prompt_tokens, completion_tokens, .. } => {
                input_tokens = input_tokens.saturating_add(prompt_tokens);
                output_tokens = output_tokens.saturating_add(completion_tokens);
            }
            AgentEvent::Error { message, .. } => {
                tracing::warn!(title = %title, error = %message, "Subagent worker error");
                had_error = true;
            }
            _ => {}
        }
    }

    // Fallback: a think-only voice worker may pour its perspective into `think` calls and end without
    // a final text answer, leaving `findings` empty. Recover the thinking so the voice isn't lost
    // (and wrongly reported as failed). Harmless for research workers — only fires when there's no text.
    if findings.trim().is_empty() {
        let thoughts: Vec<String> = pending
            .values()
            .filter(|(name, _)| name == "think")
            .filter_map(|(_, args)| {
                args.get("thought")
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string())
            })
            .collect();
        if !thoughts.is_empty() {
            findings = thoughts.join("\n\n");
        }
    }

    let ok = !had_error && !findings.trim().is_empty();
    let status = if ok {
        SubagentStatus::Done
    } else {
        SubagentStatus::Failed
    };
    emit(&tx, dispatch_id, worker_id, &title, &model, status, output_tokens).await;

    MissionResult {
        title,
        model,
        findings,
        sources,
        input_tokens,
        output_tokens,
        ok,
    }
}

/// Send a status update on the side-channel if present (best-effort — never blocks the worker).
#[allow(clippy::too_many_arguments)]
async fn emit(
    tx: &Option<Sender<SubagentUpdate>>,
    dispatch_id: u64,
    worker_id: usize,
    title: &str,
    model: &str,
    status: SubagentStatus,
    tokens: u32,
) {
    if let Some(tx) = tx {
        let _ = tx
            .send(SubagentUpdate {
                dispatch_id,
                id: worker_id,
                title: title.to_string(),
                model: model.to_string(),
                status,
                tokens,
            })
            .await;
    }
}

/// Bound a tool result before it's handed back to the orchestrator: keep the citation-relevant
/// shape (the query, a preview of rows/results) but drop the long tail of data.
fn truncate_source_data(tool_name: &str, mut data: Value) -> Value {
    if let Some(obj) = data.as_object_mut() {
        // SQL / semantic results: keep the first N rows + a total count.
        if let Some(rows) = obj.get("rows").and_then(|r| r.as_array()) {
            let total = rows.len();
            if total > MAX_SOURCE_ROWS {
                let kept: Vec<Value> = rows.iter().take(MAX_SOURCE_ROWS).cloned().collect();
                obj.insert("rows".into(), Value::Array(kept));
                obj.insert("row_count".into(), json!(total));
                obj.insert("truncated".into(), json!(true));
            }
        }
        // web_search: cap result count and trim the heavy full-text field to a snippet.
        if let Some(results) = obj.get_mut("results").and_then(|r| r.as_array_mut()) {
            results.truncate(MAX_SOURCE_WEB_RESULTS);
            for r in results.iter_mut() {
                if let Some(ro) = r.as_object_mut() {
                    if let Some(text) = ro.get("text").and_then(|t| t.as_str()) {
                        if text.len() > MAX_SOURCE_TEXT {
                            let snippet: String = text.chars().take(MAX_SOURCE_TEXT).collect();
                            ro.insert("text".into(), json!(snippet));
                        }
                    }
                }
            }
        }
    }
    let _ = tool_name; // (reserved for tool-specific shaping if needed later)
    data
}

/// System prompt for a single read-only research worker.
fn build_worker_prompt(objective: &str) -> String {
    let datetime = Utc::now().format("%A, %B %-d, %Y at %-I:%M %p UTC").to_string();
    format!(
        "You are a focused research worker with ONE objective. Pursue it, then report back.\n\n\
         Current date/time: {datetime}\n\n\
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

/// System prompt for a Council VOICE worker. Unlike a research worker, a voice does not investigate
/// or cite — it speaks in first person as the archetype its objective describes. The objective carries
/// the whole role (who this voice is, the decision, any context); the voice sees only that.
fn build_council_voice_prompt(objective: &str) -> String {
    format!(
        "You are ONE voice at a council, speaking from a single point of view. You are NOT a researcher \
         and you do NOT cite sources — you give an honest, human perspective.\n\n\
         <objective>\n{objective}\n</objective>\n\n\
         How to speak:\n\
         - Speak in the FIRST PERSON from this vantage. Be specific and honest, including what you'd \
         worry about, push for, or refuse to let slide.\n\
         - You are this perspective only — do NOT try to be balanced or represent the other voices. \
         Your job is to make your angle as real and sharp as it deserves to be.\n\
         - If your objective frames you as a real person's LENS (\"how X would approach this\"), speak \
         AS THAT LIKELY LENS, not as a faithful prediction of what they would really say — you are a \
         thought experiment that helps take their view, not a forecast of their actual opinion.\n\
         - End with a plain-text view (do NOT end on a think call). Use think to reason if it helps, \
         then give your view as a few tight paragraphs (or a short list) — signal, not a transcript."
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

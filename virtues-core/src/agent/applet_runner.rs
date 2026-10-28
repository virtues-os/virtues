//! Agent loop runner for actions with an `agent` field set.
//!
//! This runs the LLM agent loop for an action. It is invoked by the unified
//! `crate::applet_runner::run_applet` after any subprocess phase has completed.
//! Trigger validation, condition evaluation, concurrency gating, and run-row
//! lifecycle are all handled upstream. The loop itself is
//! `agent::headless`; this module adds what is particular to applets: the
//! model slot, the prompt and its memory.
//!
//! A run starts clean and writes into no chat (agents/plan/applet-shape.md,
//! "Output and where results go"). It reads its memory and its tables, and
//! what it says lands on its own run, in the applet's thread on its page.

use serde::Serialize;
use sqlx::PgPool;

use virtues_registry::models::ModelSlot;

use super::headless::{model_for_slot, run_headless, HeadlessJob};
use crate::error::Result;
use crate::scheduler::applets::Applet;
use crate::server::yjs::YjsState;

/// Result of a single agent loop run.
#[derive(Debug, Serialize)]
pub struct AgentLoopResult {
    pub applet_id: String,
    pub steps: u32,
    pub message: Option<String>,
    /// What this run spent with the model, in micros-USD — the sum of the
    /// gateway's authoritative per-call figures, never an estimate.
    pub cost_micros: i64,
    /// Set when the loop was stopped by `config.limits.max_llm_cost` rather
    /// than finishing. The run is recorded `budget_exceeded`, not `error`:
    /// nothing broke, a ceiling the owner set was reached.
    pub budget_stopped: Option<String>,
    /// Set when the model stream reported an error (no key, upstream failure,
    /// a tool loop that died). The run is recorded `error`, never `success` —
    /// the loop ends quietly after an error event, so without this a run that
    /// never reached the model looked like a clean zero-step finish.
    pub error: Option<String>,
    /// The pages it wrote, which the run records as what it made.
    pub pages_written: usize,
}

/// Run one pass of the LLM agent loop for an action.
///
/// `prompt` is the action's `agent` field (the instruction). `context` is an
/// optional dynamic context block — typically the result summary from a
/// subprocess phase that ran immediately before. Concurrency/condition gating
/// is handled upstream by `crate::applet_runner::run_applet`.
///
/// `run_id` is the row this run's model spend is attributed to. Every `Usage`
/// event becomes one `app_ai_calls` row tagged with it — which is what makes
/// applet spend visible in the Usage tab at all, and what the per-run ceiling
/// is checked against.
pub async fn run_agent_loop(
    pool: &PgPool,
    yjs_state: &YjsState,
    action: &Applet,
    prompt: &str,
    context: Option<&str>,
    run_id: &str,
    message: Option<&str>,
) -> Result<AgentLoopResult> {
    let applet_id = &action.id;
    let limits = crate::applet_runner::limits::Limits::from_config(&action.config);

    let model_override = action.config.get("model").and_then(|v| v.as_str()).map(|s| s.to_string());
    // An applet may ask for a SLOT instead of a model id. The slot is the
    // supported lever: a model id in a manifest is a literal that goes stale
    // the moment the catalog moves, which is why the registry owns selection.
    //
    // Without this an agent applet gets the background (Lite) model, which is
    // right for the summarizing and bookkeeping most of them do and wrong for
    // any applet whose OUTPUT is prose a person reads. The wiki editor is the
    // second kind: its articles are held to the same bar as the day's.
    let model_slot = action
        .config
        .get("model_slot")
        .and_then(|v| v.as_str())
        .and_then(|s| {
            // `chat` is the Standard slot's old name, and authored applets on
            // a box may still say it; `from_name` reads it.
            match ModelSlot::from_name(s) {
                Some(slot @ (ModelSlot::Lite | ModelSlot::Standard | ModelSlot::Deep)) => Some(slot),
                _ => {
                    tracing::warn!(applet_id = %action.id, slot = s, "unknown model_slot; using the background model");
                    None
                }
            }
        });
    let model = match model_override {
        Some(m) => m,
        None => model_for_slot(pool, model_slot.unwrap_or(ModelSlot::Lite)).await,
    };

    // A `SKILL.md` applet's `allowed-tools`, which reconcile writes here.
    let allowed_tools: Option<Vec<String>> = action
        .config
        .get("allowed_tools")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|t| t.as_str().map(String::from)).collect());

    let tool_context = crate::tools::ToolContext {
        user_id: Some("system".to_string()),
        applet_id: Some(applet_id.to_string()),
        own_applet: crate::applet_templates::dir_for_applet_id(applet_id)
            .and_then(|d| d.rsplit('/').next().map(str::to_string)),
        applet_run_id: Some(run_id.to_string()),
        // A run is a turn: what it reads of a page serves it, not a later run
        // that sees this one's results only as clipped replays.
        turn: Some(crate::api::chat::generate_id()),
        ..Default::default()
    };

    let outcome = run_headless(
        pool,
        yjs_state,
        HeadlessJob {
            label: applet_id,
            model,
            system_prompt: build_applet_system_prompt(pool, prompt, context, action.memory.as_deref()).await,
            chat_id: None,
            message,
            tools: {
                let mut tools = crate::tools::get_tools_for_applet(false, allowed_tools.as_deref());
                // Its `references/` and `assets/`, when it has any.
                tools.extend(crate::tools::use_applet::own_files_definition_for_id(applet_id));
                // The connection tools its file lists and the owner allowed.
                tools.extend(crate::tools::connection::applet_tool_definitions(pool, applet_id).await);
                tools
            },
            tool_context,
            max_cost_micros: limits.max_llm_cost_micros,
            feature: "applet",
            applet_run_id: Some(run_id.to_string()),
        },
    )
    .await?;

    // What the run made, so the applet's page can show it. A record we
    // failed to write must not fail the run that wrote the page.
    for page_id in &outcome.pages_written {
        if let Err(e) = crate::scheduler::applets::record_page_output(pool, run_id, page_id).await {
            tracing::warn!(applet_id, page_id, error = %e, "failed to record the page a run wrote");
        }
    }

    Ok(AgentLoopResult {
        applet_id: applet_id.to_string(),
        steps: outcome.steps,
        message: outcome.message,
        cost_micros: outcome.cost_micros,
        budget_stopped: outcome.budget_stopped,
        error: outcome.error,
        pages_written: outcome.pages_written.len(),
    })
}

// ============================================================================
// Helpers
// ============================================================================

/// Build the system prompt for an autonomous action run.
///
/// `context` is optional dynamic data supplied by the unified runner — typically
/// the stdout summary from a subprocess phase that ran immediately before.
async fn build_applet_system_prompt(
    pool: &PgPool,
    instruction: &str,
    context: Option<&str>,
    memory: Option<&str>,
) -> String {
    let assistant_name = crate::api::assistant_profile::get_assistant_name(pool)
        .await
        .unwrap_or_else(|_| "Ari".to_string());
    let user_name = crate::api::profile::get_display_name(pool)
        .await
        .unwrap_or_else(|_| "there".to_string());

    let datetime = crate::api::profile::local_datetime_line(pool).await;

    // Only what the applet cannot know for itself. Its instruction says what
    // it is and what to reply with; a role or a reply format stated here
    // contradicted every applet that defines its own.
    let mut prompt = format!(
        "You are {assistant_name}, running on {user_name}'s server as one of their applets, with nobody watching.\n\n\
         Current date/time: {datetime}\n\n\
         <action_instruction>\n{instruction}\n</action_instruction>\n\n\
         Follow the instruction above. If it does not say what to reply with, reply with a short account of what you did and found.",
    );

    if let Some(mem) = memory {
        if !mem.trim().is_empty() {
            prompt.push_str(&format!(
                "\n\n<memory>\nYour persistent memory from prior runs. You can update this with the update_applet_memory tool.\n{}\n</memory>",
                mem
            ));
        }
    }

    if let Some(ctx) = context {
        prompt.push_str(&format!(
            "\n\n<context>\n{}\n</context>",
            ctx
        ));
    }

    prompt
}

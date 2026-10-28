//! Agent loop runner for actions with an `agent` field set.
//!
//! This runs the LLM agent loop for an action. It is invoked by the unified
//! `crate::applet_runner::run_applet` after any subprocess phase has completed.
//! Trigger validation, condition evaluation, concurrency gating, and run-row
//! lifecycle are all handled upstream. The loop itself is
//! `agent::headless`; this module adds what is particular to applets: where
//! the work goes, the model slot, the prompt and its memory.

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
    pub chat_id: Option<String>,
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

    // Extract optional chat_id and model from config
    let chat_id = action.config.get("chat_id").and_then(|v| v.as_str()).map(|s| s.to_string());

    // An applet whose work has nowhere to go fails before it spends anything
    // on the model: the run could only finish as a success that made nothing.
    // A chat in Recently deleted still has its id here, but nobody reads it.
    let has_chat = match &chat_id {
        Some(cid) => {
            sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS (SELECT 1 FROM app_chats WHERE id = $1 AND deleted_at IS NULL)",
            )
            .bind(cid)
            .fetch_one(pool)
            .await?
        }
        None => false,
    };
    let has_face = crate::server::faces::face_for(applet_id).is_some();
    let delivers = delivers_kind(&action.config);
    if let Some(reason) = missing_destination(delivers, has_chat, has_face) {
        tracing::warn!(applet_id, %reason, "applet has nowhere to put its work");
        return Ok(AgentLoopResult {
            applet_id: applet_id.to_string(),
            chat_id,
            steps: 0,
            message: None,
            cost_micros: 0,
            budget_stopped: None,
            error: Some(reason),
        });
    }
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
        chat_id: chat_id.clone(),
        applet_id: Some(applet_id.to_string()),
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
            chat_id: chat_id.clone(),
            message,
            tools: crate::tools::get_tools_for_applet(chat_id.is_some(), allowed_tools.as_deref()),
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

    // A run that finished without making what the applet makes failed, even
    // when the model ended politely. Without this, a run whose query broke
    // and which then said "I couldn't create today's page" was recorded as a
    // success. A run already failed or stopped at its ceiling keeps that
    // reason.
    let mut error = outcome.error;
    if error.is_none() && outcome.budget_stopped.is_none() {
        error = missing_delivery(delivers, !outcome.pages_written.is_empty(), message.is_some());
    }

    Ok(AgentLoopResult {
        applet_id: applet_id.to_string(),
        chat_id,
        steps: outcome.steps,
        message: outcome.message,
        cost_micros: outcome.cost_micros,
        budget_stopped: outcome.budget_stopped,
        error,
    })
}

// ============================================================================
// Helpers
// ============================================================================

/// What an applet says it makes, from `config.delivers.kind` ("page",
/// "chat", "dashboard"). Absent when it doesn't say, which most don't yet.
fn delivers_kind(config: &serde_json::Value) -> Option<&str> {
    config.get("delivers")?.get("kind")?.as_str()
}

/// Why a finished run failed to make what its applet makes, if it did.
/// Only a page is checked after the run: the other kinds have no tool whose
/// success says the work arrived, so `missing_destination` checks them
/// before it instead. A run a person's message woke is answering them, and a
/// reply that writes no page is still a reply, so it isn't checked.
fn missing_delivery(delivers: Option<&str>, wrote_page: bool, answering: bool) -> Option<String> {
    (delivers == Some("page") && !wrote_page && !answering)
        .then(|| "The run ended without writing its page.".to_string())
}

/// Why an applet's work has nowhere to go, if it doesn't. A conversation or a
/// dashboard can't be checked after the run the way a page is: a run that
/// found nothing new rightly posts nothing and changes no table. What can be
/// checked is that the place exists.
fn missing_destination(delivers: Option<&str>, has_chat: bool, has_face: bool) -> Option<String> {
    match delivers {
        Some("chat") if !has_chat => Some("It has no conversation to post to.".to_string()),
        Some("dashboard") if !has_face => Some("It has no dashboard to update.".to_string()),
        _ => None,
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_page_applet_that_wrote_no_page_failed() {
        let page = json!({ "delivers": { "kind": "page" } });
        assert_eq!(delivers_kind(&page), Some("page"));
        assert_eq!(delivers_kind(&json!({})), None);
        assert!(missing_delivery(Some("page"), false, false).is_some());
        assert!(missing_delivery(Some("page"), true, false).is_none());
        assert!(missing_delivery(None, false, false).is_none(), "an applet that says nothing is not checked");
        assert!(missing_delivery(Some("chat"), false, false).is_none(), "a quiet run on a chat applet is not a failure");
        assert!(missing_delivery(Some("page"), false, true).is_none(), "a reply to a message is not a missing page");
    }

    #[test]
    fn an_applet_with_nowhere_to_put_its_work_fails() {
        assert!(missing_destination(Some("chat"), false, false).is_some());
        assert!(missing_destination(Some("chat"), true, false).is_none());
        assert!(missing_destination(Some("dashboard"), false, false).is_some());
        assert!(missing_destination(Some("dashboard"), false, true).is_none());
        assert!(missing_destination(Some("page"), false, false).is_none(), "every box has Pages");
        assert!(missing_destination(None, false, false).is_none(), "an applet that says nothing is not checked");
    }
}

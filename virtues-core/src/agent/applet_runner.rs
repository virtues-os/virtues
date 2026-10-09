//! Agent loop runner for actions with an `agent` field set.
//!
//! This runs the LLM agent loop for an action. It is invoked by the unified
//! `crate::applet_runner::run_applet` after any subprocess phase has completed.
//! Trigger validation, condition evaluation, concurrency gating, and run-row
//! lifecycle are all handled upstream — this function is pure execution.

use serde::Serialize;
use sqlx::PgPool;

use crate::api::chats::{append_message, ChatMessage};
use crate::api::compaction::build_context_for_llm;
use crate::error::Result;
use crate::scheduler::applets::Applet;
use crate::server::yjs::YjsState;
use crate::types::Timestamp;

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
            use virtues_registry::models::ModelSlot;
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

    // Build system prompt (with memory if present)
    let system_prompt =
        build_applet_system_prompt(pool, prompt, context, action.memory.as_deref()).await;

    // 3. Load compacted message history (only if chat is linked)
    let messages = if let Some(cid) = &chat_id {
        load_chat_messages(pool, cid).await?
    } else {
        Vec::new()
    };
    let mut llm_messages = build_context_for_llm(&messages, None, 0, Some(&system_prompt), None);

    add_wake_turn(&mut llm_messages, message);
    // The person's words go into the chat too, so the conversation shows
    // what was said.
    if let (Some(text), Some(cid)) = (message, &chat_id) {
        let said = ChatMessage {
            id: None,
            role: "user".to_string(),
            content: text.to_string(),
            timestamp: Timestamp::now(),
            model: None,
            provider: None,
            agent_id: None,
            tool_calls: None,
            reasoning: None,
            intent: None,
            subject: None,
            reasoning_details: None,
            parts: None,
        };
        if let Err(e) = append_message(pool, cid.clone(), said).await {
            tracing::warn!(applet_id, error = %e, "failed to post the message to the applet's chat");
        }
    }

    // 4. Get tools and model
    let tools = crate::tools::get_tools_for_applet();
    let model = if let Some(m) = &model_override {
        m.clone()
    } else if let Some(slot) = model_slot {
        // The person's pin for that slot when they have one, else the
        // registry's — the same door chat resolves through.
        match slot {
            virtues_registry::models::ModelSlot::Standard => {
                crate::api::assistant_profile::get_standard_model(pool).await
            }
            virtues_registry::models::ModelSlot::Deep => {
                crate::api::assistant_profile::get_deep_model(pool).await
            }
            _ => crate::api::assistant_profile::get_background_model(pool).await,
        }
        .unwrap_or_else(|_| crate::api::model_catalog::model_for_slot(slot))
    } else {
        crate::api::assistant_profile::get_background_model(pool).await
            .unwrap_or_else(|_| crate::api::model_catalog::model_for_slot(
                virtues_registry::models::ModelSlot::Lite
            ))
    };

    // 5. Create and run AgentLoop (egress via BearerClient — no api config needed)
    let tool_context = crate::tools::ToolContext {
        user_id: Some("system".to_string()),
        chat_id: chat_id.clone(),
        applet_id: Some(applet_id.to_string()),
        ..Default::default()
    };

    // The spend ceiling is the loop's own budget: checked between steps
    // against what the gateway charged, and enforced outside the model. When
    // it is reached the loop takes one last step with tools off, so the run
    // ends in an answer rather than mid-task — which can spend one step past
    // the ceiling.
    let agent_loop = crate::agent::AgentLoop::new_with_yjs(pool.clone(), yjs_state.clone())
        .with_budget(crate::agent::TurnBudget {
            max_cost_micros: limits.max_llm_cost_micros,
            ..Default::default()
        });

    tracing::info!(applet_id, model = %model, chat_id = ?chat_id, "Starting action run");

    // 7. Consume the event stream
    use futures::StreamExt;
    let mut stream = agent_loop.run(
        model.clone(),
        llm_messages,
        tools,
        tool_context,
        None,
    );

    let mut assistant_content = String::new();
    let mut step_count: u32 = 0;
    let mut cost_micros: i64 = 0;
    let mut budget_stopped: Option<String> = None;
    let mut error: Option<String> = None;

    while let Some(event) = stream.next().await {
        match event {
            crate::agent::AgentEvent::TextDelta { content } => {
                assistant_content.push_str(&content);
            }
            crate::agent::AgentEvent::StepComplete { step, .. } => {
                step_count = step;
            }
            // One LLM call, one `app_ai_calls` row — the table's own contract,
            // and the only reason applet spend appears in the Usage tab. This
            // arm used to fall through the catch-all below, so every agent
            // applet's cost was silently discarded.
            crate::agent::AgentEvent::Usage {
                prompt_tokens,
                completion_tokens,
                reasoning_tokens,
                cost_micros: step_cost,
                ..
            } => {
                let step_cost = step_cost.unwrap_or(0);
                cost_micros += step_cost;

                if let Err(e) = crate::api::ai_calls::record_ai_call(
                    pool,
                    &crate::api::ai_calls::AiCall {
                        feature: "applet".to_string(),
                        model: model.clone(),
                        prompt_tokens: prompt_tokens as i64,
                        completion_tokens: completion_tokens as i64,
                        reasoning_tokens: reasoning_tokens.unwrap_or(0) as i64,
                        cost_micros: step_cost,
                        // Applet runs egress through `BearerClient`, which
                        // forks to the user's own endpoint when BYO is set —
                        // so this is NOT always the wallet, and `step_cost` is
                        // 0-as-unknown when it isn't.
                        route: if crate::api::settings_byo::byo_is_active(pool).await {
                            crate::api::ai_calls::Route::Byo
                        } else {
                            crate::api::ai_calls::Route::Wallet
                        },
                        applet_run_id: Some(run_id.to_string()),
                    },
                )
                .await
                {
                    // Best-effort, exactly as on the chat path: a cost row we
                    // failed to write must not fail the run that earned it.
                    tracing::warn!(applet_id, error = %e, "failed to record applet ai_call");
                }
            }
            crate::agent::AgentEvent::Done {
                total_steps,
                finish_reason: crate::agent::FinishReason::BudgetExceeded,
            } => {
                let cap = limits.max_llm_cost_micros.unwrap_or(cost_micros);
                tracing::info!(applet_id, cost_micros, cap, "applet hit spend ceiling");
                budget_stopped = Some(format!(
                    "stopped at the spend ceiling — {} of {} used after {} step{}",
                    crate::applet_runner::limits::format_usd(cost_micros),
                    crate::applet_runner::limits::format_usd(cap),
                    total_steps.max(1),
                    if total_steps == 1 { "" } else { "s" }
                ));
            }
            crate::agent::AgentEvent::Error { message, .. } => {
                tracing::error!(applet_id, error = %message, "Applet run error");
                // The first error is the cause; anything after it is fallout.
                error.get_or_insert_with(|| message.clone());
                if let Some(cid) = &chat_id {
                    let error_msg = ChatMessage {
                        id: None,
                        role: "system".to_string(),
                        content: format!("[System: Applet run error: {}]", message),
                        timestamp: Timestamp::now(),
                        model: None,
                        provider: None,
                        agent_id: Some("autonomous".to_string()),
                        tool_calls: None,
                        reasoning: None,
                        intent: None,
                        subject: None,
                        reasoning_details: None,
                        parts: None,
                    };
                    if let Err(e) = append_message(pool, cid.clone(), error_msg).await {
                        tracing::warn!(applet_id, error = %e, "failed to post applet error to its chat");
                    }
                }
            }
            _ => {}
        }
    }

    // 8. Save assistant message (only if chat is linked)
    if !assistant_content.is_empty() {
        if let Some(cid) = &chat_id {
            let msg = ChatMessage {
                id: None,
                role: "assistant".to_string(),
                content: assistant_content.clone(),
                timestamp: Timestamp::now(),
                model: Some(model),
                provider: None,
                agent_id: Some("autonomous".to_string()),
                tool_calls: None,
                reasoning: None,
                intent: None,
                subject: None,
                reasoning_details: None,
                parts: None,
            };
            if let Err(e) = append_message(pool, cid.clone(), msg).await {
                tracing::warn!(applet_id, error = %e, "failed to post applet reply to its chat");
            }
        }
    }

    tracing::info!(
        applet_id,
        steps = step_count,
        cost_micros,
        failed = error.is_some(),
        "Applet run complete"
    );

    Ok(AgentLoopResult {
        applet_id: applet_id.to_string(),
        chat_id,
        steps: step_count,
        message: if assistant_content.is_empty() {
            None
        } else {
            Some(assistant_content)
        },
        cost_micros,
        budget_stopped,
        error,
    })
}

// ============================================================================
// Helpers
// ============================================================================

/// What a wake nobody asked for (a clock, a poll) runs on.
const RUN_INSTRUCTION: &str = "Run your action instruction now.";

/// Ends the conversation on this wake's own turn, after whatever the chat
/// already holds: the person's message when they sent one, the instruction
/// otherwise. Without a turn of its own a run on a chat-linked applet ended on
/// the applet's last reply, which the model reads as text to continue, and a
/// message sent to it was never read at all.
///
/// Providers (Bedrock, zai) also require the first non-system message to be a
/// user message, and a compacted chat's post-checkpoint tail can begin with
/// an assistant turn, so that case opens with the instruction.
fn add_wake_turn(llm_messages: &mut Vec<serde_json::Value>, message: Option<&str>) {
    let role = |m: &serde_json::Value| m.get("role").and_then(|r| r.as_str()).map(str::to_owned);
    if let Some(i) = llm_messages.iter().position(|m| role(m).as_deref() != Some("system")) {
        if role(&llm_messages[i]).as_deref() != Some("user") {
            llm_messages.insert(i, serde_json::json!({ "role": "user", "content": RUN_INSTRUCTION }));
        }
    }
    llm_messages.push(serde_json::json!({
        "role": "user",
        "content": message.unwrap_or(RUN_INSTRUCTION),
    }));
}

/// Load chat messages for context building.
async fn load_chat_messages(pool: &PgPool, chat_id: &str) -> Result<Vec<ChatMessage>> {
    let rows = sqlx::query_as::<_, (
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<serde_json::Value>,
        Option<String>,
        Option<serde_json::Value>,
        Timestamp,
    )>(
        r#"
        SELECT id, role, content, model, provider, agent_id, reasoning, tool_calls, subject, reasoning_details, created_at
        FROM app_chat_messages
        WHERE chat_id = $1
        ORDER BY sequence_num ASC
        "#,
    )
    .bind(chat_id)
    .fetch_all(pool)
    .await?;

    let messages = rows
        .into_iter()
        .map(|(id, role, content, model, provider, agent_id, reasoning, tool_calls_raw, subject, reasoning_details, timestamp)| {
            let tool_calls = tool_calls_raw
                .and_then(|tc| serde_json::from_value(tc).ok());

            ChatMessage {
                id: Some(id),
                role,
                content,
                timestamp,
                model,
                provider,
                agent_id,
                tool_calls,
                reasoning,
                intent: None,
                subject,
                reasoning_details,
                parts: None,
            }
        })
        .collect();

    Ok(messages)
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

    let mut prompt = format!(
        "You are {assistant_name}, {user_name}'s personal AI assistant, running autonomously.\n\n\
         Current date/time: {datetime}\n\n\
         <action_instruction>\n{instruction}\n</action_instruction>\n\n\
         You are running as an action. Use your tools to accomplish your mission. \
         Log your findings as your response. Be concise and actionable.",
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
    use serde_json::{json, Value};

    fn roles_and_text(m: &[Value]) -> Vec<(String, String)> {
        m.iter()
            .map(|x| (x["role"].as_str().unwrap().to_string(), x["content"].as_str().unwrap_or("").to_string()))
            .collect()
    }

    #[test]
    fn a_message_is_the_last_turn_after_the_chat_history() {
        let mut m = vec![
            json!({"role": "system", "content": "s"}),
            json!({"role": "user", "content": "I had eggs"}),
            json!({"role": "assistant", "content": "Logged."}),
        ];
        add_wake_turn(&mut m, Some("And toast"));
        assert_eq!(roles_and_text(&m).last().unwrap(), &("user".to_string(), "And toast".to_string()));
        assert_eq!(m.len(), 4, "nothing else is added when the history already opens with the person");
    }

    #[test]
    fn a_scheduled_wake_on_a_chat_ends_on_the_instruction_not_the_last_reply() {
        let mut m = vec![
            json!({"role": "system", "content": "s"}),
            json!({"role": "user", "content": "Make it weekly"}),
            json!({"role": "assistant", "content": "Done."}),
        ];
        add_wake_turn(&mut m, None);
        assert_eq!(roles_and_text(&m).last().unwrap(), &("user".to_string(), RUN_INSTRUCTION.to_string()));
    }

    #[test]
    fn an_empty_history_gets_one_turn() {
        let mut m = vec![json!({"role": "system", "content": "s"})];
        add_wake_turn(&mut m, None);
        assert_eq!(
            roles_and_text(&m),
            vec![("system".into(), "s".into()), ("user".into(), RUN_INSTRUCTION.into())]
        );
    }

    #[test]
    fn a_tail_that_opens_on_an_assistant_turn_is_opened_by_the_instruction() {
        let mut m = vec![
            json!({"role": "system", "content": "s"}),
            json!({"role": "assistant", "content": "Earlier reply"}),
        ];
        add_wake_turn(&mut m, Some("Hello"));
        let r = roles_and_text(&m);
        assert_eq!(r[1].0, "user", "providers need the first non-system turn to be the user's");
        assert_eq!(r.last().unwrap(), &("user".to_string(), "Hello".to_string()));
    }
}

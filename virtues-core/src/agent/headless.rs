//! One headless pass of the agent loop: no person watching, no stream to a
//! client. Applet runs (`agent::applet_runner`) and request runs
//! (`crate::task_runner`) both go through [`run_headless`]. Each caller
//! builds its own prompt and decides what the outcome means; this module
//! loads the chat, runs the loop under a spend ceiling, records every model
//! call in `app_ai_calls`, and posts the reply to the chat.

use futures::StreamExt;
use sqlx::PgPool;
use virtues_registry::models::ModelSlot;

use crate::api::chats::{append_message, ChatMessage};
use crate::api::compaction::build_context_for_llm;
use crate::error::Result;
use crate::server::yjs::YjsState;
use crate::types::Timestamp;

/// Everything one headless run needs.
pub struct HeadlessJob<'a> {
    /// Names the run in logs: an applet id or a task id.
    pub label: &'a str,
    pub model: String,
    pub system_prompt: String,
    /// The chat the run reads its history from and posts its reply to.
    pub chat_id: Option<String>,
    /// A person's words for this run. They are posted to the chat and end
    /// the conversation the model sees. `None` wakes the run on
    /// [`RUN_INSTRUCTION`].
    pub message: Option<&'a str>,
    pub tools: Vec<serde_json::Value>,
    pub tool_context: crate::tools::ToolContext,
    /// Spend ceiling for this run, in micros-USD. `None` is no ceiling.
    pub max_cost_micros: Option<i64>,
    /// `app_ai_calls.feature` for the run's model calls.
    pub feature: &'static str,
    /// `app_ai_calls.applet_run_id`, for an applet run.
    pub applet_run_id: Option<String>,
}

/// What a headless run did.
#[derive(Debug, Default)]
pub struct HeadlessOutcome {
    pub steps: u32,
    /// The model's reply, when it said anything.
    pub message: Option<String>,
    /// What the run spent with the model, in micros-USD: the sum of the
    /// gateway's per-call figures, never an estimate. Zero on BYO, where the
    /// cost is unknown.
    pub cost_micros: i64,
    /// Set when the spend ceiling stopped the loop before it finished.
    /// Nothing broke: a limit the owner set was reached.
    pub budget_stopped: Option<String>,
    /// Set when the model stream reported an error (no key, an upstream
    /// failure, a tool loop that died). The loop ends quietly after an error
    /// event, so without this a run that never reached the model looks like
    /// a clean zero-step finish.
    pub error: Option<String>,
    /// Pages the run wrote, in the order it wrote them.
    pub pages_written: Vec<String>,
}

/// The model for a slot: the owner's pin when they have one, else the
/// registry's. Lite is the background model.
pub async fn model_for_slot(pool: &PgPool, slot: ModelSlot) -> String {
    match slot {
        ModelSlot::Standard => crate::api::assistant_profile::get_standard_model(pool).await,
        ModelSlot::Deep => crate::api::assistant_profile::get_deep_model(pool).await,
        _ => crate::api::assistant_profile::get_background_model(pool).await,
    }
    .unwrap_or_else(|_| crate::api::model_catalog::model_for_slot(slot))
}

pub async fn run_headless(pool: &PgPool, yjs_state: &YjsState, job: HeadlessJob<'_>) -> Result<HeadlessOutcome> {
    let label = job.label;
    let chat_id = job.chat_id;

    let history = match &chat_id {
        Some(cid) => load_chat_messages(pool, cid).await?,
        None => Vec::new(),
    };
    let mut llm_messages = build_context_for_llm(&history, None, 0, Some(&job.system_prompt), None, None);
    add_wake_turn(&mut llm_messages, job.message);
    // The person's words go into the chat too, so the conversation shows
    // what was said.
    if let (Some(text), Some(cid)) = (job.message, &chat_id) {
        if let Err(e) = append_message(pool, cid.clone(), chat_message("user", text, None, None)).await {
            tracing::warn!(label, error = %e, "failed to post the message to the run's chat");
        }
    }

    // The spend ceiling is the loop's own budget: checked between steps
    // against what the gateway charged, and enforced outside the model. When
    // it is reached the loop takes one last step with tools off, so the run
    // ends in an answer rather than mid-task, which can spend one step past
    // the ceiling.
    let agent_loop = crate::agent::AgentLoop::new_with_yjs(pool.clone(), yjs_state.clone()).with_budget(
        crate::agent::TurnBudget { max_cost_micros: job.max_cost_micros, ..Default::default() },
    );

    tracing::info!(label, model = %job.model, chat_id = ?chat_id, "starting headless run");
    let mut stream = agent_loop.run(job.model.clone(), llm_messages, job.tools, job.tool_context, None);

    // Runs egress through `BearerClient`, which forks to the owner's own
    // endpoint when BYO is set, so the purse is not always the wallet.
    let route = if crate::api::settings_byo::byo_is_active(pool).await {
        crate::api::ai_calls::Route::Byo
    } else {
        crate::api::ai_calls::Route::Wallet
    };

    let mut out = HeadlessOutcome::default();
    let mut reply = String::new();
    // Tool results name only the call's id, so the name is kept from the
    // call's start.
    let mut tool_names: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    while let Some(event) = stream.next().await {
        match event {
            crate::agent::AgentEvent::ToolCallStart { id, name, .. } => {
                tool_names.insert(id, name);
            }
            crate::agent::AgentEvent::ToolCallResult { id, result, success, .. } => {
                if let Some(page_id) = tool_names.get(&id).and_then(|name| page_written(name, success, &result)) {
                    out.pages_written.push(page_id.to_string());
                }
            }
            crate::agent::AgentEvent::TextDelta { content } => reply.push_str(&content),
            crate::agent::AgentEvent::StepComplete { step, .. } => out.steps = step,
            // One model call, one `app_ai_calls` row: the table's contract,
            // and how headless spend reaches the Usage tab and the ceilings.
            crate::agent::AgentEvent::Usage {
                prompt_tokens,
                completion_tokens,
                reasoning_tokens,
                cost_micros,
                ..
            } => {
                let cost = cost_micros.unwrap_or(0);
                out.cost_micros += cost;
                let call = crate::api::ai_calls::AiCall {
                    feature: job.feature.to_string(),
                    model: job.model.clone(),
                    prompt_tokens: prompt_tokens as i64,
                    completion_tokens: completion_tokens as i64,
                    reasoning_tokens: reasoning_tokens.unwrap_or(0) as i64,
                    cost_micros: cost,
                    route,
                    chat_id: chat_id.clone(),
                    applet_run_id: job.applet_run_id.clone(),
                };
                if let Err(e) = crate::api::ai_calls::record_ai_call(pool, &call).await {
                    // A cost row we failed to write must not fail the run
                    // that earned it, the same as on the chat path.
                    tracing::warn!(label, error = %e, "failed to record a headless ai_call");
                }
            }
            crate::agent::AgentEvent::Done {
                total_steps,
                finish_reason: crate::agent::FinishReason::BudgetExceeded,
            } => {
                let cap = job.max_cost_micros.unwrap_or(out.cost_micros);
                tracing::info!(label, cost_micros = out.cost_micros, cap, "headless run hit its spend ceiling");
                out.budget_stopped = Some(format!(
                    "stopped at the spend ceiling: {} of {} used after {} step{}",
                    crate::applet_runner::limits::format_usd(out.cost_micros),
                    crate::applet_runner::limits::format_usd(cap),
                    total_steps.max(1),
                    if total_steps == 1 { "" } else { "s" }
                ));
            }
            crate::agent::AgentEvent::Error { message, .. } => {
                tracing::error!(label, error = %message, "headless run error");
                // The first error is the cause; anything after it is fallout.
                out.error.get_or_insert_with(|| message.clone());
                if let Some(cid) = &chat_id {
                    let note = chat_message(
                        "system",
                        &format!("[System: run error: {message}]"),
                        None,
                        Some("autonomous"),
                    );
                    if let Err(e) = append_message(pool, cid.clone(), note).await {
                        tracing::warn!(label, error = %e, "failed to post a run error to its chat");
                    }
                }
            }
            _ => {}
        }
    }

    if !reply.is_empty() {
        if let Some(cid) = &chat_id {
            let msg = chat_message("assistant", &reply, Some(job.model.clone()), Some("autonomous"));
            if let Err(e) = append_message(pool, cid.clone(), msg).await {
                tracing::warn!(label, error = %e, "failed to post a run's reply to its chat");
            }
        }
        out.message = Some(reply);
    }

    tracing::info!(label, steps = out.steps, cost_micros = out.cost_micros, failed = out.error.is_some(), "headless run complete");
    Ok(out)
}

fn chat_message(role: &str, content: &str, model: Option<String>, agent_id: Option<&str>) -> ChatMessage {
    ChatMessage {
        id: None,
        role: role.to_string(),
        content: content.to_string(),
        timestamp: Timestamp::now(),
        model,
        provider: None,
        agent_id: agent_id.map(str::to_string),
        tool_calls: None,
        reasoning: None,
        intent: None,
        subject: None,
        reasoning_details: None,
        parts: None,
    }
}

/// The tools that write a page. `edit_page` asked to edit with no page bound
/// answers `needs_binding` and succeeds without writing anything.
const PAGE_WRITE_TOOLS: &[&str] = &["create_page", "edit_page", "revise_article"];

/// The page a tool call wrote, if it wrote one. Every page tool answers with
/// the `page_id` it acted on.
fn page_written<'a>(tool: &str, success: bool, result: &'a serde_json::Value) -> Option<&'a str> {
    if !success || !PAGE_WRITE_TOOLS.contains(&tool) || result.get("needs_binding").is_some() {
        return None;
    }
    result.get("page_id")?.as_str()
}

/// What a wake nobody asked for (a clock, a poll) runs on.
pub const RUN_INSTRUCTION: &str = "Run your action instruction now.";

/// Ends the conversation on this wake's own turn, after whatever the chat
/// already holds: the person's message when they sent one, the instruction
/// otherwise. Without a turn of its own a run on a chat-linked applet ended on
/// the applet's last reply, which the model reads as text to continue, and a
/// message sent to it was never read at all.
///
/// Providers (Bedrock, zai) also require the first non-system message to be a
/// user message, and a compacted chat's post-checkpoint tail can begin with
/// an assistant turn, so that case opens with the instruction.
///
/// A chat can also already end on a user turn: the person's message is posted
/// before the run, so a run that failed or hit its ceiling leaves it
/// unanswered. Some providers reject two user turns in a row, so this wake's
/// words join that turn instead of following it.
fn add_wake_turn(llm_messages: &mut Vec<serde_json::Value>, message: Option<&str>) {
    let role = |m: &serde_json::Value| m.get("role").and_then(|r| r.as_str()).map(str::to_owned);
    if let Some(i) = llm_messages.iter().position(|m| role(m).as_deref() != Some("system")) {
        if role(&llm_messages[i]).as_deref() != Some("user") {
            llm_messages.insert(i, serde_json::json!({ "role": "user", "content": RUN_INSTRUCTION }));
        }
    }
    let text = message.unwrap_or(RUN_INSTRUCTION);
    if let Some(last) = llm_messages.last_mut() {
        if role(last).as_deref() == Some("user") {
            if let Some(held) = last.get("content").and_then(|c| c.as_str()) {
                last["content"] = serde_json::Value::String(format!("{held}\n\n{text}"));
                return;
            }
        }
    }
    llm_messages.push(serde_json::json!({ "role": "user", "content": text }));
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
            // absent-ok: a stored tool-call record that no longer decodes is
            // replayed without its calls; the message text still reads.
            let tool_calls = tool_calls_raw.and_then(|tc| serde_json::from_value(tc).ok());
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
    fn only_a_successful_page_write_counts() {
        assert_eq!(page_written("create_page", true, &json!({ "page_id": "page_1" })), Some("page_1"));
        assert_eq!(page_written("revise_article", true, &json!({ "page_id": "page_2" })), Some("page_2"));
        assert_eq!(page_written("create_page", false, &json!({ "page_id": "page_1" })), None, "a failed write");
        assert_eq!(page_written("sql_query", true, &json!({ "page_id": "x" })), None, "not a page tool");
        assert_eq!(
            page_written("edit_page", true, &json!({ "needs_binding": true })),
            None,
            "no page was bound, so nothing was written"
        );
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
    fn an_unanswered_message_joins_the_next_wake_instead_of_doubling_the_turn() {
        let mut m = vec![
            json!({"role": "system", "content": "s"}),
            json!({"role": "user", "content": "Make it weekly"}),
        ];
        add_wake_turn(&mut m, None);
        assert_eq!(m.len(), 2, "no second user turn in a row");
        assert_eq!(m[1]["content"], format!("Make it weekly\n\n{RUN_INSTRUCTION}"));
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

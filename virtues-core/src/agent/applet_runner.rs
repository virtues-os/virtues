//! Agent loop runner for actions with an `agent` field set.
//!
//! This runs the LLM agent loop for an action. It is invoked by the unified
//! `crate::applet_runner::run_applet` after any subprocess phase has completed.
//! Trigger validation, condition evaluation, concurrency gating, and run-row
//! lifecycle are all handled upstream — this function is pure execution.

use chrono::Utc;
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
}

/// Run one pass of the LLM agent loop for an action.
///
/// `prompt` is the action's `agent` field (the instruction). `context` is an
/// optional dynamic context block — typically the result summary from a
/// subprocess phase that ran immediately before. Concurrency/condition gating
/// is handled upstream by `crate::applet_runner::run_applet`.
pub async fn run_agent_loop(
    pool: &PgPool,
    yjs_state: &YjsState,
    action: &Applet,
    prompt: &str,
    context: Option<&str>,
) -> Result<AgentLoopResult> {
    let applet_id = &action.id;

    // Extract optional chat_id and model from config
    let chat_id = action.config.get("chat_id").and_then(|v| v.as_str()).map(|s| s.to_string());
    let model_override = action.config.get("model").and_then(|v| v.as_str()).map(|s| s.to_string());

    // Build system prompt (with memory if present)
    let system_prompt =
        build_applet_system_prompt(pool, prompt, context, action.memory.as_deref()).await;

    // 3. Load compacted message history (only if chat is linked)
    let messages = if let Some(cid) = &chat_id {
        load_chat_messages(pool, cid).await?
    } else {
        Vec::new()
    };
    let mut llm_messages = build_context_for_llm(&messages, None, 0, Some(&system_prompt));

    // Providers (Bedrock, zai) require the first non-system message to be a
    // user message. Two ways that breaks here: a folder applet with no linked
    // chat has an empty history (system prompt only), and a compacted chat's
    // post-checkpoint tail can begin with an assistant turn. Insert a
    // synthetic kickoff turn right after the system message when needed.
    let first_non_system = llm_messages
        .iter()
        .position(|m| m.get("role").and_then(|r| r.as_str()) != Some("system"));
    let needs_kickoff = match first_non_system {
        None => true, // system-only (or empty) conversation
        Some(i) => llm_messages[i].get("role").and_then(|r| r.as_str()) != Some("user"),
    };
    if needs_kickoff {
        let at = first_non_system.unwrap_or(llm_messages.len());
        llm_messages.insert(
            at,
            serde_json::json!({
                "role": "user",
                "content": "Run your action instruction now.",
            }),
        );
    }

    // 4. Get tools and model
    let tools = crate::tools::get_tools_for_applet();
    let model = if let Some(m) = &model_override {
        m.clone()
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

    let agent_loop = crate::agent::AgentLoop::new_with_yjs(pool.clone(), yjs_state.clone());

    tracing::info!(applet_id, model = %model, chat_id = ?chat_id, "Starting action run");

    // 7. Consume the event stream
    use futures::StreamExt;
    let mut stream = agent_loop.run(
        model.clone(),
        llm_messages,
        tools,
        tool_context,
        None,
        None,
    );

    let mut assistant_content = String::new();
    let mut step_count: u32 = 0;

    while let Some(event) = stream.next().await {
        match event {
            crate::agent::AgentEvent::TextDelta { content } => {
                assistant_content.push_str(&content);
            }
            crate::agent::AgentEvent::StepComplete { step, .. } => {
                step_count = step;
            }
            crate::agent::AgentEvent::Error { message, .. } => {
                tracing::error!(applet_id, error = %message, "Applet run error");
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
                        thought_signature: None,
                        parts: None,
                    };
                    let _ = append_message(pool, cid.clone(), error_msg).await;
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
                thought_signature: None,
                parts: None,
            };
            let _ = append_message(pool, cid.clone(), msg).await;
        }
    }

    tracing::info!(applet_id, steps = step_count, "Applet run complete");

    Ok(AgentLoopResult {
        applet_id: applet_id.to_string(),
        chat_id,
        steps: step_count,
        message: if assistant_content.is_empty() {
            None
        } else {
            Some(assistant_content)
        },
    })
}

// ============================================================================
// Helpers
// ============================================================================

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
        Option<String>,
        Timestamp,
    )>(
        r#"
        SELECT id, role, content, model, provider, agent_id, reasoning, tool_calls, subject, thought_signature, created_at
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
        .map(|(id, role, content, model, provider, agent_id, reasoning, tool_calls_raw, subject, thought_signature, timestamp)| {
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
                thought_signature,
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

    let now = Utc::now();
    let datetime = now.format("%A, %B %-d, %Y at %-I:%M %p UTC").to_string();

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
                "\n\n<memory>\nYour persistent memory from prior runs. You can update this with the update_action_memory tool.\n{}\n</memory>",
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

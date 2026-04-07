//! Action runner
//!
//! Executes an action: load instruction → activation gate → build prompt → run AgentLoop → save messages.
//! No scheduling logic — that's handled by the Scheduler. This is pure execution.

use chrono::Utc;
use serde::Serialize;
use sqlx::SqlitePool;

use crate::api::action_events::ActionBroadcastState;
use crate::api::chats::{append_message, ChatMessage};
use crate::api::code::{execute_code, ExecuteCodeRequest};
use crate::api::compaction::build_context_for_llm;
use crate::error::Result;
use crate::scheduler::actions::Action;
use crate::server::yjs::YjsState;
use crate::types::Timestamp;

/// Result of a single action run
#[derive(Debug, Serialize)]
pub struct ActionRunResult {
    pub action_id: String,
    pub chat_id: Option<String>,
    pub status: ActionRunStatus,
    pub steps: u32,
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
pub enum ActionRunStatus {
    Completed,
    ActivationSkipped,
    ActivationError(String),
    Error(String),
}

/// Run an action execution cycle.
///
/// The scheduler is responsible for: checking if the task is due, creating a run,
/// preventing overlapping runs, and marking the run complete. This function only
/// handles the actual action execution.
pub async fn run_action(
    pool: &SqlitePool,
    yjs_state: &YjsState,
    action: &Action,
    force_run: bool,
    broadcast: Option<&ActionBroadcastState>,
    context: Option<&str>,
) -> Result<ActionRunResult> {
    let action_id = &action.id;
    let instruction = action.instruction.as_deref().unwrap_or("");

    // Extract optional chat_id and model from config
    let chat_id = action.config.get("chat_id").and_then(|v| v.as_str()).map(|s| s.to_string());
    let model_override = action.config.get("model").and_then(|v| v.as_str()).map(|s| s.to_string());

    // 1. Run activation gate (if set)
    let activation_output = if let Some(code) = &action.activation_code {
        match run_activation_gate(code).await {
            Ok(Some(output)) => {
                tracing::info!(action_id, output = %output, "Activation gate passed");
                Some(output)
            }
            Ok(None) => {
                if force_run {
                    tracing::info!(action_id, "Activation gate returned falsy but force_run=true, proceeding");
                    None
                } else {
                    tracing::info!(action_id, "Activation gate returned falsy, skipping run");
                    return Ok(ActionRunResult {
                        action_id: action_id.to_string(),
                        chat_id,
                        status: ActionRunStatus::ActivationSkipped,
                        steps: 0,
                        message: Some("Activation gate returned falsy".to_string()),
                    });
                }
            }
            Err(e) => {
                if force_run {
                    tracing::warn!(action_id, error = %e, "Activation script error on force_run, proceeding");
                    None
                } else {
                    tracing::error!(action_id, error = %e, "Activation script error");
                    // Log error to chat if linked
                    if let Some(cid) = &chat_id {
                        let msg = ChatMessage {
                            id: None,
                            role: "system".to_string(),
                            content: format!("[System: Activation script error: {}]", e),
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
                        let _ = append_message(pool, cid.clone(), msg).await;
                    }
                    return Ok(ActionRunResult {
                        action_id: action_id.to_string(),
                        chat_id,
                        status: ActionRunStatus::ActivationError(e.to_string()),
                        steps: 0,
                        message: None,
                    });
                }
            }
        }
    } else {
        None
    };

    // 2. Build system prompt (with memory if present)
    let system_prompt = build_action_system_prompt(
        pool, instruction, activation_output.as_deref(), context, action.memory.as_deref(),
    ).await;

    // 3. Load compacted message history (only if chat is linked)
    let messages = if let Some(cid) = &chat_id {
        load_chat_messages(pool, cid).await?
    } else {
        Vec::new()
    };
    let llm_messages = build_context_for_llm(&messages, None, 0, Some(&system_prompt));

    // 4. Get tools and model
    let tools = crate::tools::get_all_tool_definitions_for_llm();
    let model = if let Some(m) = &model_override {
        m.clone()
    } else {
        crate::api::assistant_profile::get_background_model(pool).await
            .unwrap_or_else(|_| virtues_registry::models::default_model_for_slot(
                virtues_registry::models::ModelSlot::Lite
            ).to_string())
    };

    // 5. Get tollbooth config
    let tollbooth_url = std::env::var("TOLLBOOTH_URL")
        .unwrap_or_else(|_| "http://localhost:9002".to_string());
    let tollbooth_secret = match std::env::var("TOLLBOOTH_INTERNAL_SECRET") {
        Ok(s) => s,
        Err(_) => {
            tracing::error!(action_id, "TOLLBOOTH_INTERNAL_SECRET not set, cannot run action");
            return Ok(ActionRunResult {
                action_id: action_id.to_string(),
                chat_id,
                status: ActionRunStatus::Error("TOLLBOOTH_INTERNAL_SECRET not set".to_string()),
                steps: 0,
                message: None,
            });
        }
    };
    let tollbooth_user_id = std::env::var("TOLLBOOTH_USER_ID")
        .unwrap_or_else(|_| "system".to_string());

    // 6. Create and run AgentLoop
    let tool_context = crate::tools::ToolContext {
        page_id: None,
        user_id: Some("system".to_string()),
        space_id: None,
        chat_id: chat_id.clone(),
        action_id: Some(action_id.to_string()),
    };

    let agent_loop = crate::agent::AgentLoop::new_with_yjs(
        pool.clone(),
        tollbooth_url,
        tollbooth_user_id,
        tollbooth_secret,
        yjs_state.clone(),
    );

    tracing::info!(action_id, model = %model, chat_id = ?chat_id, "Starting action run");

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
        // Broadcast to SSE subscribers if a chat is linked
        if let (Some(bc), Some(cid)) = (broadcast, &chat_id) {
            let _ = bc.broadcast(cid, event.clone());
        }

        match event {
            crate::agent::AgentEvent::TextDelta { content } => {
                assistant_content.push_str(&content);
            }
            crate::agent::AgentEvent::StepComplete { step, .. } => {
                step_count = step;
            }
            crate::agent::AgentEvent::Error { message, .. } => {
                tracing::error!(action_id, error = %message, "Action run error");
                // Log error to chat if linked
                if let Some(cid) = &chat_id {
                    let error_msg = ChatMessage {
                        id: None,
                        role: "system".to_string(),
                        content: format!("[System: Action run error: {}]", message),
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

    // 9. Clean up broadcast channel
    if let (Some(bc), Some(cid)) = (broadcast, &chat_id) {
        bc.cleanup(cid);
    }

    tracing::info!(action_id, steps = step_count, "Action run complete");

    Ok(ActionRunResult {
        action_id: action_id.to_string(),
        chat_id,
        status: ActionRunStatus::Completed,
        steps: step_count,
        message: if assistant_content.is_empty() { None } else { Some(assistant_content) },
    })
}

// ============================================================================
// Helpers
// ============================================================================

/// Run the activation gate (Python script in sandbox).
async fn run_activation_gate(code: &str) -> std::result::Result<Option<String>, String> {
    let request = ExecuteCodeRequest {
        code: code.to_string(),
        timeout: 30,
    };

    let response = execute_code(request).await;

    if !response.success {
        return Err(response.error.unwrap_or_else(|| "Unknown activation error".to_string()));
    }

    let stdout = response.stdout.trim().to_string();

    if stdout.is_empty() || stdout == "false" || stdout == "0" || stdout == "False" {
        Ok(None)
    } else {
        Ok(Some(stdout))
    }
}

/// Load chat messages for context building.
async fn load_chat_messages(pool: &SqlitePool, chat_id: &str) -> Result<Vec<ChatMessage>> {
    let rows = sqlx::query_as::<_, (String, String, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, String)>(
        r#"
        SELECT id, role, content, model, provider, agent_id, reasoning, tool_calls, subject, thought_signature, created_at
        FROM app_chat_messages
        WHERE chat_id = ?
        ORDER BY sequence_num ASC
        "#,
    )
    .bind(chat_id)
    .fetch_all(pool)
    .await?;

    let messages = rows
        .into_iter()
        .map(|(id, role, content, model, provider, agent_id, reasoning, tool_calls_raw, subject, thought_signature, created_at)| {
            let tool_calls = tool_calls_raw
                .and_then(|tc| serde_json::from_str(&tc).ok());
            let timestamp = created_at.parse::<Timestamp>().unwrap_or_else(|_| Timestamp::now());

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
/// `context` is optional dynamic data injected by the scheduler each run
/// (e.g., the current hour's ontology data for the dayline hourly action).
async fn build_action_system_prompt(
    pool: &SqlitePool,
    instruction: &str,
    activation_output: Option<&str>,
    context: Option<&str>,
    memory: Option<&str>,
) -> String {
    let assistant_name = crate::api::assistant_profile::get_assistant_name(pool)
        .await
        .unwrap_or_else(|_| "Assistant".to_string());
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

    if let Some(output) = activation_output {
        prompt.push_str(&format!(
            "\n\n<trigger_context>\nActivation script output: {}\n</trigger_context>",
            output
        ));
    }

    prompt
}

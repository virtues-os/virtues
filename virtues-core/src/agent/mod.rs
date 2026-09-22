//! Agent Module
//!
//! Production-ready agentic loop for LLM tool execution.
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │                        AgentLoop                             │
//! │  ┌─────────────────────────────────────────────────────────┐ │
//! │  │  1. Call LLM (stream.rs)                                │ │
//! │  │  2. Stream text → client                                │ │
//! │  │  3. If tool_calls:                                      │ │
//! │  │     a. Execute tools (executor.rs)                      │ │
//! │  │     b. Stream tool results → client                     │ │
//! │  │     c. Append to messages                               │ │
//! │  │     d. GOTO 1                                           │ │
//! │  │  4. Done                                                │ │
//! │  └─────────────────────────────────────────────────────────┘ │
//! └──────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use virtues::agent::{AgentLoop, AgentConfig};
//!
//! let agent = AgentLoop::new(pool);
//! let stream = agent.run(messages, tools, context);
//!
//! while let Some(event) = stream.next().await {
//!     // Handle AgentEvent
//! }
//! ```

pub mod applet_runner;
pub mod executor;
pub mod guard;
pub mod subagent;
pub mod prompt;
pub mod prompt_blocks;
pub mod protocol;
pub mod stream;

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use async_stream::stream;
use futures::Stream;
use serde_json::Value;
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;

use crate::tools::{ToolContext, ToolExecutor};
use crate::server::yjs::YjsState;

pub use executor::{ExecutorConfig, ToolExecutionError, ToolExecutionResult};
pub use protocol::{AgentEvent, ErrorCode, FinishReason, StepReason};
pub use stream::{LlmConfig, LlmStreamResult, StreamError, ToolCall, TokenUsage};

/// Configuration for the AgentLoop
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Maximum number of LLM calls (steps) in a single run
    pub max_steps: u32,
    /// Timeout for individual tool execution
    pub tool_timeout: Duration,
    /// Whether to execute multiple tools in parallel
    pub parallel_tools: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_steps: 20,
            tool_timeout: Duration::from_secs(30),
            parallel_tools: true,
        }
    }
}

/// What one turn may spend before the loop stops starting steps. Beside
/// `AgentConfig` rather than in it so the config stays a plain struct every
/// caller builds by hand; a run with no budgets is what every caller had.
#[derive(Debug, Clone, Copy, Default)]
pub struct TurnBudget {
    /// Most the turn may spend, in micro-dollars as the gateway reports
    /// cost, checked between steps. `None` = unbounded. A step whose usage
    /// carries no cost (a BYO endpoint) counts as free here — the ceiling is
    /// for the bill this box can see.
    pub max_cost_micros: Option<i64>,
    /// Longest the whole turn may run, checked between steps (a step in
    /// flight is not cut; the next one is not started).
    pub max_wall_clock: Option<Duration>,
}

/// The main agent loop orchestrator
///
/// Handles the complete cycle of:
/// 1. Calling the LLM
/// 2. Streaming responses to the client
/// 3. Executing tool calls
/// 4. Continuing until completion
pub struct AgentLoop {
    _pool: Arc<PgPool>,
    llm_config: LlmConfig,
    tool_executor: ToolExecutor,
    config: AgentConfig,
    budget: TurnBudget,
}

impl AgentLoop {
    /// Create a new AgentLoop. All egress (LLM + tools) goes through
    /// `BearerClient`, which sources the virtues-api URL + bearer itself.
    pub fn new(pool: PgPool) -> Self {
        let pool = Arc::new(pool);
        Self {
            tool_executor: ToolExecutor::new((*pool).clone()),
            llm_config: LlmConfig {
                client: crate::virtues_api::client::BearerClient::from_env((*pool).clone()),
            },
            _pool: pool,
            config: AgentConfig::default(),
            budget: TurnBudget::default(),
        }
    }

    /// Create a new AgentLoop with YjsState for real-time page editing
    pub fn new_with_yjs(pool: PgPool, yjs_state: YjsState) -> Self {
        let pool = Arc::new(pool);
        Self {
            tool_executor: ToolExecutor::new_with_yjs((*pool).clone(), yjs_state),
            llm_config: LlmConfig {
                client: crate::virtues_api::client::BearerClient::from_env((*pool).clone()),
            },
            _pool: pool,
            config: AgentConfig::default(),
            budget: TurnBudget::default(),
        }
    }

    /// Cap what one turn may spend. See `TurnBudget`.
    pub fn with_budget(mut self, budget: TurnBudget) -> Self {
        self.budget = budget;
        self
    }

    /// Create with custom configuration
    pub fn with_config(mut self, config: AgentConfig) -> Self {
        self.config = config;
        self
    }

    /// Run the agent loop
    ///
    /// Returns a stream of AgentEvents that can be forwarded to the client.
    /// Pass a CancellationToken to allow stopping the loop early.
    pub fn run(
        &self,
        model: String,
        initial_messages: Vec<Value>,
        tools: Vec<Value>,
        context: ToolContext,
        cancel_token: Option<CancellationToken>,
    ) -> Pin<Box<dyn Stream<Item = AgentEvent> + Send + '_>> {
        let llm_config = self.llm_config.clone();
        let pool = self._pool.clone();
        let tool_executor = self.tool_executor.clone();
        let config = self.config.clone();
        let budget = self.budget;
        let executor_config = ExecutorConfig {
            tool_timeout: config.tool_timeout,
            parallel: config.parallel_tools,
        };

        Box::pin(stream! {
            let mut messages = initial_messages;
            let mut step: u32 = 0;
            // How this turn ends. Assigned at each break; reported once, below.
            let mut finish = protocol::FinishReason::EndTurn;
            // Per turn: which tools have failed, and how — see `guard`.
            let mut repeat_guard = guard::RepeatGuard::new();
            // Per turn: what it has cost and how long it has run, for the
            // budget. Checked where the step ceiling is.
            let turn_started = std::time::Instant::now();
            let mut spent_micros: i64 = 0;

            // Ask the model to RETURN its thinking. Claude 5 omits the text
            // unless told `display: summarized`; Gemini needs
            // `includeThoughts`. The catalog carries the right options per
            // family (`ReasoningFacts::display_options`), and a BYO endpoint,
            // which never sees the gateway's providerOptions, gets none.
            let display_options: Option<Value> = if crate::api::settings_byo::byo_is_active(&pool).await {
                None
            } else {
                crate::api::model_catalog::reasoning_facts(&model)
                    .map(|f| f.display_options)
                    .filter(|v| v.as_object().is_some_and(|o| !o.is_empty()))
            };

            // Emit loop started
            yield AgentEvent::LoopStarted {
                max_steps: config.max_steps,
            };

            loop {
                step += 1;

                // Check for cancellation at start of each step
                if let Some(ref token) = cancel_token {
                    if token.is_cancelled() {
                        tracing::info!(step, "Agent loop cancelled by user");
                        finish = protocol::FinishReason::Cancelled;
                        break;
                    }
                }

                // Check max steps.
                //
                // Not an `error` event: the SDK treats one as fatal — it throws
                // out of the stream, so everything after it is discarded and a
                // turn that simply ran long renders as a failed request with a
                // Retry button. The ceiling is an ENDING, and it says so
                // through the finish reason, which the row and the notice both
                // read.
                if step > config.max_steps {
                    tracing::info!(step, max_steps = config.max_steps, "agent loop hit its step ceiling");
                    finish = protocol::FinishReason::MaxSteps;
                    break;
                }
                // The budgets. Same shape as the ceiling above: a finish
                // reason, not an error — the turn ended for a reason the
                // person can be told, and what streamed before it stands.
                let over_cost = budget
                    .max_cost_micros
                    .is_some_and(|cap| spent_micros >= cap);
                let over_clock = budget
                    .max_wall_clock
                    .is_some_and(|cap| turn_started.elapsed() >= cap);
                if over_cost || over_clock {
                    tracing::warn!(
                        step,
                        spent_micros,
                        elapsed_secs = turn_started.elapsed().as_secs(),
                        over_cost,
                        over_clock,
                        "turn stopped at its budget"
                    );
                    finish = protocol::FinishReason::BudgetExceeded;
                    break;
                }

                tracing::info!(step, "Agent loop step");

                let provider_options = display_options.clone();

                // Stream events through a channel for incremental delivery.
                // Previously events were collected into a Vec and yielded in a
                // burst after the entire LLM response completed. Using a channel
                // lets each text-delta reach the client as it arrives.
                let (ev_tx, mut ev_rx) = tokio::sync::mpsc::unbounded_channel::<AgentEvent>();

                // Clone data needed for the spawned streaming task
                let llm_cfg = llm_config.clone();
                let mdl = model.clone();
                let msgs = messages.clone();
                let tls = tools.clone();

                // Spawn streaming in a separate task so events are sent
                // through the channel immediately (not buffered until completion)
                let stream_handle = tokio::spawn(async move {
                    stream::stream_llm_response(
                        &llm_cfg,
                        &mdl,
                        &msgs,
                        &tls,
                        provider_options,
                        // The temperature chat has always run at. It lived in
                        // the proxy as a default nobody on the box could see;
                        // now the box says it, and the proxy default can go.
                        Some(0.7),
                        None, // agent loop: no fixed output cap
                        |event| {
                            let _ = ev_tx.send(event);
                        },
                    )
                    .await
                });

                // Yield events incrementally as they arrive from the stream,
                // and stop pulling the moment the person presses Stop.
                //
                // Cancellation used to be checked only at the START of a step
                // and after tool execution, so Stop pressed two seconds into a
                // three-thousand-token answer stopped nothing: the gateway call
                // ran to completion, was billed in full, and every remaining
                // delta was still accumulated — the row then saved as
                // "cancelled" held the entire reply. Aborting the task drops
                // the response body, which closes the connection to the
                // provider and ends the generation.
                //
                // Checked between events rather than with a `select!`, because
                // `yield` inside a select arm does not survive the
                // `async_stream` macro. Deltas arrive continuously while a
                // model is generating, so "after the next event" is as
                // immediate as it needs to be.
                let mut cancelled_mid_stream = false;
                while let Some(event) = ev_rx.recv().await {
                    yield event;
                    if cancel_token.as_ref().is_some_and(|t| t.is_cancelled()) {
                        tracing::info!(step, "Stop pressed mid-stream — dropping the provider call");
                        stream_handle.abort();
                        cancelled_mid_stream = true;
                        break;
                    }
                }

                if cancelled_mid_stream {
                    // What streamed before the abort has already reached the
                    // caller and is what gets saved. No error event: the person
                    // asked for this ending, and an error card would claim
                    // something went wrong.
                    finish = protocol::FinishReason::Cancelled;
                    break;
                }

                // Get the streaming result after the task completes
                let result = match stream_handle.await {
                    Ok(inner) => inner,
                    Err(e) => Err(StreamError::Connection(format!("Stream task panicked: {}", e))),
                };

                let result = match result {
                    Ok(r) => r,
                    Err(e) => {
                        // An interrupted stream is not an LLM error: the model
                        // was mid-sentence when the bytes stopped. The text
                        // that streamed is already with the caller; this event
                        // is what stops it being saved as the whole answer.
                        let code = match e {
                            stream::StreamError::Interrupted(_) => ErrorCode::Interrupted,
                            _ => ErrorCode::LlmError,
                        };
                        // The one ending that IS an error event: the reply
                        // stopped for a reason outside the turn, and the person
                        // needs the card and the retry.
                        tracing::error!(step, error = %e, "the model stream failed");
                        finish = protocol::FinishReason::Error;
                        yield AgentEvent::error(e.to_string(), Some(code), false);
                        break;
                    }
                };

                if let Some(cost) = result.usage.as_ref().and_then(|u| u.cost_micros) {
                    spent_micros += cost;
                }

                // The step's reasoning blocks, for the row and for the echo
                // below. Before the completion check, so a final step's
                // thinking is stored too.
                if !result.reasoning_details.is_empty() {
                    yield AgentEvent::ReasoningDetails { details: result.reasoning_details.clone() };
                }

                // Check if we're done (no tool calls)
                if result.tool_calls.is_empty() {
                    if result.finish_reason == StepReason::MaxTokens {
                        // `finish_reason: length` used to be mapped and then
                        // ignored, so a reply the model never finished read
                        // as one it did. It is still not an `error` event —
                        // see the ceiling above — it is how this turn ended.
                        finish = protocol::FinishReason::OutputLimit;
                    }
                    yield AgentEvent::step_complete(step, result.finish_reason);
                    break;
                }

                // We have tool calls - emit step complete
                yield AgentEvent::step_complete(step, StepReason::ToolCalls);

                // Execute tools
                tracing::info!(
                    count = result.tool_calls.len(),
                    "Executing tool calls"
                );

                // A call identical to one that already failed this turn, or
                // a tool that has failed several times in a row, is not run:
                // it comes back as a failed result saying so, and the model
                // has to change something. The step ceiling still bounds the
                // turn; this bounds how much of it is spent asking the same
                // question.
                let (admitted, refused) = repeat_guard.admit(&result.tool_calls);
                let mut tool_results = executor::execute_tools(
                    &tool_executor,
                    &admitted,
                    &context,
                    &executor_config,
                )
                .await;
                tool_results.extend(refused);
                repeat_guard.record(&result.tool_calls, &tool_results);

                // Emit tool results, checking for awaiting_user condition
                let mut awaiting_user = false;
                for tool_result in &tool_results {
                    // Check if tool needs user action (e.g., binding a page)
                    if let Ok(result) = &tool_result.result {
                        if let Some(data) = result.data.as_object() {
                            if data.get("needs_binding").and_then(|v| v.as_bool()).unwrap_or(false) {
                                awaiting_user = true;
                            }
                        }
                    }
                    yield tool_result.to_event();
                }

                // If a tool needs user action, stop the loop early
                if awaiting_user {
                    tracing::info!(step, "Tool requires user action, pausing loop");
                    finish = protocol::FinishReason::AwaitingUser;
                    break;
                }

                // Check for cancellation after tool execution
                if let Some(ref token) = cancel_token {
                    if token.is_cancelled() {
                        tracing::info!(step, "Agent loop cancelled after tool execution");
                        finish = protocol::FinishReason::Cancelled;
                        break;
                    }
                }

                // Build messages for next iteration
                // 1. Add assistant message with tool calls
                messages.push(executor::build_assistant_tool_message(
                    &result.content,
                    &result.tool_calls,
                    &result.reasoning_details,
                ));

                // 2. Add tool result messages
                for tool_result in &tool_results {
                    messages.push(executor::build_tool_result_message(
                        &tool_result.tool_call_id,
                        &tool_result.to_llm_content(),
                    ));
                }

                // 2b. Hand over any media the tools returned, so a file the
                // model found counts for as much as one the user pasted.
                //
                // Gated on the catalog's capability flag rather than a mime
                // check here: whether an image can be sent is a fact about the
                // model, and the Chat slot is user-overridable to anything the
                // gateway carries. `None` means the catalog is cold and we do
                // not know — treated as cannot, because guessing wrong fails
                // the whole request rather than one attachment.
                let attachments: Vec<(String, crate::tools::ToolAttachment)> = tool_results
                    .iter()
                    .filter_map(|tr| tr.result.as_ref().ok().map(|r| (tr.tool_name.clone(), r)))
                    .flat_map(|(name, r)| {
                        r.attachments.iter().map(move |a| (name.clone(), a.clone()))
                    })
                    .collect();

                if !attachments.is_empty() {
                    match crate::api::model_catalog::supports_vision(&model) {
                        Some(true) => {
                            if let Some(msg) = executor::build_attachment_message(&attachments) {
                                tracing::info!(
                                    count = attachments.len(),
                                    "Attaching tool media to next turn"
                                );
                                messages.push(msg);
                            }
                        }
                        // Say it in the transcript rather than dropping the
                        // media silently — the model must be able to tell the
                        // user it cannot see, instead of reporting an absence.
                        other => {
                            let why = if other == Some(false) {
                                format!("{model} cannot read images")
                            } else {
                                format!("image support for {model} is unknown on this box")
                            };
                            tracing::warn!(model = %model, "Dropping tool media: {}", why);
                            messages.push(serde_json::json!({
                                "role": "user",
                                "content": format!(
                                    "[System: the tool returned {} file(s) to look at, but they were not attached because {}. Tell the user you cannot see the file rather than guessing at its contents.]",
                                    attachments.len(),
                                    why
                                )
                            }));
                        }
                    }
                }

                // 3. Inject turn limit warning when running low on steps
                let steps_remaining = config.max_steps.saturating_sub(step);
                if steps_remaining <= 3 && steps_remaining > 0 {
                    // "Steps" — model calls — not "tool calls": one step can
                    // carry several calls, and a model told it had two calls
                    // left when it had two steps rationed the wrong thing.
                    let warning = format!(
                        "[System: {} step{} (model call{}) remaining in this turn. Finish, or say where you got to and what is left.]",
                        steps_remaining,
                        if steps_remaining == 1 { "" } else { "s" },
                        if steps_remaining == 1 { "" } else { "s" }
                    );
                    messages.push(serde_json::json!({
                        "role": "user",
                        "content": warning
                    }));
                    tracing::debug!(steps_remaining, "Injected turn limit warning");
                }

                // Continue loop for next LLM call
            }

            // How it ended, once, at the end. A trailing unconditional
            // `done()` used to follow every break, so the loop's verdict was
            // always `EndTurn` and every caller that matched on it — the
            // finish reason on the wire, the row's subject — was matching on
            // dead arms.
            yield AgentEvent::done_with_reason(step, finish);
        })
    }
}

impl std::fmt::Debug for AgentLoop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentLoop")
            .field("config", &self.config)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AgentConfig::default();
        assert_eq!(config.max_steps, 20);
        assert_eq!(config.tool_timeout, Duration::from_secs(30));
        assert!(config.parallel_tools);
    }
}

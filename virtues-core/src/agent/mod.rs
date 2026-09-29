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
//! let mut stream = agent.run(model, messages, tools, context, cancel_token);
//!
//! while let Some(event) = stream.next().await {
//!     // Handle AgentEvent
//! }
//! ```

pub mod applet_runner;
pub mod caps;
pub mod executor;
pub mod guard;
pub mod subagent;
pub mod prompt;
pub mod prompt_blocks;
pub mod protocol;
pub mod stream;
mod turn;

use std::pin::Pin;
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
pub use stream::{LlmConfig, LlmStreamResult, StepOptions, StreamError, ToolCall, TokenUsage};
pub use crate::virtues_api::request::Thinking;

/// Configuration for the AgentLoop
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Maximum number of LLM calls (steps) in a single run
    pub max_steps: u32,
    /// Timeout for individual tool execution
    pub tool_timeout: Duration,
    /// Whether to execute multiple tools in parallel
    pub parallel_tools: bool,
    /// How hard the model thinks on every step, turned into the request by
    /// `reasoning_for` against what the catalog says the model accepts.
    /// `Default` sends nothing and leaves the provider's own default, which
    /// for most is its highest-but-one effort — and effort governs how many
    /// tool calls a model makes, not only how long it thinks.
    pub thinking: Thinking,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_steps: 20,
            tool_timeout: Duration::from_secs(30),
            parallel_tools: true,
            thinking: Thinking::Default,
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
    /// Most calls one tool may take in the turn, by tool name. Empty: none
    /// is capped. See `caps`.
    pub tool_caps: &'static [(&'static str, u32)],
}

/// The main agent loop orchestrator
///
/// Handles the complete cycle of:
/// 1. Calling the LLM
/// 2. Streaming responses to the client
/// 3. Executing tool calls
/// 4. Continuing until completion
pub struct AgentLoop {
    pool: PgPool,
    llm_config: LlmConfig,
    tool_executor: ToolExecutor,
    config: AgentConfig,
    budget: TurnBudget,
}

impl AgentLoop {
    /// Create a new AgentLoop. All egress (LLM + tools) goes through
    /// `BearerClient`, which sources the virtues-api URL + bearer itself.
    pub fn new(pool: PgPool) -> Self {
        Self::with_executor(pool.clone(), ToolExecutor::new(pool))
    }

    /// Create a new AgentLoop with YjsState for real-time page editing
    pub fn new_with_yjs(pool: PgPool, yjs_state: YjsState) -> Self {
        Self::with_executor(pool.clone(), ToolExecutor::new_with_yjs(pool, yjs_state))
    }

    fn with_executor(pool: PgPool, tool_executor: ToolExecutor) -> Self {
        Self {
            llm_config: LlmConfig {
                client: crate::virtues_api::client::BearerClient::from_env(pool.clone()),
            },
            pool,
            tool_executor,
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
        let pool = self.pool.clone();
        let tool_executor = self.tool_executor.clone();
        let config = self.config.clone();
        let budget = self.budget;
        let executor_config = ExecutorConfig {
            tool_timeout: config.tool_timeout,
            parallel: config.parallel_tools,
        };

        Box::pin(stream! {
            let mut messages = initial_messages;
            let mut turn = turn::TurnState::new(&budget);
            // How this turn ends. Assigned at each break; reported once, below.
            let finish: FinishReason;

            let byo = crate::api::settings_byo::byo_is_active(&pool).await;
            // The model does not change mid-turn, so neither do these.
            let facts = crate::api::model_catalog::reasoning_facts(&model);
            let gateway_options = turn::gateway_options(facts.as_ref(), byo);
            // An owner's own endpoint gets no reasoning field from the loop: it
            // never did before chat asked for low effort, and an endpoint whose
            // model does not reason can refuse `reasoning_effort` outright,
            // which would fail every turn.
            let (reasoning, reasoning_effort) = if byo {
                (None, None)
            } else {
                crate::virtues_api::request::reasoning_for(config.thinking, facts.as_ref(), byo)
            };

            yield AgentEvent::LoopStarted { max_steps: config.max_steps };

            loop {
                turn.step += 1;
                let step = turn.step;
                if cancelled(&cancel_token) {
                    tracing::info!(step, "Agent loop cancelled by user");
                    finish = FinishReason::Cancelled;
                    break;
                }
                // Reached only with a ceiling of zero: otherwise the last
                // step ends the turn whatever it returns. A finish reason, not
                // an `error` event, which the SDK treats as fatal.
                if step > config.max_steps {
                    tracing::info!(step, max_steps = config.max_steps, "agent loop hit its step ceiling");
                    finish = FinishReason::MaxSteps;
                    break;
                }
                let last_step = turn.last_step(config.max_steps, &budget);
                if last_step.is_some() {
                    messages.push(turn::system_note(
                        "no more tool calls this turn. Answer now from what you have, and say briefly what you could not check.",
                    ));
                }

                tracing::info!(step, "Agent loop step");
                let (stream_handle, mut events) = turn::spawn_step(
                    &llm_config,
                    &model,
                    &messages,
                    &tools,
                    gateway_options.clone(),
                    StepOptions {
                        reasoning: reasoning.clone(),
                        reasoning_effort: reasoning_effort.clone(),
                        tool_choice: last_step.map(|_| serde_json::json!("none")),
                    },
                    // Lets the provider route the call to the server that
                    // already holds this conversation's cache.
                    context.chat_id.clone(),
                );

                // Checked between events rather than with a `select!`, because
                // `yield` inside a select arm does not survive `async_stream`.
                // Aborting drops the response body, which ends the generation
                // and its billing; what streamed before stands.
                let mut cancelled_mid_stream = false;
                while let Some(event) = events.recv().await {
                    yield event;
                    if cancelled(&cancel_token) {
                        tracing::info!(step, "Stop pressed mid-stream — dropping the provider call");
                        stream_handle.abort();
                        cancelled_mid_stream = true;
                        break;
                    }
                }
                if cancelled_mid_stream {
                    // No error event: the person asked for this ending.
                    finish = FinishReason::Cancelled;
                    break;
                }

                let result = match stream_handle.await {
                    Ok(inner) => inner,
                    Err(e) => Err(StreamError::Connection(format!("Stream task panicked: {}", e))),
                };
                let result = match result {
                    Ok(r) => r,
                    Err(e) => {
                        // The one ending that IS an error event: the reply
                        // stopped for a reason outside the turn, and the
                        // person needs the card and the retry.
                        tracing::error!(step, error = %e, "the model stream failed");
                        finish = FinishReason::Error;
                        yield AgentEvent::error(e.to_string(), Some(turn::stream_error_code(&e)), false);
                        break;
                    }
                };
                turn.record_usage(&result, &model, context.chat_id.as_deref());

                // Before the completion check, so a final step's thinking is
                // stored too.
                if !result.reasoning_details.is_empty() {
                    yield AgentEvent::ReasoningDetails { details: result.reasoning_details.clone() };
                }

                if result.tool_calls.is_empty() {
                    finish = turn::answered(result.finish_reason, last_step);
                    yield AgentEvent::step_complete(step, result.finish_reason);
                    break;
                }
                yield AgentEvent::step_complete(step, StepReason::ToolCalls);

                // Told to answer and called a tool anyway (a provider that
                // ignores `tool_choice: none`); no step is coming to read it.
                if let Some(reason) = last_step {
                    tracing::warn!(step, "tool call on the tool-less last step; ending the turn");
                    finish = reason;
                    break;
                }

                let tool_results = turn
                    .run_tools(&tool_executor, result.tool_calls.clone(), &context, &executor_config)
                    .await;
                let awaiting_user = tool_results.iter().any(turn::needs_user);
                for tool_result in &tool_results {
                    yield tool_result.to_event();
                }
                if awaiting_user {
                    tracing::info!(step, "Tool requires user action, pausing loop");
                    finish = FinishReason::AwaitingUser;
                    break;
                }
                if cancelled(&cancel_token) {
                    tracing::info!(step, "Agent loop cancelled after tool execution");
                    finish = FinishReason::Cancelled;
                    break;
                }

                turn::append_next_step(
                    &mut messages,
                    &model,
                    &result,
                    &tool_results,
                    crate::api::model_catalog::supports_vision(&model),
                    config.max_steps.saturating_sub(step),
                );
            }

            yield AgentEvent::done(turn.step, finish);
        })
    }
}

/// Whether the person has pressed Stop.
fn cancelled(token: &Option<CancellationToken>) -> bool {
    token.as_ref().is_some_and(|t| t.is_cancelled())
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
        assert_eq!(config.thinking, Thinking::Default);
        assert!(TurnBudget::default().tool_caps.is_empty());
    }
}

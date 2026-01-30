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
//! let agent = AgentLoop::new(pool, tollbooth_config);
//! let stream = agent.run(messages, tools, context);
//!
//! while let Some(event) = stream.next().await {
//!     // Handle AgentEvent
//! }
//! ```

pub mod executor;
pub mod prompt;
pub mod protocol;
pub mod stream;

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use async_stream::stream;
use futures::Stream;
use serde_json::Value;
use sqlx::SqlitePool;

use crate::tools::{ToolContext, ToolExecutor};

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
            max_steps: 10,
            tool_timeout: Duration::from_secs(30),
            parallel_tools: true,
        }
    }
}

/// The main agent loop orchestrator
///
/// Handles the complete cycle of:
/// 1. Calling the LLM
/// 2. Streaming responses to the client
/// 3. Executing tool calls
/// 4. Continuing until completion
pub struct AgentLoop {
    pool: Arc<SqlitePool>,
    llm_config: LlmConfig,
    tool_executor: ToolExecutor,
    config: AgentConfig,
}

impl AgentLoop {
    /// Create a new AgentLoop
    pub fn new(
        pool: SqlitePool,
        tollbooth_url: String,
        tollbooth_user_id: String,
        tollbooth_secret: String,
    ) -> Self {
        let pool = Arc::new(pool);
        Self {
            tool_executor: ToolExecutor::new(
                (*pool).clone(),
                tollbooth_url.clone(),
                tollbooth_secret.clone(),
            ),
            llm_config: LlmConfig {
                tollbooth_url,
                tollbooth_user_id,
                tollbooth_secret,
            },
            pool,
            config: AgentConfig::default(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(mut self, config: AgentConfig) -> Self {
        self.config = config;
        self
    }

    /// Run the agent loop
    ///
    /// Returns a stream of AgentEvents that can be forwarded to the client.
    pub fn run(
        &self,
        model: String,
        initial_messages: Vec<Value>,
        tools: Vec<Value>,
        context: ToolContext,
        initial_thought_signature: Option<String>,
    ) -> Pin<Box<dyn Stream<Item = AgentEvent> + Send + '_>> {
        let llm_config = self.llm_config.clone();
        let tool_executor = self.tool_executor.clone();
        let config = self.config.clone();
        let executor_config = ExecutorConfig {
            tool_timeout: config.tool_timeout,
            parallel: config.parallel_tools,
        };

        Box::pin(stream! {
            let mut messages = initial_messages;
            let mut step: u32 = 0;
            let mut next_thought_signature = initial_thought_signature;

            // Emit loop started
            yield AgentEvent::LoopStarted {
                max_steps: config.max_steps,
            };

            loop {
                step += 1;

                // Check max steps
                if step > config.max_steps {
                    yield AgentEvent::error(
                        format!("Maximum steps ({}) exceeded", config.max_steps),
                        Some(ErrorCode::MaxStepsExceeded),
                        false,
                    );
                    break;
                }

                tracing::info!(step, "Agent loop step");

                // Build provider options for reasoning models
                let provider_options = stream::build_provider_options(&model);

                // Use the next_thought_signature if available, otherwise look in history
                let thought_signature = if next_thought_signature.is_some() {
                    next_thought_signature.take()
                } else {
                    messages.iter().rev()
                        .filter_map(|m| m.get("thought_signature").and_then(|s| s.as_str()))
                        .next()
                        .map(|s| s.to_string())
                };

                // Collect events to emit after streaming
                let mut pending_events: Vec<AgentEvent> = Vec::new();

                // Stream LLM response
                let result = stream::stream_llm_response(
                    &llm_config,
                    &model,
                    &messages,
                    &tools,
                    provider_options,
                    thought_signature,
                    |event| {
                        if let AgentEvent::ThoughtSignature { signature } = &event {
                            next_thought_signature = Some(signature.clone());
                        }
                        pending_events.push(event);
                    },
                )
                .await;

                // Emit all collected events
                for event in pending_events {
                    yield event;
                }

                let result = match result {
                    Ok(r) => r,
                    Err(e) => {
                        yield AgentEvent::error(
                            e.to_string(),
                            Some(ErrorCode::LlmError),
                            false,
                        );
                        break;
                    }
                };

                // Check if we're done (no tool calls)
                if result.tool_calls.is_empty() {
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

                let tool_results = executor::execute_tools(
                    &tool_executor,
                    &result.tool_calls,
                    &context,
                    &executor_config,
                )
                .await;

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
                    yield AgentEvent::done_with_reason(step, protocol::FinishReason::AwaitingUser);
                    break;
                }

                // Build messages for next iteration
                // 1. Add assistant message with tool calls
                messages.push(executor::build_assistant_tool_message(
                    &result.content,
                    &result.tool_calls,
                    result.thought_signature.as_deref(),
                ));

                // 2. Add tool result messages
                for tool_result in &tool_results {
                    messages.push(executor::build_tool_result_message(
                        &tool_result.tool_call_id,
                        &tool_result.to_llm_content(),
                    ));
                }

                // Continue loop for next LLM call
            }

            // Emit done
            yield AgentEvent::done(step);
        })
    }
}

impl std::fmt::Debug for AgentLoop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentLoop")
            .field("config", &self.config)
            .field("llm_config.tollbooth_url", &self.llm_config.tollbooth_url)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AgentConfig::default();
        assert_eq!(config.max_steps, 10);
        assert_eq!(config.tool_timeout, Duration::from_secs(30));
        assert!(config.parallel_tools);
    }
}

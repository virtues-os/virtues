//! Tool Execution
//!
//! Wrapper around ToolExecutor for the agent loop, providing
//! parallel execution, timeouts, and error handling.

use std::time::Duration;

use futures::future::join_all;
use serde_json::Value;
use tokio::time::timeout;

use crate::tools::{ToolContext, ToolError, ToolExecutor, ToolResult};

use super::protocol::AgentEvent;
use super::stream::ToolCall;

/// Configuration for tool execution
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    /// Timeout for individual tool execution
    pub tool_timeout: Duration,
    /// Whether to execute tools in parallel
    pub parallel: bool,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            tool_timeout: Duration::from_secs(30),
            parallel: true,
        }
    }
}

/// Result of executing a tool
#[derive(Debug)]
pub struct ToolExecutionResult {
    pub tool_call_id: String,
    pub tool_name: String,
    pub result: Result<ToolResult, ToolExecutionError>,
}

impl ToolExecutionResult {
    /// Convert to AgentEvent
    pub fn to_event(&self) -> AgentEvent {
        match &self.result {
            Ok(result) => AgentEvent::tool_result(
                &self.tool_call_id,
                result.data.clone(),
                result.success,
            ),
            Err(e) => AgentEvent::tool_error(&self.tool_call_id, e.to_string()),
        }
    }

    /// Check if execution was successful
    pub fn is_success(&self) -> bool {
        matches!(&self.result, Ok(r) if r.success)
    }

    /// Get the result value for LLM context
    pub fn to_llm_content(&self) -> String {
        match &self.result {
            Ok(result) => serde_json::to_string(&result.data).unwrap_or_else(|_| {
                format!("Tool completed: {}", if result.success { "success" } else { "with errors" })
            }),
            Err(e) => format!("Tool execution failed: {}", e),
        }
    }
}

/// Errors that can occur during tool execution
#[derive(Debug, thiserror::Error)]
pub enum ToolExecutionError {
    #[error("Tool execution timed out after {0:?}")]
    Timeout(Duration),

    #[error("Tool not found: {0}")]
    NotFound(String),

    #[error("Invalid arguments: {0}")]
    InvalidArguments(String),

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
}

impl From<ToolError> for ToolExecutionError {
    fn from(err: ToolError) -> Self {
        match err {
            ToolError::UnknownTool(name) => Self::NotFound(name),
            ToolError::InvalidParameters(msg) => Self::InvalidArguments(msg),
            ToolError::ExecutionFailed(msg) => Self::ExecutionFailed(msg),
            ToolError::NotEnabled(msg) => Self::ExecutionFailed(format!("Tool not enabled: {}", msg)),
            ToolError::MissingContext(msg) => Self::ExecutionFailed(format!("Missing context: {}", msg)),
        }
    }
}

/// Execute a list of tool calls
///
/// Handles parallel execution, timeouts, and error conversion.
pub async fn execute_tools(
    executor: &ToolExecutor,
    tool_calls: &[ToolCall],
    context: &ToolContext,
    config: &ExecutorConfig,
) -> Vec<ToolExecutionResult> {
    if config.parallel {
        execute_parallel(executor, tool_calls, context, config).await
    } else {
        execute_sequential(executor, tool_calls, context, config).await
    }
}

/// Execute tools in parallel
async fn execute_parallel(
    executor: &ToolExecutor,
    tool_calls: &[ToolCall],
    context: &ToolContext,
    config: &ExecutorConfig,
) -> Vec<ToolExecutionResult> {
    let futures = tool_calls.iter().map(|tc| {
        let executor = executor.clone();
        let context = context.clone();
        let config = config.clone();
        let tc = tc.clone();
        
        async move {
            execute_single(&executor, &tc, &context, &config).await
        }
    });

    join_all(futures).await
}

/// Execute tools sequentially
async fn execute_sequential(
    executor: &ToolExecutor,
    tool_calls: &[ToolCall],
    context: &ToolContext,
    config: &ExecutorConfig,
) -> Vec<ToolExecutionResult> {
    let mut results = Vec::with_capacity(tool_calls.len());
    
    for tc in tool_calls {
        results.push(execute_single(executor, tc, context, config).await);
    }
    
    results
}

/// Execute a single tool call with timeout
async fn execute_single(
    executor: &ToolExecutor,
    tool_call: &ToolCall,
    context: &ToolContext,
    config: &ExecutorConfig,
) -> ToolExecutionResult {
    tracing::info!(
        tool_call_id = %tool_call.id,
        tool_name = %tool_call.name,
        "Executing tool"
    );

    let result = timeout(
        config.tool_timeout,
        executor.execute(&tool_call.name, tool_call.arguments.clone(), context),
    )
    .await;

    let result = match result {
        Ok(Ok(result)) => {
            tracing::info!(
                tool_call_id = %tool_call.id,
                success = result.success,
                "Tool execution completed"
            );
            Ok(result)
        }
        Ok(Err(e)) => {
            tracing::warn!(
                tool_call_id = %tool_call.id,
                error = %e,
                "Tool execution failed"
            );
            Err(ToolExecutionError::from(e))
        }
        Err(_) => {
            tracing::warn!(
                tool_call_id = %tool_call.id,
                timeout = ?config.tool_timeout,
                "Tool execution timed out"
            );
            Err(ToolExecutionError::Timeout(config.tool_timeout))
        }
    };

    ToolExecutionResult {
        tool_call_id: tool_call.id.clone(),
        tool_name: tool_call.name.clone(),
        result,
    }
}

/// Build the tool result message for the LLM
pub fn build_tool_result_message(tool_call_id: &str, content: &str) -> Value {
    serde_json::json!({
        "role": "tool",
        "tool_call_id": tool_call_id,
        "content": content
    })
}

/// Build the assistant message with tool calls
pub fn build_assistant_tool_message(
    content: &str,
    tool_calls: &[ToolCall],
    thought_signature: Option<&str>,
) -> Value {
    let mut msg = serde_json::json!({
        "role": "assistant",
        "content": if content.is_empty() { Value::Null } else { Value::String(content.to_string()) },
        "tool_calls": tool_calls.iter().map(|tc| {
            serde_json::json!({
                "id": tc.id,
                "type": "function",
                "function": {
                    "name": tc.name,
                    "arguments": serde_json::to_string(&tc.arguments).unwrap_or_default()
                }
            })
        }).collect::<Vec<_>>()
    });

    if let Some(sig) = thought_signature {
        msg["thought_signature"] = serde_json::json!(sig);
    }

    msg
}

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

    // `dispatch_subagents` fans out several nested agent loops in parallel and routinely runs for
    // minutes — far past the default per-tool timeout. Give it a long dedicated ceiling so it isn't
    // killed mid-research (the workers have their own step + per-call limits as the real bounds).
    let tool_timeout = if tool_call.name == "dispatch_subagents" {
        Duration::from_secs(600)
    } else {
        config.tool_timeout
    };

    let result = timeout(
        tool_timeout,
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

/// Carry tool attachments to the model as a following user message.
///
/// It has to be a *user* message: a `role: "tool"` message takes a string in
/// the OpenAI-compatible shape every provider here speaks, so an image cannot
/// ride inside the tool result itself. Following it with a user turn is the
/// conventional way around that, and it produces exactly the content blocks a
/// pasted screenshot produces — see the `UIPart::File` arm in `compaction`.
///
/// The leading text is not decoration. Without it the model receives an image
/// with no provenance in a turn it did not expect one, and the failure mode is
/// that it narrates the image as though the user had just sent it.
///
/// Returns None when nothing survives filtering, so the caller adds no message
/// at all rather than an empty user turn.
pub fn build_attachment_message(attachments: &[(String, crate::tools::ToolAttachment)]) -> Option<Value> {
    let mut parts: Vec<Value> = Vec::new();
    let mut named: Vec<&str> = Vec::new();

    for (_, att) in attachments {
        if !att.media_type.starts_with("image/") {
            continue;
        }
        named.push(&att.filename);
        parts.push(serde_json::json!({
            "type": "image_url",
            "image_url": { "url": att.data_url }
        }));
    }

    if parts.is_empty() {
        return None;
    }

    let preface = if named.len() == 1 {
        format!(
            "[Contents of {}, returned by the tool call above. This is the file itself, not something the user just sent.]",
            named[0]
        )
    } else {
        format!(
            "[Contents of {} files returned by the tool calls above ({}). These are the files themselves, not something the user just sent.]",
            named.len(),
            named.join(", ")
        )
    };

    parts.insert(0, serde_json::json!({ "type": "text", "text": preface }));

    Some(serde_json::json!({
        "role": "user",
        "content": parts
    }))
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

#[cfg(test)]
mod attachment_tests {
    use super::*;
    use crate::tools::ToolAttachment;

    fn att(media_type: &str, filename: &str) -> (String, ToolAttachment) {
        (
            "read_asset".to_string(),
            ToolAttachment {
                media_type: media_type.to_string(),
                data_url: format!("data:{media_type};base64,AAAA"),
                filename: filename.to_string(),
            },
        )
    }

    #[test]
    fn images_become_content_blocks_behind_a_provenance_line() {
        let msg = build_attachment_message(&[att("image/png", "shot.png")]).expect("a message");
        assert_eq!(msg["role"], "user");
        let parts = msg["content"].as_array().expect("content parts");
        assert_eq!(parts.len(), 2, "one preface + one image");
        assert_eq!(parts[0]["type"], "text");
        let preface = parts[0]["text"].as_str().unwrap();
        assert!(preface.contains("shot.png"), "names the file: {preface}");
        // The model must not read this as the user having just sent a picture.
        assert!(
            preface.contains("not something the user just sent"),
            "disclaims user provenance: {preface}"
        );
        assert_eq!(parts[1]["type"], "image_url");
        assert_eq!(parts[1]["image_url"]["url"], "data:image/png;base64,AAAA");
    }

    #[test]
    fn non_images_are_dropped_rather_than_sent_as_broken_images() {
        // PDF and audio ride the same UIPart path inbound from the browser, but
        // outbound they need their own block shapes; until then, silence beats
        // an image_url the provider will reject.
        assert!(build_attachment_message(&[att("application/pdf", "a.pdf")]).is_none());
        assert!(build_attachment_message(&[att("audio/mp4", "a.m4a")]).is_none());
    }

    #[test]
    fn no_attachments_means_no_message_at_all() {
        assert!(build_attachment_message(&[]).is_none());
    }

    #[test]
    fn a_mixed_batch_keeps_only_the_images_and_counts_them_honestly() {
        let msg = build_attachment_message(&[
            att("image/png", "one.png"),
            att("application/pdf", "skipped.pdf"),
            att("image/jpeg", "two.jpg"),
        ])
        .expect("a message");
        let parts = msg["content"].as_array().unwrap();
        assert_eq!(parts.len(), 3, "preface + two images, pdf dropped");
        let preface = parts[0]["text"].as_str().unwrap();
        assert!(preface.contains("2 files"), "counts what was attached, not what was offered: {preface}");
        assert!(!preface.contains("skipped.pdf"), "does not name what it dropped: {preface}");
    }
}

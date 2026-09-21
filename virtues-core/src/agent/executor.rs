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
            // A tool that answered with a failure carries its reason in
            // `error`, not in `data`. Passing `success: false` with the reason
            // dropped left the UI to say only "the tool reported a failure".
            Ok(result) if !result.success => AgentEvent::tool_error(
                &self.tool_call_id,
                result.error.clone().unwrap_or_else(|| "the tool reported a failure".to_string()),
            ),
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

    /// Get the result value for LLM context.
    ///
    /// A tool that answered with `ToolResult::error` has `data: Null` and its
    /// reason in `error`. Serializing only `data` handed the model the literal
    /// string `null` — no reason, not even the fact of a failure — so it
    /// confabulated or retried the same call. The reason is the content.
    ///
    /// One envelope for every failure: `Tool failed (<tool>, <cause>): <reason>`.
    /// The wire this box speaks has no error flag on a tool result, so the
    /// envelope IS the flag, and it used to be three of them stacked —
    /// `Tool execution failed: Execution failed: Query failed: …` — one per
    /// layer the error crossed, saying nothing the next did not. The cause is
    /// the one word that changes what the model should do: `timeout` means
    /// ask for less, `invalid arguments` means fix the call, `execution` means
    /// read the reason.
    ///
    /// A failure's `data` rides along, capped. A tool that fails WITH output
    /// — `code_interpreter` puts the traceback in stderr and says only "Code
    /// execution failed" in `error` — was handing the model the sentence and
    /// keeping the evidence, so it could not fix the code and the
    /// repeated-failure guard then refused the identical retry. The data is
    /// the part it needs.
    pub fn to_llm_content(&self) -> String {
        match &self.result {
            Ok(result) if !result.success => {
                let reason = result.error.as_deref().unwrap_or("the tool reported a failure");
                let mut out = format!("Tool failed ({}, execution): {reason}", self.tool_name);
                if !result.data.is_null() {
                    if let Ok(data) = serde_json::to_string(&result.data) {
                        out.push('\n');
                        out.push_str(&clip_for_model(&data, FAILURE_DATA_BYTES));
                    }
                }
                out
            }
            Ok(result) => {
                let text = serde_json::to_string(&result.data)
                    .unwrap_or_else(|_| "Tool completed: success".to_string());
                clip_for_model(&text, MAX_TOOL_OUTPUT_BYTES)
            }
            Err(e) => format!("Tool failed ({}, {}): {}", self.tool_name, e.cause(), e),
        }
    }
}

/// How much of one tool's output the turn that called it gets to read.
///
/// The turn after it replays at most 32 KiB (`compaction::MAX_REPLAYED_TOOL_BYTES`),
/// but the calling turn saw everything, and measured on a real box the
/// ninetieth-percentile result was 94 KB — a `semantic_search` with fifty
/// previews, a page, a web result set — with nothing capping it but
/// `sql_query`'s own 256 KiB. Ninety-six KiB is about 24k tokens, where the
/// coding harnesses converge (Claude Code truncates tool output at 25k
/// tokens). Past it the model is told how much it did not see and what to do
/// about it, which is the difference between a cap and a silent loss.
pub const MAX_TOOL_OUTPUT_BYTES: usize = 96 * 1024;

/// How much of a FAILED tool's data rides with the failure. A traceback fits
/// in a fraction of this; a failure that returns more than this is returning
/// its whole output under the wrong flag.
const FAILURE_DATA_BYTES: usize = 8 * 1024;

/// Cut at a character boundary and say what was cut. The instruction at the
/// end is the same one the replay clip carries, because the fix is the same:
/// ask for less.
fn clip_for_model(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut cut = max;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    format!(
        "{}\n… [{} more bytes not shown. Ask for less: a narrower window, fewer results, or one record by id.]",
        &text[..cut],
        text.len() - cut
    )
}

/// Errors that can occur during tool execution.
///
/// The Display strings carry no "failed" prefix of their own: the envelope
/// in `to_llm_content` names the tool and the cause once, and the UI's error
/// text (`to_event`) is this string alone, so a prefix here was printed
/// twice and stripped once on each side.
#[derive(Debug, thiserror::Error)]
pub enum ToolExecutionError {
    #[error("no result after {0:?}. Ask for less — a narrower window, fewer rows — or split the request")]
    Timeout(Duration),

    #[error("no tool named {0}")]
    NotFound(String),

    #[error("{0}")]
    InvalidArguments(String),

    #[error("{0}")]
    ExecutionFailed(String),
}

impl ToolExecutionError {
    /// The one word in the failure envelope that changes what the model
    /// should do next.
    pub fn cause(&self) -> &'static str {
        match self {
            Self::Timeout(_) => "timeout",
            Self::NotFound(_) => "not found",
            Self::InvalidArguments(_) => "invalid arguments",
            Self::ExecutionFailed(_) => "execution",
        }
    }
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

    // Arguments the model sent that were not JSON. The stream used to replace
    // them with `{}` silently, so the tool failed on a missing required field
    // and the model was told "'sql' is required" about a call it had written
    // in full. Say what actually happened, so the next attempt fixes the
    // JSON rather than the arguments.
    if let Some(bad) = tool_call.arguments.get(super::stream::UNPARSEABLE_ARGUMENTS_KEY) {
        let why = bad.get("error").and_then(|v| v.as_str()).unwrap_or("not valid JSON");
        let raw = bad.get("raw").and_then(|v| v.as_str()).unwrap_or("");
        tracing::warn!(tool_call_id = %tool_call.id, tool_name = %tool_call.name, error = %why, "tool call arguments were not JSON");
        return ToolExecutionResult {
            tool_call_id: tool_call.id.clone(),
            tool_name: tool_call.name.clone(),
            result: Err(ToolExecutionError::InvalidArguments(format!(
                "the arguments were not valid JSON ({why}). Send the call again with well-formed JSON. What arrived began: {raw}"
            ))),
        };
    }

    // `dispatch_subagents` fans out several nested agent loops in parallel and routinely runs for
    // minutes — far past the default per-tool timeout. Give it a long dedicated ceiling so it isn't
    // killed mid-research (the workers have their own step + per-call limits as the real bounds).
    // `write_it_up` chains two model calls (document, chapters) over a full interview
    // transcript — ~40s+ observed for the document alone, so the 30s default would kill every run.
    // `generate_image` is one image-model call: ~18s per attempt measured on the box 2026-09-14,
    // and the gateway client may resend an empty completion up to three times, so 30s cut it off
    // after the first billed attempt — every image was paid for and none reached the chat.
    let tool_timeout = if tool_call.name == "dispatch_subagents" {
        Duration::from_secs(600)
    } else if tool_call.name == "write_it_up" {
        Duration::from_secs(240)
    } else if tool_call.name == "generate_image" {
        Duration::from_secs(120)
    } else if tool_call.name == "code_interpreter" {
        // Its schema offers the model a timeout of up to 120s and defaults to
        // 60. Under the 30s default the DEFAULT was already unreachable: any
        // code that ran longer died no matter what the model asked for, and
        // the sandbox subprocess was left running. The ceiling matches what
        // the tool advertises, plus room for the sandbox to start.
        Duration::from_secs(150)
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
            // Report the ceiling this tool actually ran under, not the default.
            tracing::warn!(
                tool_call_id = %tool_call.id,
                timeout = ?tool_timeout,
                "Tool execution timed out"
            );
            Err(ToolExecutionError::Timeout(tool_timeout))
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
    reasoning_details: &[Value],
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

    // The gateway asks for its own reasoning blocks back, verbatim, on the
    // message that produced them, so the model can resume the thought that
    // led to these tool calls. Only when there were any: an empty array is
    // a claim of "no reasoning" some providers reject.
    if !reasoning_details.is_empty() {
        msg["reasoning_details"] = Value::Array(reasoning_details.to_vec());
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

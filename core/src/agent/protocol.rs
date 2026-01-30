//! Agent Protocol
//!
//! Defines the SSE event types that the frontend expects from the agent loop.
//! These events provide a clean contract between the Rust backend and Svelte frontend.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Events emitted by the AgentLoop during execution.
///
/// These are serialized to SSE events for the frontend to consume.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    // ─────────────────────────────────────────────────────────────────────────
    // Text Streaming
    // ─────────────────────────────────────────────────────────────────────────
    /// A chunk of text content from the LLM
    TextDelta {
        content: String,
    },

    /// Reasoning/thinking content (for models that support it)
    ReasoningDelta {
        content: String,
    },

    // ─────────────────────────────────────────────────────────────────────────
    // Tool Lifecycle
    // ─────────────────────────────────────────────────────────────────────────
    /// A tool call has started
    ToolCallStart {
        id: String,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        args: Option<Value>,
    },

    /// Tool arguments are being streamed (partial)
    ToolCallArgsPartial {
        id: String,
        args_delta: String,
    },

    /// Tool call arguments are complete
    ToolCallArgsComplete {
        id: String,
        args: Value,
    },

    /// Tool execution has completed
    ToolCallResult {
        id: String,
        result: Value,
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },

    // ─────────────────────────────────────────────────────────────────────────
    // Step/Loop Management
    // ─────────────────────────────────────────────────────────────────────────
    /// A step in the agent loop has completed
    StepComplete {
        step: u32,
        reason: StepReason,
    },

    /// The agent loop has started
    LoopStarted {
        max_steps: u32,
    },

    // ─────────────────────────────────────────────────────────────────────────
    // Metadata
    // ─────────────────────────────────────────────────────────────────────────
    /// Token usage for a step
    Usage {
        prompt_tokens: u32,
        completion_tokens: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        total_tokens: Option<u32>,
    },

    /// Message ID assignment (for persistence)
    MessageId {
        id: String,
    },

    // ─────────────────────────────────────────────────────────────────────────
    // Terminal Events
    // ─────────────────────────────────────────────────────────────────────────
    /// The agent loop has completed
    Done {
        total_steps: u32,
        finish_reason: FinishReason,
    },

    /// An error occurred
    Error {
        message: String,
        code: Option<ErrorCode>,
        recoverable: bool,
    },

    /// Gemini thought signature (opaque, for subsequent tool calls)
    ThoughtSignature {
        signature: String,
    },
}

/// Reason why a step completed
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StepReason {
    /// LLM finished generating (stop token)
    EndTurn,
    /// LLM wants to execute tools
    ToolCalls,
    /// Hit max tokens limit
    MaxTokens,
    /// Content was filtered
    ContentFilter,
}

/// Reason why the agent loop finished
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    /// Normal completion (LLM said stop)
    EndTurn,
    /// Hit maximum steps limit
    MaxSteps,
    /// A tool requires user action (e.g., binding a page)
    AwaitingUser,
    /// An error occurred
    Error,
}

/// Error codes for categorizing failures
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Too many iterations in the loop
    MaxStepsExceeded,
    /// LLM API error
    LlmError,
    /// Tool execution failed
    ToolError,
    /// Request was cancelled
    Cancelled,
    /// Rate limited
    RateLimited,
    /// Invalid request
    InvalidRequest,
    /// Internal server error
    Internal,
}

impl AgentEvent {
    /// Convert to SSE event format
    pub fn to_sse_data(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            r#"{"type":"error","message":"Failed to serialize event","recoverable":false}"#.to_string()
        })
    }

    /// Create a text delta event
    pub fn text(content: impl Into<String>) -> Self {
        Self::TextDelta {
            content: content.into(),
        }
    }

    /// Create a reasoning delta event
    pub fn reasoning(content: impl Into<String>) -> Self {
        Self::ReasoningDelta {
            content: content.into(),
        }
    }

    /// Create a tool call start event
    pub fn tool_start(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self::ToolCallStart {
            id: id.into(),
            name: name.into(),
            args: None,
        }
    }

    /// Create a tool call result event
    pub fn tool_result(id: impl Into<String>, result: Value, success: bool) -> Self {
        Self::ToolCallResult {
            id: id.into(),
            result,
            success,
            error: None,
        }
    }

    /// Create a tool call error event
    pub fn tool_error(id: impl Into<String>, error: impl Into<String>) -> Self {
        Self::ToolCallResult {
            id: id.into(),
            result: Value::Null,
            success: false,
            error: Some(error.into()),
        }
    }

    /// Create a step complete event
    pub fn step_complete(step: u32, reason: StepReason) -> Self {
        Self::StepComplete { step, reason }
    }

    /// Create a done event
    pub fn done(total_steps: u32) -> Self {
        Self::Done {
            total_steps,
            finish_reason: FinishReason::EndTurn,
        }
    }

    /// Create a done event with a specific finish reason
    pub fn done_with_reason(total_steps: u32, finish_reason: FinishReason) -> Self {
        Self::Done {
            total_steps,
            finish_reason,
        }
    }

    /// Create an error event
    pub fn error(message: impl Into<String>, code: Option<ErrorCode>, recoverable: bool) -> Self {
        Self::Error {
            message: message.into(),
            code,
            recoverable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_serialization() {
        let event = AgentEvent::text("Hello");
        let json = event.to_sse_data();
        assert!(json.contains("text_delta"));
        assert!(json.contains("Hello"));
    }

    #[test]
    fn test_tool_call_serialization() {
        let event = AgentEvent::tool_start("call_123", "edit_page");
        let json = event.to_sse_data();
        assert!(json.contains("tool_call_start"));
        assert!(json.contains("call_123"));
        assert!(json.contains("edit_page"));
    }
}

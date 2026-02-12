//! Token Estimation Module
//!
//! Provides token counting utilities for context management.
//! Uses heuristic-based estimation since exact tokenization varies by model.

use crate::api::chats::ChatMessage;

/// Estimate tokens for a text string.
///
/// Uses a heuristic of ~4 characters per token, which is a reasonable
/// approximation for English text across most LLM tokenizers.
/// For more accuracy, consider using tiktoken-rs for OpenAI models
/// or model-specific tokenizers.
pub fn estimate_tokens(content: &str) -> i64 {
    // Heuristic: ~4 chars per token for English text
    // This is conservative - actual token counts may be lower
    let char_count = content.len() as i64;
    (char_count / 4).max(1)
}

/// Estimate tokens for a ChatMessage including all its content
pub fn estimate_message_tokens(message: &ChatMessage) -> i64 {
    let mut tokens = 0i64;

    // Main content
    tokens += estimate_tokens(&message.content);

    // Role overhead (typically ~4 tokens for role markers)
    tokens += 4;

    // Reasoning content if present
    if let Some(reasoning) = &message.reasoning {
        tokens += estimate_tokens(reasoning);
    }

    // Tool calls
    if let Some(tool_calls) = &message.tool_calls {
        for tool_call in tool_calls {
            // Tool name
            tokens += estimate_tokens(&tool_call.tool_name);

            // Arguments (serialize to string and estimate)
            if let Ok(args_str) = serde_json::to_string(&tool_call.arguments) {
                tokens += estimate_tokens(&args_str);
            }

            // Result if present
            if let Some(result) = &tool_call.result {
                if let Ok(result_str) = serde_json::to_string(result) {
                    tokens += estimate_tokens(&result_str);
                }
            }
        }
    }

    tokens
}

/// Result of context estimation
#[derive(Debug, Clone)]
pub struct ContextEstimate {
    /// Total estimated tokens in the context
    pub total_tokens: i64,
    /// Context window size for the model
    pub context_window: i64,
    /// Usage as a percentage (0.0 - 100.0)
    pub usage_percentage: f64,
    /// Status based on thresholds
    pub status: ContextStatus,
}

/// Context usage status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextStatus {
    /// Under 70% - no action needed
    Healthy,
    /// 70-84% - show warning
    Warning,
    /// 85%+ - needs compaction
    Critical,
}

impl ContextStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContextStatus::Healthy => "healthy",
            ContextStatus::Warning => "warning",
            ContextStatus::Critical => "critical",
        }
    }
}

/// Estimate total context usage for a session
///
/// # Arguments
/// * `messages` - All messages in the conversation
/// * `summary` - Optional conversation summary (if compacted)
/// * `system_prompt` - The system prompt used
/// * `context_window` - The model's context window size
pub fn estimate_session_context(
    messages: &[ChatMessage],
    summary: Option<&str>,
    system_prompt: Option<&str>,
    context_window: i64,
) -> ContextEstimate {
    let mut total_tokens = 0i64;

    // System prompt
    if let Some(prompt) = system_prompt {
        total_tokens += estimate_tokens(prompt);
        total_tokens += 4; // Role overhead
    }

    // Conversation summary (if compacted)
    if let Some(summary_text) = summary {
        total_tokens += estimate_tokens(summary_text);
        total_tokens += 4; // Role overhead for system message
    }

    // All messages
    for message in messages {
        total_tokens += estimate_message_tokens(message);
    }

    // Calculate percentage and status
    let usage_percentage = if context_window > 0 {
        (total_tokens as f64 / context_window as f64) * 100.0
    } else {
        0.0
    };

    let status = if usage_percentage >= 85.0 {
        ContextStatus::Critical
    } else if usage_percentage >= 70.0 {
        ContextStatus::Warning
    } else {
        ContextStatus::Healthy
    };

    ContextEstimate {
        total_tokens,
        context_window,
        usage_percentage,
        status,
    }
}

/// Estimate context for only the verbatim messages (after summary_up_to_index)
pub fn estimate_verbatim_context(
    messages: &[ChatMessage],
    summary_up_to_index: usize,
    summary: Option<&str>,
    system_prompt: Option<&str>,
    context_window: i64,
) -> ContextEstimate {
    let verbatim_messages = if summary_up_to_index < messages.len() {
        &messages[summary_up_to_index..]
    } else {
        &[]
    };

    estimate_session_context(verbatim_messages, summary, system_prompt, context_window)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens() {
        // 100 characters should be ~25 tokens
        let text = "a".repeat(100);
        assert_eq!(estimate_tokens(&text), 25);

        // Empty string should return 1 (minimum)
        assert_eq!(estimate_tokens(""), 1);

        // Short string
        assert_eq!(estimate_tokens("Hello"), 1);
    }

    #[test]
    fn test_context_status_thresholds() {
        let messages: Vec<ChatMessage> = vec![];

        // Under 70% - healthy
        let estimate = estimate_session_context(&messages, None, None, 1000);
        assert_eq!(estimate.status, ContextStatus::Healthy);

        // Create messages that would be 70%+
        let large_content = "x".repeat(2800); // ~700 tokens
        let messages = vec![ChatMessage {
            id: None,
            role: "user".to_string(),
            content: large_content,
            timestamp: crate::types::Timestamp::parse("2024-01-01T00:00:00Z").unwrap(),
            model: None,
            provider: None,
            agent_id: None,
            tool_calls: None,
            reasoning: None,
            intent: None,
            subject: None,
            thought_signature: None,
            parts: None,
        }];

        let estimate = estimate_session_context(&messages, None, None, 1000);
        assert!(estimate.usage_percentage >= 70.0);
    }
}

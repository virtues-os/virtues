//! Token Estimation Module
//!
//! Provides token counting utilities for context management.
//! Uses heuristic-based estimation since exact tokenization varies by model.

use crate::api::chats::ChatMessage;

/// What to assume a model's context window is when the catalog cannot say.
///
/// Conservative on purpose: too large and a chat never compacts, which is the
/// failure that hurts. The catalog's COLD floor reports a window of `0` —
/// "unknown", explicitly, not "zero" — and `unwrap_or` never fires on a
/// `Some(0)`, so every caller that took it at its word computed 0% usage and
/// reported Healthy forever. Anything that is not a positive window is unknown.
pub const ASSUMED_CONTEXT_WINDOW: i64 = 200_000;

/// The usable window for a model, or the assumption if it has none.
pub fn context_window_or_assumed(window: Option<i64>) -> i64 {
    window.filter(|w| *w > 0).unwrap_or(ASSUMED_CONTEXT_WINDOW)
}

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

/// Estimate tokens for a ChatMessage — as `build_context_for_llm` will send it.
///
/// This has to mirror the renderer, not the row. It used to sum the
/// `tool_calls` COLUMN, which the renderer never sends for a row without
/// `parts`, and to ignore `parts` entirely, which is where attachments live —
/// so on a tool-heavy chat the gauge read many times the real size while a
/// 40-page PDF counted as nothing at all. Compaction fires off this number, so
/// a fiction here is a fiction everywhere.
pub fn estimate_message_tokens(message: &ChatMessage) -> i64 {
    use crate::api::chat::UIPart;

    // Role overhead (typically ~4 tokens for role markers)
    let mut tokens = 4i64;

    if let Some(parts) = &message.parts {
        for part in parts {
            match part {
                UIPart::Text { text } => tokens += estimate_tokens(text),
                // Dropped by the renderer: not a content block type.
                UIPart::Reasoning { .. } => {}
                UIPart::ToolInvocation { tool_name, input, output, error_text, .. } => {
                    tokens += estimate_tokens(tool_name);
                    tokens += estimate_tokens(&input.to_string());
                    match (error_text, output) {
                        (Some(err), _) => tokens += estimate_tokens(err),
                        (None, Some(res)) => tokens += estimate_tokens(&res.to_string()),
                        (None, None) => {}
                    }
                }
                // The data URL is what rides to the model. Images are priced
                // per-tile by most providers rather than per-character, so this
                // is an over-estimate for them — an over-estimate of something
                // real, where the old number was a zero.
                UIPart::File { url, filename, .. } => {
                    tokens += estimate_tokens(url);
                    if let Some(name) = filename {
                        tokens += estimate_tokens(name);
                    }
                }
                // Folded into the system prompt, counted there.
                UIPart::Checkpoint { .. } => {}
                UIPart::Unknown => {}
            }
        }
        return tokens;
    }

    // No parts: the renderer falls back to the joined text and sends nothing
    // else — not the reasoning, and not the `tool_calls` column.
    tokens += estimate_tokens(&message.content);
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
/// What the last assembled system prompt measured, in tokens.
///
/// The gauge cannot rebuild the prompt (see `estimate_session_context`), so the
/// turn that builds it leaves the number here. Atomic and process-local: no
/// column, no migration, and a restart costs one turn of accuracy.
static LAST_SYSTEM_PROMPT_TOKENS: std::sync::atomic::AtomicI64 =
    std::sync::atomic::AtomicI64::new(0);

/// Record the size of the system prompt just sent.
pub fn record_system_prompt_tokens(tokens: i64) {
    LAST_SYSTEM_PROMPT_TOKENS.store(tokens, std::sync::atomic::Ordering::Relaxed);
}

/// The last recorded system-prompt size; 0 before any turn has run.
pub fn last_system_prompt_tokens() -> i64 {
    LAST_SYSTEM_PROMPT_TOKENS.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn estimate_session_context(
    messages: &[ChatMessage],
    summary: Option<&str>,
    system_prompt: Option<&str>,
    context_window: i64,
) -> ContextEstimate {
    let mut total_tokens = 0i64;

    // System prompt.
    //
    // Every caller of this function passes None — none of them HAS the prompt,
    // which is assembled per turn from seven DB-backed blocks. So the single
    // largest component of the request was invisible to the gauge the user
    // reads and to the threshold that triggers compaction: the prefix measures
    // ~13.5k tokens on this box, which is a chat showing "82%" already being
    // over the 85% Critical line, and much worse on a small-window BYO model.
    //
    // Rebuilding it here would mean those seven queries on every check. The
    // turn that just ran already built it, so it reports its size and this
    // reads that. Process-local and lost on restart, where it falls back to 0
    // and is correct again after one turn — a gauge that is briefly optimistic
    // beats seven queries per keystroke or a fabricated constant.
    let prompt_tokens = match system_prompt {
        Some(prompt) => estimate_tokens(prompt),
        None => last_system_prompt_tokens(),
    };
    if prompt_tokens > 0 {
        total_tokens += prompt_tokens;
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
    let verbatim_messages = &messages[summary_up_to_index.min(messages.len())..];

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

    /// The gauge counts the system prompt even though no caller can hand it
    /// over. It could not, so a chat reading "82%" was already past the 85%
    /// line that triggers compaction, and compaction did not fire.
    #[test]
    fn the_remembered_system_prompt_counts_toward_the_window() {
        let messages: Vec<ChatMessage> = vec![];

        record_system_prompt_tokens(0);
        let blind = estimate_session_context(&messages, None, None, 1000);

        record_system_prompt_tokens(500);
        let seeing = estimate_session_context(&messages, None, None, 1000);

        assert!(
            seeing.total_tokens > blind.total_tokens,
            "a recorded prompt has to move the estimate"
        );
        assert_eq!(
            seeing.total_tokens - blind.total_tokens,
            504,
            "500 tokens plus the 4-token role overhead"
        );

        // An explicit prompt still wins over the remembered one.
        let explicit = estimate_session_context(&messages, None, Some("hi"), 1000);
        assert!(explicit.total_tokens < seeing.total_tokens);

        record_system_prompt_tokens(0);
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
            reasoning_details: None,
            parts: None,
        }];

        let estimate = estimate_session_context(&messages, None, None, 1000);
        assert!(estimate.usage_percentage >= 70.0);
    }
}

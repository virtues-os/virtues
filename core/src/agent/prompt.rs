//! Static system prompts for the agent.
//!
//! Minimal system prompt with guidelines only.
//! Tool descriptions come from their schemas in virtues-registry.

/// Base system prompt for the AI assistant.
///
/// Contains only guidelines. Tool descriptions are in their schemas.
/// Dynamic context (like active page) is appended by build_system_prompt().
pub const BASE_SYSTEM_PROMPT: &str = r#"You are a helpful AI assistant.

<guidelines>
- Be concise and helpful
- Use tools when appropriate - explain what you're doing
- For page edits, read content first, then make targeted changes
</guidelines>
"#;

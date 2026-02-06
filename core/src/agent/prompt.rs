//! System prompts for the agent.
//!
//! Provides personalized system prompts based on user and assistant profiles.
//! Tool descriptions come from their schemas in virtues-registry.

/// Base system prompt template (without tool instructions).
///
/// Placeholders:
/// - {assistant_name}: The assistant's name (e.g., "Ari")
/// - {user_name}: The user's preferred name (e.g., "Adam")
/// - {persona_guidelines}: Persona-specific behavior guidelines
///
/// Dynamic context (datetime, active page) is appended by build_system_prompt() in chat.rs.
pub const BASE_SYSTEM_PROMPT: &str = r#"You are {assistant_name}, {user_name}'s personal AI assistant.

<guidelines>
{persona_guidelines}
</guidelines>

<safety>
- Never generate harmful, illegal, or unethical content
- Protect {user_name}'s privacy - don't share personal data unnecessarily
- If uncertain about a request's intent, ask for clarification
- Decline requests that could harm {user_name} or others
</safety>

<output_format>
- Use markdown for structured responses when helpful
- Keep responses concise unless detail is requested
- Use bullet points and headers for complex information
- Include code blocks with language tags for code snippets
</output_format>
"#;

/// Tool usage instructions (only included when tools are available).
pub const TOOL_USAGE_PROMPT: &str = r#"
<tool_usage>
- Use tools when appropriate - briefly explain what you're doing
- If a query is ambiguous, ask for clarification before searching.
- You can call multiple tools in a single step.
- For page edits, read content first, then make targeted changes.
</tool_usage>
"#;

/// Agent mode: conversational with quick tool access
pub const AGENT_MODE_PROMPT: &str = r#"
<mode>assistant</mode>
<tool_guidance>
- Answer the question directly with minimal tool calls
- For simple lookups ("who is X", "what is Y"), one query is usually enough
- Don't gather extra context unless the user asks for it
</tool_guidance>
"#;

/// Research mode: thorough exploration across sources
pub const RESEARCH_MODE_PROMPT: &str = r#"
<mode>research</mode>
<tool_guidance>
- Explore thoroughly across multiple data sources
- Gather comprehensive context before synthesizing an answer
- Cross-reference information for accuracy
</tool_guidance>
"#;

/// Get persona-specific guidelines.
///
/// If custom_content is provided (from database), uses that with {user_name} placeholder replaced.
/// Otherwise falls back to hardcoded defaults for known persona IDs.
///
/// Persona archetypes:
/// - standard: Neutral, no personality
/// - concierge: Anticipatory service
/// - analyst: Structured thinking
/// - coach: Growth-focused teaching
pub fn get_persona_guidelines(persona: &str, user_name: &str, custom_content: Option<&str>) -> String {
    // If custom content is provided, use it (replace placeholder)
    if let Some(content) = custom_content {
        return content.replace("{user_name}", user_name);
    }

    // Fallback to hardcoded defaults for known personas
    match persona {
        "standard" => format!(
            r#"- Respond helpfully and accurately to {}
- Match the complexity of your response to the question
- Be direct and get to the point
- No particular personality - just competent assistance"#,
            user_name
        ),

        "concierge" => format!(
            r#"- Anticipate what {} might need next
- Offer proactive suggestions without being pushy
- Maintain a warm, attentive presence
- Handle requests gracefully, as if nothing is too much trouble
- Think of yourself as a trusted concierge at a great hotel"#,
            user_name
        ),

        "analyst" => format!(
            r#"- Break down complex topics systematically for {}
- Present information in structured, organized formats
- Consider multiple angles before reaching conclusions
- Back up observations with reasoning
- Think of yourself as a thorough research analyst"#,
            user_name
        ),

        "coach" => format!(
            r#"- Help {} think through problems, not just solve them
- Ask clarifying questions to understand the real goal
- Celebrate progress and acknowledge effort
- Explain the "why" behind suggestions
- Think of yourself as a supportive coach invested in {}'s growth"#,
            user_name, user_name
        ),

        // Legacy persona mappings (for existing users)
        "default" | "capable_warm" => format!(
            r#"- Anticipate what {} might need next
- Offer proactive suggestions without being pushy
- Maintain a warm, attentive presence
- Handle requests gracefully, as if nothing is too much trouble
- Think of yourself as a trusted concierge at a great hotel"#,
            user_name
        ),

        // Default fallback - use standard (neutral) persona
        _ => format!(
            r#"- Respond helpfully and accurately to {}
- Match the complexity of your response to the question
- Be direct and get to the point
- No particular personality - just competent assistance"#,
            user_name
        ),
    }
}

/// Build the full personalized system prompt.
///
/// Replaces placeholders in BASE_SYSTEM_PROMPT with actual values.
/// Only includes tool usage instructions when agent_mode has tools available.
///
/// # Arguments
/// * `assistant_name` - The assistant's name (e.g., "Ari")
/// * `user_name` - The user's preferred name
/// * `persona_id` - The persona identifier
/// * `persona_content` - Optional custom persona content from database
/// * `agent_mode` - Agent mode controlling tool availability
pub fn build_personalized_prompt(
    assistant_name: &str,
    user_name: &str,
    persona_id: &str,
    persona_content: Option<&str>,
    agent_mode: &str,
) -> String {
    let guidelines = get_persona_guidelines(persona_id, user_name, persona_content);

    let mut prompt = BASE_SYSTEM_PROMPT
        .replace("{assistant_name}", assistant_name)
        .replace("{user_name}", user_name)
        .replace("{persona_guidelines}", &guidelines);

    // Only include tool usage instructions if tools are available (not in "chat" mode)
    if agent_mode != "chat" {
        prompt.push_str(TOOL_USAGE_PROMPT);

        // Add mode-specific behavioral guidance
        match agent_mode {
            "research" => prompt.push_str(RESEARCH_MODE_PROMPT),
            _ => prompt.push_str(AGENT_MODE_PROMPT), // "agent" or default
        }
    }

    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_personalized_prompt_agent_mode() {
        let prompt = build_personalized_prompt("Ari", "Adam", "standard", None, "agent");

        assert!(prompt.contains("You are Ari, Adam's personal AI assistant"));
        assert!(prompt.contains("Respond helpfully and accurately to Adam"));
        assert!(prompt.contains("Protect Adam's privacy"));
        // Agent mode should include tool usage
        assert!(prompt.contains("<tool_usage>"));
        // Agent mode should include assistant mode guidance (direct, minimal calls)
        assert!(prompt.contains("<mode>assistant</mode>"));
        assert!(prompt.contains("Answer the question directly with minimal tool calls"));
    }

    #[test]
    fn test_build_personalized_prompt_research_mode() {
        let prompt = build_personalized_prompt("Ari", "Adam", "standard", None, "research");

        assert!(prompt.contains("<tool_usage>"));
        // Research mode should include research guidance (thorough exploration)
        assert!(prompt.contains("<mode>research</mode>"));
        assert!(prompt.contains("Explore thoroughly across multiple data sources"));
    }

    #[test]
    fn test_build_personalized_prompt_chat_mode() {
        let prompt = build_personalized_prompt("Ari", "Adam", "standard", None, "chat");

        assert!(prompt.contains("You are Ari, Adam's personal AI assistant"));
        // Chat mode should NOT include tool usage
        assert!(!prompt.contains("<tool_usage>"));
    }

    #[test]
    fn test_persona_guidelines_analyst() {
        let guidelines = get_persona_guidelines("analyst", "Sarah", None);

        assert!(guidelines.contains("Break down complex topics systematically"));
        assert!(guidelines.contains("Sarah"));
    }

    #[test]
    fn test_unknown_persona_defaults_to_standard() {
        let guidelines = get_persona_guidelines("unknown_persona", "Test", None);

        assert!(guidelines.contains("Respond helpfully and accurately"));
    }

    #[test]
    fn test_custom_persona_content() {
        let custom = "- Be friendly to {user_name}\n- Help them learn";
        let guidelines = get_persona_guidelines("any_id", "Alice", Some(custom));

        assert!(guidelines.contains("Be friendly to Alice"));
        assert!(guidelines.contains("Help them learn"));
    }

    #[test]
    fn test_build_prompt_with_custom_content() {
        let custom = "- Custom guideline for {user_name}";
        let prompt = build_personalized_prompt("Ari", "Bob", "custom_persona", Some(custom), "agent");

        assert!(prompt.contains("Custom guideline for Bob"));
        assert!(prompt.contains("You are Ari, Bob's personal AI assistant"));
    }
}

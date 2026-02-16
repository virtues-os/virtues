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

<output_format>
- Use markdown for structured responses when helpful
- Keep responses concise unless detail is requested
- Don't pad, hedge, or over-qualify. Silence is better than filler.
- Prioritize understanding over output — help {user_name} see clearly, not just get things done.
- Use bullet points and headers for complex information
- Include code blocks with language tags for code snippets
</output_format>
"#;

/// Narrative identity framing — the AI's relationship to user self-knowledge.
///
/// Always present (persona-independent). The user's narrative identity content
/// is injected by build_system_prompt() in chat.rs via the {narrative_identity} placeholder.
pub const NARRATIVE_IDENTITY_PROMPT: &str = r#"
<narrative_identity>
{user_name} may have written a narrative identity — who they are, what they believe, what they're working on in themselves, what direction they're facing. This includes things they trust you to know but do not want repeated back: struggles, vices, faith, temperament. Read it. Absorb it. Then mostly forget you read it.

Most conversations don't need this context at all. A math question is a math question. A recipe is a recipe. News is news. Do not manufacture connections between routine queries and someone's narrative identity. The fastest way to lose trust is to psychoanalyze a shopping list.

When it IS relevant — decisions about priorities, questions about direction, moments of self-doubt, reflections on habits — let it inform your tone and framing naturally. Don't quote it. Don't reference it explicitly. Don't say "based on your narrative identity" or "I notice that aligns with your stated values." Just be a better assistant because you understand them.

- Never lecture, nudge, or coach unless asked
- Never resurface struggles, vices, or private admissions
- Hold your understanding lightly — you could be wrong about what matters to them right now
- When in doubt, just answer the question

{narrative_identity}
</narrative_identity>
"#;

/// Tool usage instructions (only included when tools are available).
pub const TOOL_USAGE_PROMPT: &str = r#"
<tool_usage>
- Use the think tool before complex multi-step tasks to plan your approach
- You can call multiple tools in a single step when they're independent
- If a query returns no results, try a broader search before giving up
- When uncertain about table structure, use get_schema first
- For page edits, read content first with get_page_content, then make targeted changes
- If edit_page returns permission_needed, briefly ask the user to grant permission. The UI shows an approval button — just acknowledge you're waiting.
- If a query is ambiguous, ask for clarification before searching
</tool_usage>
"#;

/// Agent mode: conversational with quick tool access
pub const AGENT_MODE_PROMPT: &str = r#"
<mode>assistant</mode>
<tool_guidance>
- For simple lookups, one query is usually enough. For multi-step tasks, use as many tools as needed
- Don't gather extra context unless the user asks for it
- Do NOT use tools for: conversational replies, opinions, follow-ups on data already in context

Common SQL patterns:
- Time filtering: WHERE timestamp > datetime('now', '-7 days')
- This month: WHERE timestamp >= date('now', 'start of month')
- Person lookup: JOIN wiki_people ON ... WHERE name LIKE '%Sarah%'
- Financial totals: SELECT category, SUM(amount)/100.0 as dollars FROM data_financial_transaction ...
- Aggregation: GROUP BY + ORDER BY for top-N patterns
</tool_guidance>
"#;

/// Research mode: thorough exploration across sources
pub const RESEARCH_MODE_PROMPT: &str = r#"
<mode>research</mode>
<tool_guidance>
- Start with the think tool to plan your research approach
- Recommended workflow: think → semantic_search → sql_query → web_search → code_interpreter → synthesize
- Explore thoroughly across multiple data sources before synthesizing
- Cross-reference information for accuracy
- Use sql_query for structured data: aggregates, time series, exact filters, counts
- Use semantic_search for conceptual/fuzzy queries across all your data
- Use web_search for external context: news, definitions, current events
- Use code_interpreter for: calculations, statistical analysis, data transformations
- Gather all relevant data before writing your final response
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
/// Includes narrative identity framing (always present) and tool instructions (when tools available).
///
/// # Arguments
/// * `assistant_name` - The assistant's name (e.g., "Ari")
/// * `user_name` - The user's preferred name
/// * `persona_id` - The persona identifier
/// * `persona_content` - Optional custom persona content from database
/// * `agent_mode` - Agent mode controlling tool availability
/// * `narrative_identity` - User's narrative identity content (empty string if none set)
pub fn build_personalized_prompt(
    assistant_name: &str,
    user_name: &str,
    persona_id: &str,
    persona_content: Option<&str>,
    agent_mode: &str,
    narrative_identity: &str,
) -> String {
    let guidelines = get_persona_guidelines(persona_id, user_name, persona_content);

    let mut prompt = BASE_SYSTEM_PROMPT
        .replace("{assistant_name}", assistant_name)
        .replace("{user_name}", user_name)
        .replace("{persona_guidelines}", &guidelines);

    // Narrative identity section — always present (persona-independent)
    prompt.push_str(
        &NARRATIVE_IDENTITY_PROMPT
            .replace("{user_name}", user_name)
            .replace("{narrative_identity}", narrative_identity),
    );

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
        let prompt = build_personalized_prompt("Ari", "Adam", "standard", None, "agent", "");

        assert!(prompt.contains("You are Ari, Adam's personal AI assistant"));
        assert!(prompt.contains("Respond helpfully and accurately to Adam"));
        // Narrative identity section always present
        assert!(prompt.contains("<narrative_identity>"));
        assert!(prompt.contains("just answer the question"));
        // Agent mode should include tool usage
        assert!(prompt.contains("<tool_usage>"));
        assert!(prompt.contains("Use the think tool before complex"));
        // Agent mode should include assistant mode guidance
        assert!(prompt.contains("<mode>assistant</mode>"));
        assert!(prompt.contains("For simple lookups, one query is usually enough"));
    }

    #[test]
    fn test_build_personalized_prompt_research_mode() {
        let prompt = build_personalized_prompt("Ari", "Adam", "standard", None, "research", "");

        assert!(prompt.contains("<tool_usage>"));
        // Research mode should include research guidance (thorough exploration)
        assert!(prompt.contains("<mode>research</mode>"));
        assert!(prompt.contains("Start with the think tool to plan your research approach"));
    }

    #[test]
    fn test_build_personalized_prompt_chat_mode() {
        let prompt = build_personalized_prompt("Ari", "Adam", "standard", None, "chat", "");

        assert!(prompt.contains("You are Ari, Adam's personal AI assistant"));
        // Chat mode should NOT include tool usage
        assert!(!prompt.contains("<tool_usage>"));
        // But narrative identity should still be present
        assert!(prompt.contains("<narrative_identity>"));
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
        let prompt = build_personalized_prompt("Ari", "Bob", "custom_persona", Some(custom), "agent", "");

        assert!(prompt.contains("Custom guideline for Bob"));
        assert!(prompt.contains("You are Ari, Bob's personal AI assistant"));
    }

    #[test]
    fn test_narrative_identity_section_with_data() {
        let prompt = build_personalized_prompt(
            "Ari", "Adam", "standard", None, "agent",
            "I am a builder and teacher. I care about craft, clarity, and helping others grow.",
        );

        assert!(prompt.contains("<narrative_identity>"));
        assert!(prompt.contains("I am a builder and teacher"));
        assert!(prompt.contains("helping others grow"));
        // Narrative identity should appear before tool_usage
        let ni_pos = prompt.find("<narrative_identity>").unwrap();
        let tool_pos = prompt.find("<tool_usage>").unwrap();
        assert!(ni_pos < tool_pos, "narrative_identity should appear before tool_usage");
    }

    #[test]
    fn test_narrative_identity_section_empty_data() {
        let prompt = build_personalized_prompt("Ari", "Adam", "standard", None, "agent", "");

        assert!(prompt.contains("<narrative_identity>"));
        // Static framing should still be present even with no data
        assert!(prompt.contains("Do not manufacture connections"));
        assert!(prompt.contains("Never lecture, nudge, or coach unless asked"));
    }
}

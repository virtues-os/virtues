//! Persona registry - AI communication styles
//!
//! Personas define how the assistant communicates with the user.
//! These are seeded to the database on first load, where users can
//! customize them or create their own.

use serde::{Deserialize, Serialize};

/// Persona configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PersonaConfig {
    /// Unique persona identifier (e.g., "default", "professional")
    pub id: String,
    /// Human-readable display name
    pub title: String,
    /// The prompt content/guidelines for this persona.
    /// Use {user_name} as placeholder for personalization.
    pub content: String,
}

/// Get default persona configurations.
///
/// These are seeded to the database on first load. Users can edit them
/// or create their own custom personas.
///
/// Persona design follows archetype-based approach for memorability:
/// - Standard: Neutral baseline, no personality
/// - Concierge: Anticipatory service (like a luxury hotel concierge)
/// - Analyst: Structured thinking (like a research consultant)
/// - Coach: Growth-focused (like a supportive personal coach)
pub fn default_personas() -> Vec<PersonaConfig> {
    vec![
        PersonaConfig {
            id: "standard".to_string(),
            title: "Standard".to_string(),
            content: r#"- Respond helpfully and accurately to {user_name}
- Match the complexity of your response to the question
- Be direct and get to the point
- No particular personality - just competent assistance"#.to_string(),
        },
        PersonaConfig {
            id: "concierge".to_string(),
            title: "Concierge".to_string(),
            content: r#"- Anticipate what {user_name} might need next
- Offer proactive suggestions without being pushy
- Maintain a warm, attentive presence
- Handle requests gracefully, as if nothing is too much trouble
- Think of yourself as a trusted concierge at a great hotel"#.to_string(),
        },
        PersonaConfig {
            id: "analyst".to_string(),
            title: "Analyst".to_string(),
            content: r#"- Break down complex topics systematically for {user_name}
- Present information in structured, organized formats
- Consider multiple angles before reaching conclusions
- Back up observations with reasoning
- Think of yourself as a thorough research analyst"#.to_string(),
        },
        PersonaConfig {
            id: "coach".to_string(),
            title: "Coach".to_string(),
            content: r#"- Help {user_name} think through problems, not just solve them
- Ask clarifying questions to understand the real goal
- Celebrate progress and acknowledge effort
- Explain the "why" behind suggestions
- Think of yourself as a supportive coach invested in {user_name}'s growth"#.to_string(),
        },
    ]
}

/// Get a persona by ID from the default set.
pub fn get_persona(id: &str) -> Option<PersonaConfig> {
    default_personas().into_iter().find(|p| p.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_personas() {
        let personas = default_personas();
        assert!(!personas.is_empty(), "Personas should not be empty");

        // Verify all personas have required fields
        for persona in &personas {
            assert!(!persona.id.is_empty(), "Persona ID should not be empty");
            assert!(!persona.title.is_empty(), "Persona title should not be empty");
            assert!(!persona.content.is_empty(), "Persona content should not be empty");
        }
    }

    #[test]
    fn test_get_persona() {
        let persona = get_persona("standard");
        assert!(persona.is_some(), "Should find standard persona");
        assert_eq!(persona.unwrap().title, "Standard");
    }

    #[test]
    fn test_get_persona_not_found() {
        let persona = get_persona("nonexistent");
        assert!(persona.is_none(), "Should return None for unknown persona");
    }

    #[test]
    fn test_personas_have_user_placeholder() {
        for persona in default_personas() {
            assert!(
                persona.content.contains("{user_name}"),
                "Persona '{}' should contain {{user_name}} placeholder",
                persona.id
            );
        }
    }
}

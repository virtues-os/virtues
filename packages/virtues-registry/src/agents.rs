//! Agent registry - Assistant personas
//!
//! Agents are static configuration defining different "modes" of the assistant.
//! Currently there is one default agent, but this could be extended.

use serde::{Deserialize, Serialize};

/// Agent configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentConfig {
    /// Unique agent identifier
    pub agent_id: String,
    /// Human-readable name
    pub name: String,
    /// Description of what this agent does
    pub description: String,
    /// UI color (hex)
    pub color: String,
    /// Iconify icon name
    pub icon: String,
    /// Default model for this agent
    pub default_model: String,
    /// Maximum agentic steps before forcing completion
    pub max_steps: i32,
    /// Whether this agent is enabled
    pub enabled: bool,
    /// Sort order for UI display
    pub sort_order: i32,
}

/// Get default agent configurations
pub fn default_agents() -> Vec<AgentConfig> {
    vec![AgentConfig {
        agent_id: "agent".to_string(),
        name: "Agent".to_string(),
        description: "Intelligent assistant with access to all available tools. Can query data, search the web, visualize information, and help with tasks.".to_string(),
        color: "#6b7280".to_string(),
        icon: "ri:robot-line".to_string(),
        default_model: "anthropic/claude-sonnet-4-20250514".to_string(),
        max_steps: 5,
        enabled: true,
        sort_order: 1,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_agents() {
        let agents = default_agents();
        assert!(!agents.is_empty(), "Agents should not be empty");

        // Verify all agents have required fields
        for agent in &agents {
            assert!(!agent.agent_id.is_empty());
            assert!(!agent.name.is_empty());
            assert!(!agent.default_model.is_empty());
        }
    }
}

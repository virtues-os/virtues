//! API functions for agent management
//!
//! Agents are read directly from the shared virtues-registry crate.
//! No SQLite tables needed for static agent configuration.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Agent information returned by API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub agent_id: String,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub enabled: bool,
    pub sort_order: i32,
}

impl From<virtues_registry::agents::AgentConfig> for AgentInfo {
    fn from(config: virtues_registry::agents::AgentConfig) -> Self {
        Self {
            agent_id: config.agent_id,
            name: config.name,
            description: Some(config.description),
            color: Some(config.color),
            icon: Some(config.icon),
            enabled: config.enabled,
            sort_order: config.sort_order,
        }
    }
}

/// List all enabled agents from the registry
pub async fn list_agents() -> Result<Vec<AgentInfo>> {
    let agents: Vec<AgentInfo> = virtues_registry::agents::default_agents()
        .into_iter()
        .filter(|a| a.enabled)
        .map(AgentInfo::from)
        .collect();

    Ok(agents)
}

/// Get a specific agent by ID
pub async fn get_agent(agent_id: &str) -> Result<AgentInfo> {
    let agent = virtues_registry::agents::default_agents()
        .into_iter()
        .find(|a| a.agent_id == agent_id)
        .map(AgentInfo::from)
        .ok_or_else(|| Error::NotFound(format!("Agent not found: {}", agent_id)))?;

    Ok(agent)
}

//! API functions for agent management

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::Result;

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

/// List all system default agents (user_id IS NULL)
pub async fn list_agents(db: &SqlitePool) -> Result<Vec<AgentInfo>> {
    // SQLite returns INTEGER as i64, need explicit type casts
    let agents = sqlx::query_as!(
        AgentInfo,
        r#"
        SELECT
            agent_id,
            name,
            description,
            color,
            icon,
            enabled as "enabled: bool",
            sort_order as "sort_order: i32"
        FROM app_agents
        WHERE user_id IS NULL AND enabled = true
        ORDER BY sort_order ASC
        "#
    )
    .fetch_all(db)
    .await?;

    Ok(agents)
}

/// Get a specific agent by ID
pub async fn get_agent(db: &SqlitePool, agent_id: &str) -> Result<AgentInfo> {
    // SQLite returns INTEGER as i64, need explicit type casts
    let agent = sqlx::query_as!(
        AgentInfo,
        r#"
        SELECT
            agent_id,
            name,
            description,
            color,
            icon,
            enabled as "enabled: bool",
            sort_order as "sort_order: i32"
        FROM app_agents
        WHERE user_id IS NULL AND agent_id = $1
        "#,
        agent_id
    )
    .fetch_one(db)
    .await?;

    Ok(agent)
}

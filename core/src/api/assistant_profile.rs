//! Assistant profile API
//!
//! This module provides functions for managing the user's AI assistant preferences.
//! The assistant profile is a singleton table containing AI/agent configuration.

use crate::error::{Error, Result};
use crate::storage::models::AssistantProfile;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// Request to update assistant profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAssistantProfileRequest {
    pub assistant_name: Option<String>,
    pub default_agent_id: Option<String>,
    // Legacy fields (kept for backward compatibility)
    pub default_model_id: Option<String>,
    pub background_model_id: Option<String>,
    // New model slot system
    pub chat_model_id: Option<String>,
    pub lite_model_id: Option<String>,
    pub reasoning_model_id: Option<String>,
    pub coding_model_id: Option<String>,
    pub enabled_tools: Option<serde_json::Value>,
    pub ui_preferences: Option<serde_json::Value>,
    /// AI persona/tone: capable_warm, professional, casual, adaptive
    pub persona: Option<String>,
}

/// Get the assistant profile (singleton row)
///
/// This will always return a profile, as the migration creates an empty row by default.
pub async fn get_assistant_profile(db: &SqlitePool) -> Result<AssistantProfile> {
    let profile = sqlx::query_as::<_, AssistantProfile>(
        r#"
        SELECT *
        FROM app_assistant_profile
        LIMIT 1
        "#,
    )
    .fetch_one(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch assistant profile: {}", e)))?;

    Ok(profile)
}

/// Update the assistant profile
///
/// Only updates fields that are present in the request (not None).
/// Returns the updated profile.
pub async fn update_assistant_profile(
    db: &SqlitePool,
    request: UpdateAssistantProfileRequest,
) -> Result<AssistantProfile> {
    // The singleton ID (stored as TEXT in SQLite)
    let profile_id = "00000000-0000-0000-0000-000000000001";

    // Build dynamic UPDATE query based on which fields are present
    let mut set_clauses = Vec::new();
    let mut param_idx = 1;

    // Helper macro to add SET clause
    macro_rules! add_field {
        ($field:expr, $name:literal) => {
            if $field.is_some() {
                set_clauses.push(format!("{} = ${}", $name, param_idx));
                param_idx += 1;
            }
        };
    }

    add_field!(request.assistant_name, "assistant_name");
    add_field!(request.default_agent_id, "default_agent_id");
    add_field!(request.default_model_id, "default_model_id");
    add_field!(request.background_model_id, "background_model_id");
    add_field!(request.chat_model_id, "chat_model_id");
    add_field!(request.lite_model_id, "lite_model_id");
    add_field!(request.reasoning_model_id, "reasoning_model_id");
    add_field!(request.coding_model_id, "coding_model_id");
    add_field!(request.enabled_tools, "enabled_tools");
    add_field!(request.ui_preferences, "ui_preferences");
    add_field!(request.persona, "persona");

    if set_clauses.is_empty() {
        // No updates requested, just return current profile
        return get_assistant_profile(db).await;
    }

    let query = format!(
        "UPDATE app_assistant_profile SET {}, updated_at = datetime('now') WHERE id = ${} RETURNING *",
        set_clauses.join(", "),
        param_idx
    );

    // Build query with bound parameters (only bind non-None values)
    let mut q = sqlx::query_as::<_, AssistantProfile>(&query);

    if let Some(v) = &request.assistant_name {
        q = q.bind(v);
    }
    if let Some(v) = &request.default_agent_id {
        q = q.bind(v);
    }
    if let Some(v) = &request.default_model_id {
        q = q.bind(v);
    }
    if let Some(v) = &request.background_model_id {
        q = q.bind(v);
    }
    if let Some(v) = &request.chat_model_id {
        q = q.bind(v);
    }
    if let Some(v) = &request.lite_model_id {
        q = q.bind(v);
    }
    if let Some(v) = &request.reasoning_model_id {
        q = q.bind(v);
    }
    if let Some(v) = &request.coding_model_id {
        q = q.bind(v);
    }
    if let Some(v) = &request.enabled_tools {
        q = q.bind(v);
    }
    if let Some(v) = &request.ui_preferences {
        q = q.bind(v);
    }
    if let Some(v) = &request.persona {
        q = q.bind(v);
    }
    q = q.bind(profile_id);

    let updated_profile = q
        .fetch_one(db)
        .await
        .map_err(|e| Error::Database(format!("Failed to update assistant profile: {}", e)))?;

    Ok(updated_profile)
}

/// Helper to get the assistant's name for system prompts
///
/// Returns assistant_name if set, otherwise "Assistant"
pub async fn get_assistant_name(db: &SqlitePool) -> Result<String> {
    let profile = get_assistant_profile(db).await?;

    Ok(profile
        .assistant_name
        .unwrap_or_else(|| "Assistant".to_string()))
}

/// Helper to get the lite/background model for cheap tasks (titles, summaries)
///
/// Uses new lite_model_id slot, falls back to background_model_id for compatibility,
/// then to default lite model
pub async fn get_background_model(db: &SqlitePool) -> Result<String> {
    let profile = get_assistant_profile(db).await?;

    // Try new slot first, then legacy field, then default
    Ok(profile
        .lite_model_id
        .or(profile.background_model_id)
        .unwrap_or_else(|| virtues_registry::models::default_model_for_slot(
            virtues_registry::models::ModelSlot::Lite
        ).to_string()))
}

/// Helper to get the chat model (default for conversations)
pub async fn get_chat_model(db: &SqlitePool) -> Result<String> {
    let profile = get_assistant_profile(db).await?;

    Ok(profile
        .chat_model_id
        .or(profile.default_model_id)
        .unwrap_or_else(|| virtues_registry::models::default_model_for_slot(
            virtues_registry::models::ModelSlot::Chat
        ).to_string()))
}

/// Helper to get the reasoning model (complex analysis)
pub async fn get_reasoning_model(db: &SqlitePool) -> Result<String> {
    let profile = get_assistant_profile(db).await?;

    Ok(profile
        .reasoning_model_id
        .unwrap_or_else(|| virtues_registry::models::default_model_for_slot(
            virtues_registry::models::ModelSlot::Reasoning
        ).to_string()))
}

/// Helper to get the coding model (code generation)
pub async fn get_coding_model(db: &SqlitePool) -> Result<String> {
    let profile = get_assistant_profile(db).await?;

    Ok(profile
        .coding_model_id
        .unwrap_or_else(|| virtues_registry::models::default_model_for_slot(
            virtues_registry::models::ModelSlot::Coding
        ).to_string()))
}

/// Helper to get the AI persona for system prompts
///
/// Returns persona if set, otherwise "capable_warm" (default)
/// Valid values: capable_warm, professional, casual, adaptive
pub async fn get_persona(db: &SqlitePool) -> Result<String> {
    let profile = get_assistant_profile(db).await?;

    Ok(profile
        .persona
        .unwrap_or_else(|| "capable_warm".to_string()))
}

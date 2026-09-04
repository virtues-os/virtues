//! Assistant profile API
//!
//! This module provides functions for managing the user's AI assistant preferences.
//! The assistant profile is a singleton table containing AI/agent configuration.

use crate::error::{Error, Result};
use crate::storage::models::AssistantProfile;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

/// Deserialize a field that must distinguish three states:
///
///   key absent   → `None`         → leave the column alone
///   `null`       → `Some(None)`   → SET the column to NULL
///   `"model-id"` → `Some(Some(_))`→ SET the column to that value
///
/// Plain `Option<String>` cannot do this: serde folds both *absent* and *null*
/// into `None`, so `{"chat_model_id": null}` was silently a no-op and a pinned
/// slot could never be un-pinned. That is exactly what "Virtues default" needs
/// to write — see `ModelSettings.svelte`.
fn double_option<'de, D, T>(de: D) -> std::result::Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Deserialize::deserialize(de).map(Some)
}

/// Request to update assistant profile
///
/// Every field is optional (absent = don't touch). The four model slots are
/// *doubly* optional, because clearing a slot back to "Virtues default" means
/// writing NULL, and NULL has to be distinguishable from "not mentioned".
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateAssistantProfileRequest {
    pub assistant_name: Option<String>,
    pub default_agent_id: Option<String>,
    // Model slots. NULL here = "follow the Virtues default" (the cloud slot
    // map, then the compiled floor — see api::model_catalog).
    #[serde(deserialize_with = "double_option")]
    pub chat_model_id: Option<Option<String>>,
    #[serde(deserialize_with = "double_option")]
    pub lite_model_id: Option<Option<String>>,
    #[serde(deserialize_with = "double_option")]
    pub coding_model_id: Option<Option<String>>,
    #[serde(deserialize_with = "double_option")]
    pub image_model_id: Option<Option<String>>,
    pub enabled_tools: Option<serde_json::Value>,
    pub ui_preferences: Option<serde_json::Value>,
    /// AI persona/tone: capable_warm, professional, casual, adaptive
    pub persona: Option<String>,
}

/// Get the assistant profile (singleton row)
///
/// This will always return a profile, as the migration creates an empty row by default.
pub async fn get_assistant_profile(db: &PgPool) -> Result<AssistantProfile> {
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
    db: &PgPool,
    request: UpdateAssistantProfileRequest,
) -> Result<AssistantProfile> {
    // The singleton ID (stored as TEXT)
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
    add_field!(request.chat_model_id, "chat_model_id");
    add_field!(request.lite_model_id, "lite_model_id");
    add_field!(request.coding_model_id, "coding_model_id");
    add_field!(request.image_model_id, "image_model_id");
    add_field!(request.enabled_tools, "enabled_tools");
    add_field!(request.ui_preferences, "ui_preferences");
    add_field!(request.persona, "persona");

    if set_clauses.is_empty() {
        // No updates requested, just return current profile
        return get_assistant_profile(db).await;
    }

    let query = format!(
        "UPDATE app_assistant_profile SET {}, updated_at = now() WHERE id = ${} RETURNING *",
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
    // Double-option: the outer Some means "this field was mentioned"; the inner
    // Option is the value, and `None` binds as SQL NULL — which is how a slot
    // gets reset to the Virtues default.
    if let Some(v) = &request.chat_model_id {
        q = q.bind(v.as_deref());
    }
    if let Some(v) = &request.lite_model_id {
        q = q.bind(v.as_deref());
    }
    if let Some(v) = &request.coding_model_id {
        q = q.bind(v.as_deref());
    }
    if let Some(v) = &request.image_model_id {
        q = q.bind(v.as_deref());
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
/// Returns assistant_name if set, otherwise the default "Ari"
pub async fn get_assistant_name(db: &PgPool) -> Result<String> {
    let profile = get_assistant_profile(db).await?;

    Ok(profile
        .assistant_name
        .unwrap_or_else(|| "Ari".to_string()))
}

/// Helper to get the lite/background model for cheap tasks (titles, summaries)
pub async fn get_background_model(db: &PgPool) -> Result<String> {
    let profile = get_assistant_profile(db).await?;

    Ok(profile
        .lite_model_id
        .unwrap_or_else(|| crate::api::model_catalog::model_for_slot(
            virtues_registry::models::ModelSlot::Lite
        )))
}

/// Helper to get the chat model (default for conversations)
///
/// Pin, else the Virtues default — never the legacy default_model_id column.
/// That column held a SNAPSHOT of the registry default frozen at seed time, so
/// its `.or()` fallback kept serving a model Virtues had since moved off of,
/// wearing the costume of a user pin.
pub async fn get_chat_model(db: &PgPool) -> Result<String> {
    let profile = get_assistant_profile(db).await?;

    Ok(profile
        .chat_model_id
        .unwrap_or_else(|| crate::api::model_catalog::model_for_slot(
            virtues_registry::models::ModelSlot::Chat
        )))
}

/// Helper to get the coding model (code generation)
pub async fn get_coding_model(db: &PgPool) -> Result<String> {
    let profile = get_assistant_profile(db).await?;

    Ok(profile
        .coding_model_id
        .unwrap_or_else(|| crate::api::model_catalog::model_for_slot(
            virtues_registry::models::ModelSlot::Coding
        )))
}

/// Helper to get the image model (text-to-image generation)
pub async fn get_image_model(db: &PgPool) -> Result<String> {
    let profile = get_assistant_profile(db).await?;

    Ok(profile
        .image_model_id
        .unwrap_or_else(|| crate::api::model_catalog::model_for_slot(
            virtues_registry::models::ModelSlot::Image
        )))
}

/// Helper to get the AI persona for system prompts
///
/// Returns persona if set, otherwise "capable_warm" (default)
/// Valid values: capable_warm, professional, casual, adaptive
pub async fn get_persona(db: &PgPool) -> Result<String> {
    let profile = get_assistant_profile(db).await?;

    Ok(profile
        .persona
        .unwrap_or_else(|| "capable_warm".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three states a slot field must be able to express. Collapsing any two
    /// of them breaks something: fold null into absent and a pinned slot can
    /// never be released (the "Virtues default" option silently does nothing);
    /// fold absent into null and every partial PUT wipes the slots it didn't
    /// mention.
    #[test]
    fn slot_distinguishes_absent_from_null_from_value() {
        let absent: UpdateAssistantProfileRequest =
            serde_json::from_str(r#"{"persona":"casual"}"#).unwrap();
        assert_eq!(absent.chat_model_id, None, "absent = leave the column alone");

        let cleared: UpdateAssistantProfileRequest =
            serde_json::from_str(r#"{"chat_model_id":null}"#).unwrap();
        assert_eq!(
            cleared.chat_model_id,
            Some(None),
            "explicit null = SET NULL = follow the Virtues default"
        );

        let pinned: UpdateAssistantProfileRequest =
            serde_json::from_str(r#"{"chat_model_id":"anthropic/claude-opus-4.8"}"#).unwrap();
        assert_eq!(
            pinned.chat_model_id,
            Some(Some("anthropic/claude-opus-4.8".to_string())),
            "a value = pin it"
        );
    }

    /// Old clients still send the retired legacy columns
    /// (default_model_id/background_model_id) alongside their slot. Those keys
    /// must be silently ignored — not an error — or every stale SPA's model
    /// save starts failing on upgrade.
    #[test]
    fn retired_legacy_keys_from_old_clients_are_ignored() {
        let req: UpdateAssistantProfileRequest = serde_json::from_str(
            r#"{"chat_model_id":null,"default_model_id":null,"background_model_id":"x/y"}"#,
        )
        .unwrap();
        assert_eq!(req.chat_model_id, Some(None));
    }
}

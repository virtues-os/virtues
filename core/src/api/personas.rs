//! Persona API
//!
//! This module provides CRUD operations for AI personas.
//! Personas define how the assistant communicates with the user.
//!
//! System personas are seeded from virtues-registry on first access.
//! Users can customize system personas, create custom ones, and hide any.

use crate::error::{Error, Result};
use crate::ids;
use crate::storage::models::AssistantProfile;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// A persona definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Persona {
    pub id: String,
    pub title: String,
    pub content: String,
    /// True for personas seeded from registry (can hide but not delete)
    pub is_system: bool,
}

/// Storage format for personas in the database JSON column
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersonasData {
    /// All persona definitions (system + custom)
    pub items: Vec<Persona>,
    /// IDs of hidden personas
    pub hidden: Vec<String>,
}

/// Request to create a new custom persona
#[derive(Debug, Clone, Deserialize)]
pub struct CreatePersonaRequest {
    pub title: String,
    pub content: String,
}

/// Request to update an existing persona
#[derive(Debug, Clone, Deserialize)]
pub struct UpdatePersonaRequest {
    pub title: Option<String>,
    pub content: Option<String>,
}

/// Response for persona list
#[derive(Debug, Clone, Serialize)]
pub struct PersonaListResponse {
    pub personas: Vec<Persona>,
}

// ============================================================================
// Internal helpers
// ============================================================================

/// Get the assistant profile singleton
async fn get_profile(db: &SqlitePool) -> Result<AssistantProfile> {
    sqlx::query_as::<_, AssistantProfile>("SELECT * FROM app_assistant_profile LIMIT 1")
        .fetch_one(db)
        .await
        .map_err(|e| Error::Database(format!("Failed to fetch assistant profile: {}", e)))
}

/// Parse personas JSON from profile, returning default if empty/invalid
fn parse_personas_data(profile: &AssistantProfile) -> PersonasData {
    profile
        .personas
        .as_ref()
        .and_then(|p| serde_json::from_str(p).ok())
        .unwrap_or_default()
}

/// Save personas data back to the profile
async fn save_personas_data(db: &SqlitePool, data: &PersonasData) -> Result<()> {
    let json = serde_json::to_string(data)
        .map_err(|e| Error::Other(format!("Failed to serialize personas: {}", e)))?;

    sqlx::query("UPDATE app_assistant_profile SET personas = ?, updated_at = datetime('now')")
        .bind(&json)
        .execute(db)
        .await
        .map_err(|e| Error::Database(format!("Failed to save personas: {}", e)))?;

    Ok(())
}

/// Seed system personas from registry if none exist
async fn seed_if_empty(db: &SqlitePool, data: &mut PersonasData) -> Result<bool> {
    if !data.items.is_empty() {
        return Ok(false);
    }

    // Seed from registry
    let system_personas = virtues_registry::personas::default_personas();
    data.items = system_personas
        .into_iter()
        .map(|p| Persona {
            id: p.id,
            title: p.title,
            content: p.content,
            is_system: true,
        })
        .collect();

    save_personas_data(db, data).await?;
    Ok(true)
}

// ============================================================================
// Public API functions
// ============================================================================

/// List all personas (excluding hidden ones)
///
/// Seeds system personas from registry on first access.
pub async fn list_personas(db: &SqlitePool) -> Result<Vec<Persona>> {
    let profile = get_profile(db).await?;
    let mut data = parse_personas_data(&profile);

    // Seed on first access
    seed_if_empty(db, &mut data).await?;

    // Filter out hidden personas
    let visible: Vec<Persona> = data
        .items
        .into_iter()
        .filter(|p| !data.hidden.contains(&p.id))
        .collect();

    Ok(visible)
}

/// List all personas including hidden ones (for settings UI)
pub async fn list_all_personas(db: &SqlitePool) -> Result<(Vec<Persona>, Vec<String>)> {
    let profile = get_profile(db).await?;
    let mut data = parse_personas_data(&profile);

    // Seed on first access
    seed_if_empty(db, &mut data).await?;

    Ok((data.items, data.hidden))
}

/// Get a specific persona by ID
pub async fn get_persona(db: &SqlitePool, id: &str) -> Result<Option<Persona>> {
    let profile = get_profile(db).await?;
    let data = parse_personas_data(&profile);

    Ok(data.items.into_iter().find(|p| p.id == id))
}

/// Get persona content by ID (for system prompt building)
///
/// Returns the raw content string, or None if not found.
pub async fn get_persona_content(db: &SqlitePool, id: &str) -> Result<Option<String>> {
    let persona = get_persona(db, id).await?;
    Ok(persona.map(|p| p.content))
}

/// Create a new custom persona
pub async fn create_persona(db: &SqlitePool, req: CreatePersonaRequest) -> Result<Persona> {
    let profile = get_profile(db).await?;
    let mut data = parse_personas_data(&profile);

    // Seed if empty first
    seed_if_empty(db, &mut data).await?;

    let persona = Persona {
        // Generate ID with proper prefix (persona_{hash16})
        id: ids::generate_id("persona", &[&req.title, &chrono::Utc::now().to_rfc3339()]),
        title: req.title,
        content: req.content,
        is_system: false,
    };

    data.items.push(persona.clone());
    save_personas_data(db, &data).await?;

    Ok(persona)
}

/// Update an existing persona
pub async fn update_persona(db: &SqlitePool, id: &str, req: UpdatePersonaRequest) -> Result<Persona> {
    let profile = get_profile(db).await?;
    let mut data = parse_personas_data(&profile);

    // Find and update the persona
    let persona = data
        .items
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or_else(|| Error::NotFound(format!("Persona '{}' not found", id)))?;

    if let Some(title) = req.title {
        persona.title = title;
    }
    if let Some(content) = req.content {
        persona.content = content;
    }

    let updated = persona.clone();
    save_personas_data(db, &data).await?;

    Ok(updated)
}

/// Hide a persona (soft delete for system personas, hard delete for custom)
pub async fn hide_persona(db: &SqlitePool, id: &str) -> Result<()> {
    let profile = get_profile(db).await?;
    let mut data = parse_personas_data(&profile);

    // Find the persona
    let persona = data
        .items
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| Error::NotFound(format!("Persona '{}' not found", id)))?;

    if persona.is_system {
        // System persona: add to hidden list
        if !data.hidden.contains(&id.to_string()) {
            data.hidden.push(id.to_string());
        }
    } else {
        // Custom persona: actually remove it
        data.items.retain(|p| p.id != id);
    }

    save_personas_data(db, &data).await?;
    Ok(())
}

/// Unhide a previously hidden system persona
pub async fn unhide_persona(db: &SqlitePool, id: &str) -> Result<()> {
    let profile = get_profile(db).await?;
    let mut data = parse_personas_data(&profile);

    data.hidden.retain(|h| h != id);
    save_personas_data(db, &data).await?;

    Ok(())
}

/// Reset personas to default (re-seed from registry)
pub async fn reset_personas(db: &SqlitePool) -> Result<Vec<Persona>> {
    // Clear existing data
    let data = PersonasData::default();
    save_personas_data(db, &data).await?;

    // Trigger re-seed
    list_personas(db).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_personas_data_serialization() {
        let data = PersonasData {
            items: vec![Persona {
                id: "test".to_string(),
                title: "Test".to_string(),
                content: "Test content".to_string(),
                is_system: false,
            }],
            hidden: vec!["hidden_id".to_string()],
        };

        let json = serde_json::to_string(&data).unwrap();
        let parsed: PersonasData = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.items.len(), 1);
        assert_eq!(parsed.items[0].id, "test");
        assert_eq!(parsed.hidden.len(), 1);
        assert_eq!(parsed.hidden[0], "hidden_id");
    }

    #[test]
    fn test_empty_personas_data() {
        let data = PersonasData::default();
        assert!(data.items.is_empty());
        assert!(data.hidden.is_empty());
    }
}

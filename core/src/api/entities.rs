//! Entities API - Managing resolved entities (places, people, topics)
//!
//! This module provides CRUD operations for entity types:
//! - Places: Known locations (home, work, etc.)
//! - People: Contacts and relationships (future)
//! - Topics: Subjects and interests (future)

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::error::{Error, Result};

// ============================================================================
// Place Types
// ============================================================================

/// A place entity from the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Place {
    pub id: Uuid,
    pub name: String,
    pub category: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub radius_m: Option<f64>,
    pub visit_count: Option<i32>,
    pub metadata: Option<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Request to create a new place
#[derive(Debug, Deserialize)]
pub struct CreatePlaceRequest {
    /// Display name/label for the place (e.g., "Home", "Work", "Gym")
    pub label: String,
    /// Full formatted address
    pub formatted_address: String,
    /// Latitude coordinate
    pub latitude: f64,
    /// Longitude coordinate
    pub longitude: f64,
    /// Google Place ID (optional, for linking to Google Places)
    pub google_place_id: Option<String>,
    /// Category (e.g., "home", "work", "gym")
    pub category: Option<String>,
    /// Whether to set this place as home (updates user_profile.home_place_id)
    pub set_as_home: Option<bool>,
}

/// Request to update an existing place
#[derive(Debug, Deserialize)]
pub struct UpdatePlaceRequest {
    pub label: Option<String>,
    pub formatted_address: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub google_place_id: Option<String>,
    pub category: Option<String>,
}

/// Response for created place
#[derive(Debug, Serialize)]
pub struct CreatePlaceResponse {
    pub id: Uuid,
    pub name: String,
    pub is_home: bool,
}

// ============================================================================
// Place CRUD Operations
// ============================================================================

/// List all known places (places with is_known_location: true in metadata)
pub async fn list_places(pool: &SqlitePool) -> Result<Vec<Place>> {
    let rows = sqlx::query!(
        r#"
        SELECT
            id,
            name,
            category,
            latitude,
            longitude,
            radius_m,
            visit_count,
            metadata,
            created_at,
            updated_at
        FROM data_entities_place
        WHERE json_extract(metadata, '$.is_known_location') = 'true'
        ORDER BY created_at ASC
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list places: {}", e)))?;

    let places = rows
        .into_iter()
        .filter_map(|row| {
            // SQLite returns id as Option<String>, but name/created_at/updated_at are NOT NULL (String)
            let id_str = row.id.as_ref()?;
            Some(Place {
                id: Uuid::parse_str(id_str).ok()?,
                name: row.name.clone(),
                category: row.category.clone(),
                latitude: row.latitude,
                longitude: row.longitude,
                radius_m: row.radius_m,
                visit_count: row.visit_count.map(|v| v as i32),
                metadata: row
                    .metadata
                    .as_ref()
                    .and_then(|m| serde_json::from_str(m).ok()),
                created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
                    .ok()?
                    .with_timezone(&chrono::Utc),
                updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
                    .ok()?
                    .with_timezone(&chrono::Utc),
            })
        })
        .collect();

    Ok(places)
}

/// Get a single place by ID
pub async fn get_place(pool: &SqlitePool, id: Uuid) -> Result<Place> {
    let id_str = id.to_string();
    let row = sqlx::query!(
        r#"
        SELECT
            id,
            name,
            category,
            latitude,
            longitude,
            radius_m,
            visit_count,
            metadata,
            created_at,
            updated_at
        FROM data_entities_place
        WHERE id = $1
        "#,
        id_str
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get place: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Place not found: {}", id)))?;

    // SQLite returns id as Option<String>, but name/created_at/updated_at are NOT NULL (String)
    let id_str = row
        .id
        .as_ref()
        .ok_or_else(|| Error::Database("Place ID is null".to_string()))?;

    Ok(Place {
        id: Uuid::parse_str(id_str).map_err(|e| Error::Database(format!("Invalid UUID: {}", e)))?,
        name: row.name.clone(),
        category: row.category.clone(),
        latitude: row.latitude,
        longitude: row.longitude,
        radius_m: row.radius_m,
        visit_count: row.visit_count.map(|v| v as i32),
        metadata: row
            .metadata
            .as_ref()
            .and_then(|m| serde_json::from_str(m).ok()),
        created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
            .map_err(|e| Error::Database(format!("Invalid timestamp: {}", e)))?
            .with_timezone(&chrono::Utc),
        updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
            .map_err(|e| Error::Database(format!("Invalid timestamp: {}", e)))?
            .with_timezone(&chrono::Utc),
    })
}

/// Create a new place
pub async fn create_place(
    pool: &SqlitePool,
    req: CreatePlaceRequest,
) -> Result<CreatePlaceResponse> {
    let metadata = serde_json::json!({
        "formatted_address": req.formatted_address,
        "google_place_id": req.google_place_id,
        "is_known_location": true,
        "source": "user"
    });
    let metadata_str = serde_json::to_string(&metadata)
        .map_err(|e| Error::Database(format!("Failed to serialize metadata: {}", e)))?;

    let id = Uuid::new_v4();
    let id_str = id.to_string();

    sqlx::query!(
        r#"
        INSERT INTO data_entities_place (
            id,
            name,
            category,
            address,
            latitude,
            longitude,
            radius_m,
            metadata
        ) VALUES (
            $1, $2, $3, $4, $5, $6, 50.0, $7
        )
        "#,
        id_str,
        req.label,
        req.category,
        req.formatted_address,
        req.latitude,
        req.longitude,
        metadata_str
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create place: {}", e)))?;

    // Set as home if requested
    let is_home = req.set_as_home.unwrap_or(false);
    if is_home {
        set_home_place(pool, id).await?;
    }

    Ok(CreatePlaceResponse {
        id,
        name: req.label,
        is_home,
    })
}

/// Update an existing place
pub async fn update_place(pool: &SqlitePool, id: Uuid, req: UpdatePlaceRequest) -> Result<Place> {
    // First get the existing place to preserve metadata
    let existing = get_place(pool, id).await?;
    let mut metadata = existing.metadata.unwrap_or_else(|| serde_json::json!({}));

    // Update metadata fields if provided
    if let Some(ref addr) = req.formatted_address {
        metadata["formatted_address"] = serde_json::json!(addr);
    }
    if let Some(ref gid) = req.google_place_id {
        metadata["google_place_id"] = serde_json::json!(gid);
    }
    let metadata_str = serde_json::to_string(&metadata)
        .map_err(|e| Error::Database(format!("Failed to serialize metadata: {}", e)))?;

    let id_str = id.to_string();

    // SQLite doesn't support RETURNING with complex updates, so we do update then select
    sqlx::query!(
        r#"
        UPDATE data_entities_place
        SET
            name = COALESCE($2, name),
            category = COALESCE($3, category),
            address = COALESCE($7, address),
            latitude = COALESCE($4, latitude),
            longitude = COALESCE($5, longitude),
            metadata = $6,
            updated_at = datetime('now')
        WHERE id = $1
        "#,
        id_str,
        req.label,
        req.category,
        req.latitude,
        req.longitude,
        metadata_str,
        req.formatted_address
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update place: {}", e)))?;

    // Fetch the updated place
    get_place(pool, id).await
}

/// Delete a place by ID
pub async fn delete_place(pool: &SqlitePool, id: Uuid) -> Result<()> {
    // First, unset home_place_id if this place is currently set as home
    let profile_id_str = "00000000-0000-0000-0000-000000000001";
    let id_str = id.to_string();

    sqlx::query!(
        r#"
        UPDATE data_user_profile
        SET home_place_id = NULL
        WHERE id = $1 AND home_place_id = $2
        "#,
        profile_id_str,
        id_str
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to unset home place: {}", e)))?;

    // Delete the place
    let result = sqlx::query!(
        r#"
        DELETE FROM data_entities_place
        WHERE id = $1
        "#,
        id_str
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to delete place: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("Place not found: {}", id)));
    }

    Ok(())
}

/// Set a place as the user's home (updates user_profile.home_place_id)
pub async fn set_home_place(pool: &SqlitePool, place_id: Uuid) -> Result<()> {
    let profile_id_str = "00000000-0000-0000-0000-000000000001";
    let place_id_str = place_id.to_string();

    // Verify the place exists
    let exists = sqlx::query!(
        r#"SELECT id FROM data_entities_place WHERE id = $1"#,
        place_id_str
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to verify place: {}", e)))?;

    if exists.is_none() {
        return Err(Error::NotFound(format!("Place not found: {}", place_id)));
    }

    // Update user profile
    sqlx::query!(
        r#"
        UPDATE data_user_profile
        SET home_place_id = $1
        WHERE id = $2
        "#,
        place_id_str,
        profile_id_str
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to set home place: {}", e)))?;

    Ok(())
}

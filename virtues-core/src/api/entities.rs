//! Entities API - Managing resolved entities (places, people, topics)
//!
//! This module provides CRUD operations for entity types:
//! - Places: Known locations (home, work, etc.)
//! - People: Contacts and relationships (future)
//! - Topics: Subjects and interests (future)

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::error::{Error, Result};
use crate::ids;

// ============================================================================
// Place Types
// ============================================================================

/// A place entity from the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Place {
    pub id: String,
    pub name: String,
    pub category: Option<String>,
    pub address: Option<String>,
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
    pub id: String,
    pub name: String,
    pub is_home: bool,
}

// ============================================================================
// Place CRUD Operations
// ============================================================================

/// List all known places (places with is_known_location: true in metadata)
pub async fn list_places(pool: &PgPool) -> Result<Vec<Place>> {
    let rows = sqlx::query!(
        r#"
        SELECT
            id,
            name,
            category,
            address,
            latitude,
            longitude,
            radius_m,
            visit_count,
            metadata,
            created_at,
            updated_at
        FROM wiki_places
        WHERE (metadata->>'is_known_location')::boolean = true
        ORDER BY created_at ASC
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list places: {}", e)))?;

    let places = rows
        .into_iter()
        .map(|row| Place {
            id: row.id,
            name: row.name,
            category: row.category,
            address: row.address,
            latitude: row.latitude,
            longitude: row.longitude,
            radius_m: Some(row.radius_m),
            visit_count: Some(row.visit_count as i32),
            metadata: Some(row.metadata),
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
        .collect();

    Ok(places)
}

/// Get a single place by ID
pub async fn get_place(pool: &PgPool, id: String) -> Result<Place> {
    let id_str = &id;
    let row = sqlx::query!(
        r#"
        SELECT
            id,
            name,
            category,
            address,
            latitude,
            longitude,
            radius_m,
            visit_count,
            metadata,
            created_at,
            updated_at
        FROM wiki_places
        WHERE id = $1
        "#,
        id_str
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get place: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Place not found: {}", id)))?;

    Ok(Place {
        id: row.id,
        name: row.name,
        category: row.category,
        address: row.address,
        latitude: row.latitude,
        longitude: row.longitude,
        radius_m: Some(row.radius_m),
        visit_count: Some(row.visit_count as i32),
        metadata: Some(row.metadata),
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// Create a new place
pub async fn create_place(
    pool: &PgPool,
    req: CreatePlaceRequest,
) -> Result<CreatePlaceResponse> {
    let metadata = serde_json::json!({
        "google_place_id": req.google_place_id,
        "is_known_location": true,
        "source": "user"
    });

    // Generate ID with proper prefix (place_{hash16})
    let id = ids::generate_id(
        ids::WIKI_PLACE_PREFIX,
        &[&req.label, &req.latitude.to_string(), &req.longitude.to_string()],
    );
    let id_str = id.clone();

    sqlx::query!(
        r#"
        INSERT INTO wiki_places (
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
        metadata,
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create place: {}", e)))?;

    // Set as home if requested
    let is_home = req.set_as_home.unwrap_or(false);
    if is_home {
        set_home_place(pool, id.clone()).await?;
    }

    Ok(CreatePlaceResponse {
        id,
        name: req.label,
        is_home,
    })
}

/// Update an existing place
pub async fn update_place(pool: &PgPool, id: String, req: UpdatePlaceRequest) -> Result<Place> {
    // First get the existing place to preserve metadata
    let existing = get_place(pool, id.clone()).await?;
    let mut metadata = existing.metadata.unwrap_or_else(|| serde_json::json!({}));
 
    // Update metadata fields if provided (only google_place_id goes in metadata now)
    if let Some(ref gid) = req.google_place_id {
        metadata["google_place_id"] = serde_json::json!(gid);
    }

    let id_str = &id;

    sqlx::query!(
        r#"
        UPDATE wiki_places
        SET
            name = COALESCE($2, name),
            category = COALESCE($3, category),
            address = COALESCE($4, address),
            latitude = COALESCE($5, latitude),
            longitude = COALESCE($6, longitude),
            metadata = $7,
            updated_at = now()
        WHERE id = $1
        "#,
        id_str,
        req.label,
        req.category,
        req.formatted_address,
        req.latitude,
        req.longitude,
        metadata,
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update place: {}", e)))?;

    // Fetch the updated place
    get_place(pool, id).await
}

/// Delete a place by ID
pub async fn delete_place(pool: &PgPool, id: String) -> Result<()> {
    // First, unset home_place_id if this place is currently set as home
    let profile_id_str = "00000000-0000-0000-0000-000000000001";
    let id_str = &id;

    sqlx::query!(
        r#"
        UPDATE app_user_profile
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
        DELETE FROM wiki_places
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
pub async fn set_home_place(pool: &PgPool, place_id: String) -> Result<()> {
    let profile_id_str = "00000000-0000-0000-0000-000000000001";
    let place_id_str = &place_id;

    // Verify the place exists
    let exists = sqlx::query!(
        r#"SELECT id FROM wiki_places WHERE id = $1"#,
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
        UPDATE app_user_profile
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

//! Entities API - Managing resolved entities (places, people, topics)
//!
//! This module provides CRUD operations for entity types:
//! - Places: Known locations (home, work, etc.)
//! - People: Contacts and relationships (future)
//! - Topics: Subjects and interests (future)

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{Error, Result};

// ============================================================================
// Place Types
// ============================================================================

/// A place entity from the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Place {
    pub id: Uuid,
    pub canonical_name: String,
    pub category: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub cluster_radius_meters: Option<f64>,
    pub visit_count: Option<i32>,
    pub total_time_minutes: Option<i32>,
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
    pub canonical_name: String,
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
            canonical_name,
            category,
            ST_Y(geo_center::geometry) as latitude,
            ST_X(geo_center::geometry) as longitude,
            cluster_radius_meters,
            visit_count,
            total_time_minutes,
            metadata,
            created_at,
            updated_at
        FROM data.entities_place
        WHERE metadata->>'is_known_location' = 'true'
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
            canonical_name: row.canonical_name,
            category: row.category,
            latitude: row.latitude,
            longitude: row.longitude,
            cluster_radius_meters: row.cluster_radius_meters,
            visit_count: row.visit_count,
            total_time_minutes: row.total_time_minutes,
            metadata: row.metadata,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
        .collect();

    Ok(places)
}

/// Get a single place by ID
pub async fn get_place(pool: &PgPool, id: Uuid) -> Result<Place> {
    let row = sqlx::query!(
        r#"
        SELECT
            id,
            canonical_name,
            category,
            ST_Y(geo_center::geometry) as latitude,
            ST_X(geo_center::geometry) as longitude,
            cluster_radius_meters,
            visit_count,
            total_time_minutes,
            metadata,
            created_at,
            updated_at
        FROM data.entities_place
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get place: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Place not found: {}", id)))?;

    Ok(Place {
        id: row.id,
        canonical_name: row.canonical_name,
        category: row.category,
        latitude: row.latitude,
        longitude: row.longitude,
        cluster_radius_meters: row.cluster_radius_meters,
        visit_count: row.visit_count,
        total_time_minutes: row.total_time_minutes,
        metadata: row.metadata,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// Create a new place
pub async fn create_place(pool: &PgPool, req: CreatePlaceRequest) -> Result<CreatePlaceResponse> {
    let metadata = serde_json::json!({
        "formatted_address": req.formatted_address,
        "google_place_id": req.google_place_id,
        "is_known_location": true,
        "source": "user"
    });

    let row = sqlx::query!(
        r#"
        INSERT INTO data.entities_place (
            canonical_name,
            category,
            geo_center,
            cluster_radius_meters,
            metadata
        ) VALUES (
            $1,
            $2,
            ST_SetSRID(ST_MakePoint($3, $4), 4326)::geography,
            50.0,
            $5
        )
        RETURNING id, canonical_name
        "#,
        req.label,
        req.category,
        req.longitude,
        req.latitude,
        metadata
    )
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create place: {}", e)))?;

    // Set as home if requested
    let is_home = req.set_as_home.unwrap_or(false);
    if is_home {
        set_home_place(pool, row.id).await?;
    }

    Ok(CreatePlaceResponse {
        id: row.id,
        canonical_name: row.canonical_name,
        is_home,
    })
}

/// Update an existing place
pub async fn update_place(pool: &PgPool, id: Uuid, req: UpdatePlaceRequest) -> Result<Place> {
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

    let row = sqlx::query!(
        r#"
        UPDATE data.entities_place
        SET
            canonical_name = COALESCE($2, canonical_name),
            category = COALESCE($3, category),
            geo_center = CASE
                WHEN $4::float8 IS NOT NULL AND $5::float8 IS NOT NULL
                THEN ST_SetSRID(ST_MakePoint($4, $5), 4326)::geography
                ELSE geo_center
            END,
            metadata = $6,
            updated_at = NOW()
        WHERE id = $1
        RETURNING
            id,
            canonical_name,
            category,
            ST_Y(geo_center::geometry) as latitude,
            ST_X(geo_center::geometry) as longitude,
            cluster_radius_meters,
            visit_count,
            total_time_minutes,
            metadata,
            created_at,
            updated_at
        "#,
        id,
        req.label,
        req.category,
        req.longitude,
        req.latitude,
        metadata
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update place: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Place not found: {}", id)))?;

    Ok(Place {
        id: row.id,
        canonical_name: row.canonical_name,
        category: row.category,
        latitude: row.latitude,
        longitude: row.longitude,
        cluster_radius_meters: row.cluster_radius_meters,
        visit_count: row.visit_count,
        total_time_minutes: row.total_time_minutes,
        metadata: row.metadata,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// Delete a place by ID
pub async fn delete_place(pool: &PgPool, id: Uuid) -> Result<()> {
    // First, unset home_place_id if this place is currently set as home
    let profile_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001")
        .expect("Valid UUID constant");

    sqlx::query!(
        r#"
        UPDATE data.user_profile
        SET home_place_id = NULL
        WHERE id = $1 AND home_place_id = $2
        "#,
        profile_id,
        id
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to unset home place: {}", e)))?;

    // Delete the place
    let result = sqlx::query!(
        r#"
        DELETE FROM data.entities_place
        WHERE id = $1
        "#,
        id
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
pub async fn set_home_place(pool: &PgPool, place_id: Uuid) -> Result<()> {
    let profile_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001")
        .expect("Valid UUID constant");

    // Verify the place exists
    let exists = sqlx::query!(
        r#"SELECT id FROM data.entities_place WHERE id = $1"#,
        place_id
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
        UPDATE data.user_profile
        SET home_place_id = $1
        WHERE id = $2
        "#,
        place_id,
        profile_id
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to set home place: {}", e)))?;

    Ok(())
}

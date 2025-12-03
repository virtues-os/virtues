//! User profile API
//!
//! This module provides functions for managing the user's biographical profile.
//! The profile is a singleton table containing non-ephemeral metadata about the user.

use crate::error::{Error, Result};
use crate::storage::models::UserProfile;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::{types::Decimal, PgPool};
use std::str::FromStr;
use uuid::Uuid;

/// Request to update user profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProfileRequest {
    // Identity
    pub full_name: Option<String>,
    pub preferred_name: Option<String>,
    pub birth_date: Option<NaiveDate>,
    // Physical/Biometric (use f64 for JSON, convert to Decimal for DB)
    pub height_cm: Option<f64>,
    pub weight_kg: Option<f64>,
    pub ethnicity: Option<String>,
    // Home Address
    pub home_street: Option<String>,
    pub home_city: Option<String>,
    pub home_state: Option<String>,
    pub home_postal_code: Option<String>,
    pub home_country: Option<String>,
    // Work/Occupation
    pub occupation: Option<String>,
    pub employer: Option<String>,
    // Preferences
    pub theme: Option<String>,
    // Crux - shared ethos statement from onboarding
    pub crux: Option<String>,
    // Onboarding (legacy)
    pub is_onboarding: Option<bool>,
    pub onboarding_step: Option<i32>,
    pub axiology_complete: Option<bool>,
    // Granular onboarding completion
    pub onboarding_profile_complete: Option<bool>,
    pub onboarding_places_complete: Option<bool>,
    pub onboarding_tools_complete: Option<bool>,
}

/// Get the user's profile (singleton row)
///
/// This will always return a profile, as the migration creates an empty row by default.
pub async fn get_profile(db: &PgPool) -> Result<UserProfile> {
    let profile = sqlx::query_as::<_, UserProfile>(
        r#"
        SELECT *
        FROM data.user_profile
        LIMIT 1
        "#,
    )
    .fetch_one(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch user profile: {}", e)))?;

    Ok(profile)
}

/// Update the user's profile
///
/// Only updates fields that are present in the request (not None).
/// Returns the updated profile.
pub async fn update_profile(db: &PgPool, request: UpdateProfileRequest) -> Result<UserProfile> {
    // The singleton UUID
    let profile_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001")
        .expect("Valid UUID constant");

    // Build dynamic UPDATE query based on which fields are present
    let mut updates = Vec::new();
    let mut query = "UPDATE data.user_profile SET ".to_string();

    // Identity fields
    if request.full_name.is_some() {
        updates.push("full_name = $1");
    }
    if request.preferred_name.is_some() {
        updates.push("preferred_name = $2");
    }
    if request.birth_date.is_some() {
        updates.push("birth_date = $3");
    }
    // Physical fields
    if request.height_cm.is_some() {
        updates.push("height_cm = $4");
    }
    if request.weight_kg.is_some() {
        updates.push("weight_kg = $5");
    }
    if request.ethnicity.is_some() {
        updates.push("ethnicity = $6");
    }
    // Address fields
    if request.home_street.is_some() {
        updates.push("home_street = $7");
    }
    if request.home_city.is_some() {
        updates.push("home_city = $8");
    }
    if request.home_state.is_some() {
        updates.push("home_state = $9");
    }
    if request.home_postal_code.is_some() {
        updates.push("home_postal_code = $10");
    }
    if request.home_country.is_some() {
        updates.push("home_country = $11");
    }
    // Work fields
    if request.occupation.is_some() {
        updates.push("occupation = $12");
    }
    if request.employer.is_some() {
        updates.push("employer = $13");
    }
    // Preferences
    if request.theme.is_some() {
        updates.push("theme = $14");
    }
    // Crux
    if request.crux.is_some() {
        updates.push("crux = $15");
    }
    // Onboarding
    if request.is_onboarding.is_some() {
        updates.push("is_onboarding = $16");
    }
    if request.onboarding_step.is_some() {
        updates.push("onboarding_step = $17");
    }
    if request.axiology_complete.is_some() {
        updates.push("axiology_complete = $18");
    }
    // Granular onboarding completion
    if request.onboarding_profile_complete.is_some() {
        updates.push("onboarding_profile_complete = $19");
    }
    if request.onboarding_places_complete.is_some() {
        updates.push("onboarding_places_complete = $20");
    }
    if request.onboarding_tools_complete.is_some() {
        updates.push("onboarding_tools_complete = $21");
    }

    if updates.is_empty() {
        // No updates requested, just return current profile
        return get_profile(db).await;
    }

    query.push_str(&updates.join(", "));
    query.push_str(" WHERE id = $22 RETURNING *");

    // Execute the update with bound parameters
    let mut query_builder = sqlx::query_as::<_, UserProfile>(&query);

    // Convert f64 to Decimal for database
    let height_decimal = request.height_cm.map(|v| {
        Decimal::from_str(&v.to_string())
            .unwrap_or_else(|_| Decimal::from_str("0").unwrap())
    });
    let weight_decimal = request.weight_kg.map(|v| {
        Decimal::from_str(&v.to_string())
            .unwrap_or_else(|_| Decimal::from_str("0").unwrap())
    });

    // Bind all parameters in order ($1 through $22)
    query_builder = query_builder
        .bind(&request.full_name)
        .bind(&request.preferred_name)
        .bind(&request.birth_date)
        .bind(&height_decimal)
        .bind(&weight_decimal)
        .bind(&request.ethnicity)
        .bind(&request.home_street)
        .bind(&request.home_city)
        .bind(&request.home_state)
        .bind(&request.home_postal_code)
        .bind(&request.home_country)
        .bind(&request.occupation)
        .bind(&request.employer)
        .bind(&request.theme)
        .bind(&request.crux)
        .bind(&request.is_onboarding)
        .bind(&request.onboarding_step)
        .bind(&request.axiology_complete)
        .bind(&request.onboarding_profile_complete)
        .bind(&request.onboarding_places_complete)
        .bind(&request.onboarding_tools_complete)
        .bind(profile_id);

    let updated_profile = query_builder
        .fetch_one(db)
        .await
        .map_err(|e| Error::Database(format!("Failed to update user profile: {}", e)))?;

    Ok(updated_profile)
}

/// Helper to get the user's display name for system prompts
///
/// Returns preferred_name if set, otherwise full_name, otherwise "the user"
pub async fn get_display_name(db: &PgPool) -> Result<String> {
    let profile = get_profile(db).await?;

    Ok(profile
        .preferred_name
        .or(profile.full_name)
        .unwrap_or_else(|| "the user".to_string()))
}

/// Request to set user's home place via Google Places Autocomplete
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetHomePlaceRequest {
    pub formatted_address: String,
    pub latitude: f64,
    pub longitude: f64,
    pub google_place_id: Option<String>,
}

/// Response after setting home place
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetHomePlaceResponse {
    pub place_id: Uuid,
    pub canonical_name: String,
}

/// Set the user's home place
///
/// Creates an entities_place record with category='home' and links it to the user profile.
/// If a home place already exists, it will be replaced.
pub async fn set_home_place(db: &PgPool, request: SetHomePlaceRequest) -> Result<SetHomePlaceResponse> {
    // The singleton profile UUID
    let profile_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001")
        .expect("Valid UUID constant");

    // Build metadata JSON with google_place_id if provided
    let metadata = match &request.google_place_id {
        Some(gid) => serde_json::json!({ "google_place_id": gid }),
        None => serde_json::json!({}),
    };

    // Create or update the home place entity
    // Use ON CONFLICT to update if a place with category='home' already exists nearby
    let place = sqlx::query!(
        r#"
        INSERT INTO data.entities_place (
            canonical_name,
            category,
            geo_center,
            cluster_radius_meters,
            metadata
        ) VALUES (
            $1,
            'home',
            ST_SetSRID(ST_MakePoint($2, $3), 4326)::geography,
            50.0,
            $4
        )
        RETURNING id, canonical_name
        "#,
        request.formatted_address,
        request.longitude,
        request.latitude,
        metadata
    )
    .fetch_one(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to create home place: {}", e)))?;

    // Update user profile with the new home_place_id
    sqlx::query!(
        r#"
        UPDATE data.user_profile
        SET home_place_id = $1
        WHERE id = $2
        "#,
        place.id,
        profile_id
    )
    .execute(db)
    .await
    .map_err(|e| Error::Database(format!("Failed to update profile with home place: {}", e)))?;

    Ok(SetHomePlaceResponse {
        place_id: place.id,
        canonical_name: place.canonical_name,
    })
}

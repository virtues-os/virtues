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
    // Work fields
    if request.occupation.is_some() {
        updates.push("occupation = $7");
    }
    if request.employer.is_some() {
        updates.push("employer = $8");
    }
    // Preferences
    if request.theme.is_some() {
        updates.push("theme = $9");
    }
    // Crux
    if request.crux.is_some() {
        updates.push("crux = $10");
    }
    // Onboarding
    if request.is_onboarding.is_some() {
        updates.push("is_onboarding = $11");
    }
    if request.onboarding_step.is_some() {
        updates.push("onboarding_step = $12");
    }
    if request.axiology_complete.is_some() {
        updates.push("axiology_complete = $13");
    }
    // Granular onboarding completion
    if request.onboarding_profile_complete.is_some() {
        updates.push("onboarding_profile_complete = $14");
    }
    if request.onboarding_places_complete.is_some() {
        updates.push("onboarding_places_complete = $15");
    }
    if request.onboarding_tools_complete.is_some() {
        updates.push("onboarding_tools_complete = $16");
    }

    if updates.is_empty() {
        // No updates requested, just return current profile
        return get_profile(db).await;
    }

    query.push_str(&updates.join(", "));
    query.push_str(" WHERE id = $17 RETURNING *");

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

    // Bind all parameters in order ($1 through $17)
    query_builder = query_builder
        .bind(&request.full_name)
        .bind(&request.preferred_name)
        .bind(&request.birth_date)
        .bind(&height_decimal)
        .bind(&weight_decimal)
        .bind(&request.ethnicity)
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

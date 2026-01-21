//! User profile API
//!
//! This module provides functions for managing the user's biographical profile.
//! The profile is a singleton table containing non-ephemeral metadata about the user.

use crate::error::{Error, Result};
use crate::storage::models::UserProfile;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// Request to update user profile
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateProfileRequest {
    // Identity
    pub full_name: Option<String>,
    pub preferred_name: Option<String>,
    pub birth_date: Option<String>,
    // Physical/Biometric
    pub height_cm: Option<f64>,
    pub weight_kg: Option<f64>,
    pub ethnicity: Option<String>,
    // Work/Occupation
    pub occupation: Option<String>,
    pub employer: Option<String>,
    // Home
    pub home_place_id: Option<String>,
    // Onboarding - single status field
    pub onboarding_status: Option<String>,
    // Preferences
    pub theme: Option<String>,
    pub update_check_hour: Option<i32>,
    // Discovery context
    pub crux: Option<String>,
    pub technology_vision: Option<String>,
    pub pain_point_primary: Option<String>,
    pub pain_point_secondary: Option<String>,
    pub excited_features: Option<String>,
}

/// Get the user's profile (singleton row)
///
/// This will always return a profile, as the migration creates an empty row by default.
pub async fn get_profile(db: &SqlitePool) -> Result<UserProfile> {
    let profile = sqlx::query_as::<_, UserProfile>(
        r#"
        SELECT *
        FROM data_user_profile
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
pub async fn update_profile(db: &SqlitePool, request: UpdateProfileRequest) -> Result<UserProfile> {
    // Build dynamic UPDATE using a simpler approach
    let mut set_clauses = Vec::new();

    if request.full_name.is_some() {
        set_clauses.push("full_name = ?");
    }
    if request.preferred_name.is_some() {
        set_clauses.push("preferred_name = ?");
    }
    if request.birth_date.is_some() {
        set_clauses.push("birth_date = ?");
    }
    if request.height_cm.is_some() {
        set_clauses.push("height_cm = ?");
    }
    if request.weight_kg.is_some() {
        set_clauses.push("weight_kg = ?");
    }
    if request.ethnicity.is_some() {
        set_clauses.push("ethnicity = ?");
    }
    if request.occupation.is_some() {
        set_clauses.push("occupation = ?");
    }
    if request.employer.is_some() {
        set_clauses.push("employer = ?");
    }
    if request.home_place_id.is_some() {
        set_clauses.push("home_place_id = ?");
    }
    if request.onboarding_status.is_some() {
        set_clauses.push("onboarding_status = ?");
    }
    if request.theme.is_some() {
        set_clauses.push("theme = ?");
    }
    if request.update_check_hour.is_some() {
        set_clauses.push("update_check_hour = ?");
    }
    if request.crux.is_some() {
        set_clauses.push("crux = ?");
    }
    if request.technology_vision.is_some() {
        set_clauses.push("technology_vision = ?");
    }
    if request.pain_point_primary.is_some() {
        set_clauses.push("pain_point_primary = ?");
    }
    if request.pain_point_secondary.is_some() {
        set_clauses.push("pain_point_secondary = ?");
    }
    if request.excited_features.is_some() {
        set_clauses.push("excited_features = ?");
    }

    if set_clauses.is_empty() {
        // No updates requested, just return current profile
        return get_profile(db).await;
    }

    // Always update updated_at
    set_clauses.push("updated_at = datetime('now')");

    let query = format!(
        "UPDATE data_user_profile SET {} WHERE id = '00000000-0000-0000-0000-000000000001'",
        set_clauses.join(", ")
    );

    // Build the query with bindings
    let mut query_builder = sqlx::query(&query);

    // Bind in the same order as set_clauses
    if let Some(ref v) = request.full_name {
        query_builder = query_builder.bind(v);
    }
    if let Some(ref v) = request.preferred_name {
        query_builder = query_builder.bind(v);
    }
    if let Some(ref v) = request.birth_date {
        query_builder = query_builder.bind(v);
    }
    if let Some(v) = request.height_cm {
        query_builder = query_builder.bind(v);
    }
    if let Some(v) = request.weight_kg {
        query_builder = query_builder.bind(v);
    }
    if let Some(ref v) = request.ethnicity {
        query_builder = query_builder.bind(v);
    }
    if let Some(ref v) = request.occupation {
        query_builder = query_builder.bind(v);
    }
    if let Some(ref v) = request.employer {
        query_builder = query_builder.bind(v);
    }
    if let Some(ref v) = request.home_place_id {
        query_builder = query_builder.bind(v);
    }
    if let Some(ref v) = request.onboarding_status {
        query_builder = query_builder.bind(v);
    }
    if let Some(ref v) = request.theme {
        query_builder = query_builder.bind(v);
    }
    if let Some(v) = request.update_check_hour {
        query_builder = query_builder.bind(v);
    }
    if let Some(ref v) = request.crux {
        query_builder = query_builder.bind(v);
    }
    if let Some(ref v) = request.technology_vision {
        query_builder = query_builder.bind(v);
    }
    if let Some(ref v) = request.pain_point_primary {
        query_builder = query_builder.bind(v);
    }
    if let Some(ref v) = request.pain_point_secondary {
        query_builder = query_builder.bind(v);
    }
    if let Some(ref v) = request.excited_features {
        query_builder = query_builder.bind(v);
    }

    query_builder
        .execute(db)
        .await
        .map_err(|e| Error::Database(format!("Failed to update user profile: {}", e)))?;

    // Return updated profile
    get_profile(db).await
}

/// Helper to get the user's display name for system prompts
///
/// Returns preferred_name if set, otherwise full_name, otherwise "the user"
pub async fn get_display_name(db: &SqlitePool) -> Result<String> {
    let profile = get_profile(db).await?;

    Ok(profile
        .preferred_name
        .or(profile.full_name)
        .unwrap_or_else(|| "the user".to_string()))
}

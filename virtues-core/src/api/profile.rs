//! User profile API
//!
//! This module provides functions for managing the user's biographical profile.
//! The profile is a singleton table containing non-ephemeral metadata about the user.

use crate::error::{Error, Result};
use crate::storage::models::UserProfile;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

/// Request to update user profile
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateProfileRequest {
    // Identity
    pub full_name: Option<String>,
    pub preferred_name: Option<String>,
    /// Parsed by serde from `"YYYY-MM-DD"` (what an HTML date input submits);
    /// bound as a real DATE — a String bind fails Postgres's type check.
    pub birth_date: Option<chrono::NaiveDate>,
    // Physical/Biometric
    pub height_cm: Option<f64>,
    pub weight_kg: Option<f64>,
    pub ethnicity: Option<String>,
    // Work/Occupation
    pub occupation: Option<String>,
    pub employer: Option<String>,
    // Home
    pub home_place_id: Option<String>,
    /// "This person is me" — points at a `wiki_people` row (0080).
    pub self_person_id: Option<String>,
    // Onboarding - single status field
    pub onboarding_status: Option<String>,
    // Preferences
    pub theme: Option<String>,
    /// Timezone of the box's physical home location (IANA). See docs/timezone-model.md.
    pub home_timezone: Option<String>,
    // Discovery context
    pub crux: Option<String>,
    pub technology_vision: Option<String>,
    pub pain_point_primary: Option<String>,
    pub pain_point_secondary: Option<String>,
    pub excited_features: Option<serde_json::Value>,
}

/// Get the user's profile (singleton row)
///
/// This will always return a profile, as the migration creates an empty row by default.
pub async fn get_profile(db: &PgPool) -> Result<UserProfile> {
    let profile = sqlx::query_as::<_, UserProfile>(
        r#"
        SELECT *
        FROM app_user_profile
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
    // Build dynamic UPDATE with positional pg placeholders ($1, $2, …).
    let mut set_clauses: Vec<String> = Vec::new();
    let mut next = 1usize;
    let push = |col: &str, set_clauses: &mut Vec<String>, next: &mut usize| {
        set_clauses.push(format!("{} = ${}", col, *next));
        *next += 1;
    };

    if request.full_name.is_some()             { push("full_name", &mut set_clauses, &mut next); }
    if request.preferred_name.is_some()        { push("preferred_name", &mut set_clauses, &mut next); }
    if request.birth_date.is_some()            { push("birth_date", &mut set_clauses, &mut next); }
    if request.height_cm.is_some()             { push("height_cm", &mut set_clauses, &mut next); }
    if request.weight_kg.is_some()             { push("weight_kg", &mut set_clauses, &mut next); }
    if request.ethnicity.is_some()             { push("ethnicity", &mut set_clauses, &mut next); }
    if request.occupation.is_some()            { push("occupation", &mut set_clauses, &mut next); }
    if request.employer.is_some()              { push("employer", &mut set_clauses, &mut next); }
    if request.home_place_id.is_some()         { push("home_place_id", &mut set_clauses, &mut next); }
    if request.self_person_id.is_some()        { push("self_person_id", &mut set_clauses, &mut next); }
    if request.onboarding_status.is_some()     { push("onboarding_status", &mut set_clauses, &mut next); }
    if request.theme.is_some()                 { push("theme", &mut set_clauses, &mut next); }
    if request.home_timezone.is_some()         { push("home_timezone", &mut set_clauses, &mut next); }
    if request.crux.is_some()                  { push("crux", &mut set_clauses, &mut next); }
    if request.technology_vision.is_some()     { push("technology_vision", &mut set_clauses, &mut next); }
    if request.pain_point_primary.is_some()    { push("pain_point_primary", &mut set_clauses, &mut next); }
    if request.pain_point_secondary.is_some()  { push("pain_point_secondary", &mut set_clauses, &mut next); }
    if request.excited_features.is_some()      { push("excited_features", &mut set_clauses, &mut next); }

    if set_clauses.is_empty() {
        // No updates requested, just return current profile
        return get_profile(db).await;
    }

    // Always update updated_at
    set_clauses.push("updated_at = now()".to_string());

    let query = format!(
        "UPDATE app_user_profile SET {} WHERE id = '00000000-0000-0000-0000-000000000001'",
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
    if let Some(ref v) = request.self_person_id {
        query_builder = query_builder.bind(v);
    }
    if let Some(ref v) = request.onboarding_status {
        query_builder = query_builder.bind(v);
    }
    if let Some(ref v) = request.theme {
        query_builder = query_builder.bind(v);
    }
    if let Some(ref v) = request.home_timezone {
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
pub async fn get_display_name(db: &PgPool) -> Result<String> {
    let profile = get_profile(db).await?;

    Ok(profile
        .preferred_name
        .or(profile.full_name)
        .unwrap_or_else(|| "the user".to_string()))
}

/// Get the box's home timezone (IANA), if set. Pure read — no side effects.
///
/// `home_timezone` is the timezone of the box's physical location — a stable
/// anchor + fallback floor, NOT the owner's current location. The per-day
/// "where the owner was" timezone lives on `wiki_days.start_timezone`.
/// See docs/timezone-model.md.
///
/// Returns `None` until [`ensure_home_timezone`] has seeded it (run once at
/// startup); callers fall back to UTC at the boundary in the meantime.
pub async fn get_timezone(db: &PgPool) -> Result<Option<String>> {
    let profile = get_profile(db).await?;
    Ok(profile.home_timezone)
}

/// Seed `home_timezone` from the box's own system clock if it has never been set.
/// Idempotent — a no-op once a value exists. Call once at server startup, before
/// the scheduler resolves cron timezones.
///
/// For a self-hosted appliance configured at home, the system clock IS the home
/// tz. (Cloud/datacenter boxes read "UTC", the honest fallback until the owner
/// sets it explicitly during onboarding / device pairing.)
pub async fn ensure_home_timezone(db: &PgPool) -> Result<()> {
    if get_profile(db).await?.home_timezone.is_some() {
        return Ok(());
    }
    if let Some(sys_tz) = crate::timezone::system_timezone() {
        update_profile(
            db,
            UpdateProfileRequest {
                home_timezone: Some(sys_tz),
                ..Default::default()
            },
        )
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test]
    async fn birth_date_round_trips_as_a_real_date(pool: PgPool) {
        // The column is a Postgres `date`. As Option<String> this round-trip
        // failed on BOTH legs — the bind was rejected at prepare (text vs date)
        // and, had a value ever landed, `SELECT *` failed to decode it, 500ing
        // every GET /profile on the box. NaiveDate is the type the column is.
        let d = chrono::NaiveDate::from_ymd_opt(1990, 1, 15).unwrap();
        let updated = update_profile(
            &pool,
            UpdateProfileRequest { birth_date: Some(d), ..Default::default() },
        )
        .await
        .expect("update with a birth date");
        assert_eq!(updated.birth_date, Some(d));

        let fetched = get_profile(&pool).await.expect("get after set");
        assert_eq!(fetched.birth_date, Some(d));
    }
}

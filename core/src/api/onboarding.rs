//! Onboarding API - Bulk operations for onboarding flow
//!
//! These endpoints handle the specialized needs of onboarding:
//! - Onboarding status tracking with granular completion
//! - Bulk saving of axiology items after user review
//! - Bulk saving of aspirations

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::Result;

// =============================================================================
// Onboarding Status
// =============================================================================

/// Response for onboarding status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingStatus {
    /// Whether user is still in onboarding mode
    pub is_onboarding: bool,
    /// Profile information complete (name required)
    pub profile_complete: bool,
    /// Places added or skipped
    pub places_complete: bool,
    /// Tools connected or skipped
    pub tools_complete: bool,
    /// Axiology conversation complete
    pub axiology_complete: bool,
    /// Overall completion percentage (0-100)
    pub completion_percentage: u8,
    /// Whether all steps are complete
    pub all_complete: bool,
}

/// Available onboarding steps that can be completed
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingStep {
    Profile,
    Places,
    Tools,
    Axiology,
}

impl OnboardingStep {
    pub fn as_str(&self) -> &'static str {
        match self {
            OnboardingStep::Profile => "profile",
            OnboardingStep::Places => "places",
            OnboardingStep::Tools => "tools",
            OnboardingStep::Axiology => "axiology",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "profile" => Some(OnboardingStep::Profile),
            "places" => Some(OnboardingStep::Places),
            "tools" => Some(OnboardingStep::Tools),
            "axiology" => Some(OnboardingStep::Axiology),
            _ => None,
        }
    }
}

/// Get current onboarding status
pub async fn get_onboarding_status(pool: &PgPool) -> Result<OnboardingStatus> {
    let row = sqlx::query!(
        r#"
        SELECT
            is_onboarding,
            onboarding_profile_complete,
            onboarding_places_complete,
            onboarding_tools_complete,
            axiology_complete
        FROM data.user_profile
        LIMIT 1
        "#
    )
    .fetch_one(pool)
    .await?;

    let profile_complete = row.onboarding_profile_complete.unwrap_or(false);
    let places_complete = row.onboarding_places_complete.unwrap_or(false);
    let tools_complete = row.onboarding_tools_complete.unwrap_or(false);
    let axiology_complete = row.axiology_complete.unwrap_or(false);

    // Calculate completion percentage (4 steps)
    let completed_count = [profile_complete, places_complete, tools_complete, axiology_complete]
        .iter()
        .filter(|&&x| x)
        .count();
    let completion_percentage = ((completed_count * 100) / 4) as u8;
    let all_complete = completed_count == 4;

    Ok(OnboardingStatus {
        is_onboarding: row.is_onboarding,
        profile_complete,
        places_complete,
        tools_complete,
        axiology_complete,
        completion_percentage,
        all_complete,
    })
}

/// Mark a specific onboarding step as complete
pub async fn complete_step(pool: &PgPool, step: OnboardingStep) -> Result<OnboardingStatus> {
    let column = match step {
        OnboardingStep::Profile => "onboarding_profile_complete",
        OnboardingStep::Places => "onboarding_places_complete",
        OnboardingStep::Tools => "onboarding_tools_complete",
        OnboardingStep::Axiology => "axiology_complete",
    };

    // Use format! since we can't use bind for column names
    let query = format!(
        "UPDATE data.user_profile SET {} = TRUE",
        column
    );
    sqlx::query(&query).execute(pool).await?;

    // Check if all steps complete and auto-finish onboarding
    let status = get_onboarding_status(pool).await?;
    if status.all_complete {
        sqlx::query!(
            r#"
            UPDATE data.user_profile
            SET is_onboarding = FALSE
            "#
        )
        .execute(pool)
        .await?;

        // Re-fetch status with updated is_onboarding
        return get_onboarding_status(pool).await;
    }

    Ok(status)
}

/// Skip a specific onboarding step (marks as complete without action)
pub async fn skip_step(pool: &PgPool, step: OnboardingStep) -> Result<OnboardingStatus> {
    // Skipping is the same as completing - we just mark it done
    complete_step(pool, step).await
}

// =============================================================================
// Request/Response Types
// =============================================================================

/// A single extracted axiology item from the discovery conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedAxiologyItem {
    pub title: String,
    pub description: Option<String>,
}

/// Request to save all axiology items from onboarding review
#[derive(Debug, Deserialize)]
pub struct SaveAxiologyRequest {
    /// The user's life purpose (exactly one)
    pub telos: Option<ExtractedAxiologyItem>,
    /// Character strengths to cultivate
    pub virtues: Vec<ExtractedAxiologyItem>,
    /// Patterns to resist/overcome
    pub vices: Vec<ExtractedAxiologyItem>,
    /// Natural dispositions
    pub temperaments: Vec<ExtractedAxiologyItem>,
    /// External affinities
    pub preferences: Vec<ExtractedAxiologyItem>,
    /// The original reflection text (for reference)
    pub reflection: Option<String>,
}

/// Response after saving axiology items
#[derive(Debug, Serialize)]
pub struct SaveAxiologyResponse {
    pub telos_id: Option<Uuid>,
    pub virtue_ids: Vec<Uuid>,
    pub vice_ids: Vec<Uuid>,
    pub temperament_ids: Vec<Uuid>,
    pub preference_ids: Vec<Uuid>,
}

/// A single aspiration from onboarding
#[derive(Debug, Deserialize)]
pub struct OnboardingAspiration {
    pub title: String,
    pub description: Option<String>,
    pub target_timeframe: Option<String>,
}

/// Request to save aspirations from onboarding
#[derive(Debug, Deserialize)]
pub struct SaveAspirationsRequest {
    pub aspirations: Vec<OnboardingAspiration>,
}

/// Response after saving aspirations
#[derive(Debug, Serialize)]
pub struct SaveAspirationsResponse {
    pub aspiration_ids: Vec<Uuid>,
}

// =============================================================================
// API Functions
// =============================================================================

/// Save all axiology items from onboarding in a single transaction
///
/// This creates:
/// - 0-1 telos (if provided, archives any existing active telos)
/// - 0+ virtues
/// - 0+ vices
/// - 0+ temperaments
/// - 0+ preferences
pub async fn save_onboarding_axiology(
    pool: &PgPool,
    request: SaveAxiologyRequest,
) -> Result<SaveAxiologyResponse> {
    let mut tx = pool.begin().await?;

    let mut response = SaveAxiologyResponse {
        telos_id: None,
        virtue_ids: Vec::new(),
        vice_ids: Vec::new(),
        temperament_ids: Vec::new(),
        preference_ids: Vec::new(),
    };

    // Handle telos (only one active at a time)
    if let Some(telos) = request.telos {
        // Archive any existing active telos
        sqlx::query!(
            r#"
            UPDATE data.axiology_telos
            SET is_active = FALSE
            WHERE is_active = TRUE
            "#
        )
        .execute(&mut *tx)
        .await?;

        // Create new telos
        let telos_row = sqlx::query!(
            r#"
            INSERT INTO data.axiology_telos (title, description, is_active)
            VALUES ($1, $2, TRUE)
            RETURNING id
            "#,
            telos.title,
            telos.description
        )
        .fetch_one(&mut *tx)
        .await?;

        response.telos_id = Some(telos_row.id);
    }

    // Create virtues
    for virtue in request.virtues {
        let row = sqlx::query!(
            r#"
            INSERT INTO data.axiology_virtue (title, description, is_active)
            VALUES ($1, $2, TRUE)
            RETURNING id
            "#,
            virtue.title,
            virtue.description
        )
        .fetch_one(&mut *tx)
        .await?;

        response.virtue_ids.push(row.id);
    }

    // Create vices
    for vice in request.vices {
        let row = sqlx::query!(
            r#"
            INSERT INTO data.axiology_vice (title, description, is_active)
            VALUES ($1, $2, TRUE)
            RETURNING id
            "#,
            vice.title,
            vice.description
        )
        .fetch_one(&mut *tx)
        .await?;

        response.vice_ids.push(row.id);
    }

    // Create temperaments
    for temperament in request.temperaments {
        let row = sqlx::query!(
            r#"
            INSERT INTO data.axiology_temperament (title, description, is_active)
            VALUES ($1, $2, TRUE)
            RETURNING id
            "#,
            temperament.title,
            temperament.description
        )
        .fetch_one(&mut *tx)
        .await?;

        response.temperament_ids.push(row.id);
    }

    // Create preferences
    for preference in request.preferences {
        let row = sqlx::query!(
            r#"
            INSERT INTO data.axiology_preference (title, description, preference_domain, is_active)
            VALUES ($1, $2, 'general', TRUE)
            RETURNING id
            "#,
            preference.title,
            preference.description
        )
        .fetch_one(&mut *tx)
        .await?;

        response.preference_ids.push(row.id);
    }

    tx.commit().await?;

    Ok(response)
}

/// Save aspirations from onboarding
pub async fn save_onboarding_aspirations(
    pool: &PgPool,
    request: SaveAspirationsRequest,
) -> Result<SaveAspirationsResponse> {
    let mut aspiration_ids = Vec::new();

    for aspiration in request.aspirations {
        if aspiration.title.trim().is_empty() {
            continue;
        }

        let row = sqlx::query!(
            r#"
            INSERT INTO data.praxis_aspiration (
                title,
                description,
                target_timeframe,
                source_provider,
                is_active
            )
            VALUES ($1, $2, $3, 'onboarding', TRUE)
            RETURNING id
            "#,
            aspiration.title,
            aspiration.description,
            aspiration.target_timeframe
        )
        .fetch_one(pool)
        .await?;

        aspiration_ids.push(row.id);
    }

    Ok(SaveAspirationsResponse { aspiration_ids })
}

/// Mark onboarding as complete
pub async fn complete_onboarding(pool: &PgPool) -> Result<()> {
    // Set onboarding_step to NULL to indicate completion
    // The is_onboarding flag is set separately via the profile update
    sqlx::query!(
        r#"
        UPDATE data.user_profile
        SET onboarding_step = NULL
        "#
    )
    .execute(pool)
    .await?;

    Ok(())
}

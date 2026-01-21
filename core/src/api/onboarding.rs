//! Onboarding API - Status tracking for onboarding flow
//!
//! These endpoints handle the specialized needs of onboarding:
//! - Onboarding status tracking with step-based completion
//! - Bulk saving of aspirations

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::error::Result;

// =============================================================================
// Onboarding Status
// =============================================================================

/// Response for onboarding status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingStatus {
    /// Current onboarding step
    pub status: String,
    /// Whether onboarding is complete
    pub is_complete: bool,
    /// Overall completion percentage (0-100)
    pub completion_percentage: u8,
}

/// Available onboarding steps that can be completed
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingStep {
    Welcome,
    Profile,
    Places,
    Tools,
    Complete,
}

impl OnboardingStep {
    pub fn as_str(&self) -> &'static str {
        match self {
            OnboardingStep::Welcome => "welcome",
            OnboardingStep::Profile => "profile",
            OnboardingStep::Places => "places",
            OnboardingStep::Tools => "tools",
            OnboardingStep::Complete => "complete",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "welcome" => Some(OnboardingStep::Welcome),
            "profile" => Some(OnboardingStep::Profile),
            "places" => Some(OnboardingStep::Places),
            "tools" => Some(OnboardingStep::Tools),
            "complete" => Some(OnboardingStep::Complete),
            _ => None,
        }
    }

    /// Get the next step in the onboarding flow
    pub fn next(&self) -> Option<Self> {
        match self {
            OnboardingStep::Welcome => Some(OnboardingStep::Profile),
            OnboardingStep::Profile => Some(OnboardingStep::Places),
            OnboardingStep::Places => Some(OnboardingStep::Tools),
            OnboardingStep::Tools => Some(OnboardingStep::Complete),
            OnboardingStep::Complete => None,
        }
    }

    /// Get completion percentage for this step
    pub fn completion_percentage(&self) -> u8 {
        match self {
            OnboardingStep::Welcome => 0,
            OnboardingStep::Profile => 25,
            OnboardingStep::Places => 50,
            OnboardingStep::Tools => 75,
            OnboardingStep::Complete => 100,
        }
    }
}

/// Get current onboarding status
pub async fn get_onboarding_status(pool: &SqlitePool) -> Result<OnboardingStatus> {
    let row = sqlx::query!(
        r#"
        SELECT onboarding_status
        FROM data_user_profile
        LIMIT 1
        "#
    )
    .fetch_one(pool)
    .await?;

    let status = row.onboarding_status.clone();
    let step = OnboardingStep::from_str(&status).unwrap_or(OnboardingStep::Welcome);
    let is_complete = step == OnboardingStep::Complete;
    let completion_percentage = step.completion_percentage();

    Ok(OnboardingStatus {
        status,
        is_complete,
        completion_percentage,
    })
}

/// Mark a specific onboarding step as complete and advance to next step
pub async fn complete_step(pool: &SqlitePool, step: OnboardingStep) -> Result<OnboardingStatus> {
    // Get the next step
    let next_status = step.next().unwrap_or(OnboardingStep::Complete).as_str();

    sqlx::query!(
        r#"
        UPDATE data_user_profile
        SET onboarding_status = $1
        "#,
        next_status
    )
    .execute(pool)
    .await?;

    get_onboarding_status(pool).await
}

/// Skip a specific onboarding step (advances to next step)
pub async fn skip_step(pool: &SqlitePool, step: OnboardingStep) -> Result<OnboardingStatus> {
    // Skipping is the same as completing - we just advance to the next step
    complete_step(pool, step).await
}

// =============================================================================
// Request/Response Types
// =============================================================================

/// A single extracted axiology item from the discovery conversation
/// Note: Axiology tables have been removed, but we keep this type for API compatibility
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedAxiologyItem {
    pub title: String,
    pub description: Option<String>,
}

/// Request to save all axiology items from onboarding review
/// Note: Axiology tables have been removed. This now only stores telos.
#[derive(Debug, Deserialize)]
pub struct SaveAxiologyRequest {
    /// The user's life purpose (exactly one)
    pub telos: Option<ExtractedAxiologyItem>,
    /// Character strengths to cultivate (no longer stored)
    #[serde(default)]
    pub virtues: Vec<ExtractedAxiologyItem>,
    /// Patterns to resist/overcome (no longer stored)
    #[serde(default)]
    pub vices: Vec<ExtractedAxiologyItem>,
    /// Natural dispositions (no longer stored)
    #[serde(default)]
    pub temperaments: Vec<ExtractedAxiologyItem>,
    /// External affinities (no longer stored)
    #[serde(default)]
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

/// Save telos from onboarding (axiology tables have been removed)
///
/// This creates:
/// - 0-1 telos (if provided, archives any existing active telos)
/// - Other axiology items are no longer stored but accepted for API compatibility
pub async fn save_onboarding_axiology(
    pool: &SqlitePool,
    request: SaveAxiologyRequest,
) -> Result<SaveAxiologyResponse> {
    let mut response = SaveAxiologyResponse {
        telos_id: None,
        virtue_ids: Vec::new(),
        vice_ids: Vec::new(),
        temperament_ids: Vec::new(),
        preference_ids: Vec::new(),
    };

    // Handle telos (only one active at a time)
    if let Some(telos) = request.telos {
        let mut tx = pool.begin().await?;

        // Archive any existing active telos
        sqlx::query!(
            r#"
            UPDATE data_telos
            SET is_active = FALSE
            WHERE is_active = TRUE
            "#
        )
        .execute(&mut *tx)
        .await?;

        // Create new telos
        let telos_id = Uuid::new_v4().to_string();
        let telos_row = sqlx::query!(
            r#"
            INSERT INTO data_telos (id, title, description, is_active)
            VALUES ($1, $2, $3, TRUE)
            RETURNING id
            "#,
            telos_id,
            telos.title,
            telos.description
        )
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        // SQLite returns id as Option<String>, need to parse to Uuid
        if let Some(id_str) = telos_row.id.as_ref() {
            if let Ok(uuid) = Uuid::parse_str(id_str) {
                response.telos_id = Some(uuid);
            }
        }
    }

    // Note: virtues, vices, temperaments, preferences are no longer stored
    // The request fields are accepted for API compatibility but ignored

    Ok(response)
}

/// Save aspirations from onboarding
pub async fn save_onboarding_aspirations(
    pool: &SqlitePool,
    request: SaveAspirationsRequest,
) -> Result<SaveAspirationsResponse> {
    let mut aspiration_ids = Vec::new();

    for aspiration in request.aspirations {
        if aspiration.title.trim().is_empty() {
            continue;
        }

        let aspiration_id = Uuid::new_v4().to_string();
        let row = sqlx::query!(
            r#"
            INSERT INTO data_praxis_aspiration (
                id,
                title,
                description,
                target_timeframe,
                source_provider,
                status
            )
            VALUES ($1, $2, $3, $4, 'internal', 'dreaming')
            RETURNING id
            "#,
            aspiration_id,
            aspiration.title,
            aspiration.description,
            aspiration.target_timeframe
        )
        .fetch_one(pool)
        .await?;

        // SQLite returns id as Option<String>, need to parse to Uuid
        if let Some(id_str) = row.id.as_ref() {
            if let Ok(uuid) = Uuid::parse_str(id_str) {
                aspiration_ids.push(uuid);
            }
        }
    }

    Ok(SaveAspirationsResponse { aspiration_ids })
}

/// Mark onboarding as complete
pub async fn complete_onboarding(pool: &SqlitePool) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE data_user_profile
        SET onboarding_status = 'complete'
        "#
    )
    .execute(pool)
    .await?;

    Ok(())
}

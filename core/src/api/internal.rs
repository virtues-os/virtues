//! Internal API endpoints for Tollbooth and Atlas integration
//!
//! These endpoints are not exposed to users and are authenticated via
//! shared secret headers (X-Tollbooth-Secret).

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// Server status states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerStatus {
    /// Container just started, waiting for Tollbooth hydration
    Provisioning,
    /// Restoring from cold storage (zombie wake-up)
    Migrating,
    /// Normal operation
    Ready,
}

impl ServerStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ServerStatus::Provisioning => "provisioning",
            ServerStatus::Migrating => "migrating",
            ServerStatus::Ready => "ready",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "provisioning" => Some(ServerStatus::Provisioning),
            "migrating" => Some(ServerStatus::Migrating),
            "ready" => Some(ServerStatus::Ready),
            _ => None,
        }
    }
}

/// Request from Tollbooth to hydrate user profile on first request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HydrateRequest {
    /// User's email (from provisioning)
    pub email: String,
    /// Full name (if collected during signup)
    pub full_name: Option<String>,
    /// Preferred/display name
    pub preferred_name: Option<String>,
    /// Subscription tier: "standard", "pro"
    pub tier: Option<String>,
}

/// Response after hydration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HydrateResponse {
    /// Whether this was the first hydration (profile was in provisioning state)
    pub was_first_hydration: bool,
    /// Current server status after hydration
    pub server_status: String,
    /// Profile display name
    pub display_name: Option<String>,
}

/// Hydrate user profile from Tollbooth
///
/// Called by Tollbooth on the first request to a newly provisioned container.
/// Seeds the profile with data from Atlas provisioning and marks the server as ready.
pub async fn hydrate_profile(pool: &SqlitePool, request: HydrateRequest) -> Result<HydrateResponse> {
    // Get current status
    let row = sqlx::query!(
        r#"
        SELECT server_status, preferred_name, full_name
        FROM app_user_profile 
        WHERE id = '00000000-0000-0000-0000-000000000001'
        "#
    )
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch profile: {}", e)))?;

    let was_first_hydration = row.server_status == "provisioning";
    
    // Normalize email
    let email = request.email.trim().to_lowercase();
    
    // Update profile with hydration data
    sqlx::query!(
        r#"
        UPDATE app_user_profile 
        SET 
            owner_email = $1,
            full_name = COALESCE($2, full_name),
            preferred_name = COALESCE($3, preferred_name),
            server_status = 'ready',
            onboarding_status = 'complete',
            updated_at = datetime('now')
        WHERE id = '00000000-0000-0000-0000-000000000001'
        "#,
        email,
        request.full_name,
        request.preferred_name
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to hydrate profile: {}", e)))?;
    
    // Determine display name for response
    let display_name = request.preferred_name
        .or(request.full_name)
        .or(row.preferred_name)
        .or(row.full_name);

    tracing::info!(
        email = %email,
        was_first = was_first_hydration,
        "Profile hydrated from Tollbooth"
    );

    Ok(HydrateResponse {
        was_first_hydration,
        server_status: "ready".to_string(),
        display_name,
    })
}

/// Get current server status
pub async fn get_server_status(pool: &SqlitePool) -> Result<ServerStatus> {
    let row = sqlx::query!(
        r#"
        SELECT server_status
        FROM app_user_profile 
        WHERE id = '00000000-0000-0000-0000-000000000001'
        "#
    )
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch server status: {}", e)))?;

    ServerStatus::from_str(&row.server_status)
        .ok_or_else(|| Error::Other(format!("Invalid server_status: {}", row.server_status)))
}

/// Mark server as ready (used in dev mode)
pub async fn mark_server_ready(pool: &SqlitePool) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE app_user_profile 
        SET 
            server_status = 'ready',
            onboarding_status = 'complete',
            updated_at = datetime('now')
        WHERE id = '00000000-0000-0000-0000-000000000001'
        "#
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to mark server ready: {}", e)))?;

    Ok(())
}

/// Ensure server_status is correct on startup
///
/// If the profile already has an owner_email, the server was previously
/// hydrated and should be marked ready immediately. This prevents the
/// "Setting up your server" screen from appearing after container
/// restarts or rolling updates where the /data volume persists.
///
/// If owner_email is null, this is a genuinely fresh provision and we
/// leave server_status as 'provisioning' to wait for the Atlas hydrate call.
pub async fn ensure_server_status(pool: &SqlitePool) -> Result<()> {
    let row = sqlx::query!(
        r#"
        SELECT server_status, owner_email
        FROM app_user_profile 
        WHERE id = '00000000-0000-0000-0000-000000000001'
        "#
    )
    .fetch_one(pool)
    .await;

    match row {
        Ok(r) if r.server_status == "provisioning" && r.owner_email.is_some() => {
            // Previously hydrated server restarting — mark ready immediately
            mark_server_ready(pool).await?;
            tracing::info!("Server previously hydrated, auto-marked as ready on startup");
        }
        Ok(r) if r.server_status == "provisioning" => {
            tracing::info!("Fresh provision — waiting for Atlas hydration");
        }
        Ok(_) => {
            tracing::debug!("Server already in ready state");
        }
        Err(e) => {
            tracing::warn!("Could not check server status: {}", e);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_status_roundtrip() {
        assert_eq!(ServerStatus::from_str("provisioning"), Some(ServerStatus::Provisioning));
        assert_eq!(ServerStatus::from_str("ready"), Some(ServerStatus::Ready));
        assert_eq!(ServerStatus::from_str("migrating"), Some(ServerStatus::Migrating));
        assert_eq!(ServerStatus::from_str("invalid"), None);
        
        assert_eq!(ServerStatus::Provisioning.as_str(), "provisioning");
        assert_eq!(ServerStatus::Ready.as_str(), "ready");
    }
}

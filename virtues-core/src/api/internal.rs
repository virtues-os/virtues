//! Internal API endpoints for virtues-api and Atlas integration
//!
//! These endpoints are not exposed to users and are authenticated via
//! shared secret headers (X-Virtues-Api-Secret).

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

/// Server status states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerStatus {
    /// Container just started, waiting for virtues-api hydration
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

/// Request from virtues-api to hydrate user profile on first request
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

/// Hydrate user profile from virtues-api
///
/// Called by virtues-api on the first request to a newly provisioned container.
/// Seeds the profile with data from Atlas provisioning and marks the server as ready.
pub async fn hydrate_profile(pool: &PgPool, request: HydrateRequest) -> Result<HydrateResponse> {
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
    
    // Pair-only auth has no email-as-identity concept, but Atlas still sends
    // display-name fields so the profile starts with a friendly name. We
    // ignore `request.email` server-side.
    sqlx::query(
        "UPDATE app_user_profile \
         SET full_name = COALESCE($1, full_name), \
             preferred_name = COALESCE($2, preferred_name), \
             server_status = 'ready', \
             updated_at = now() \
         WHERE id = '00000000-0000-0000-0000-000000000001'",
    )
    .bind(request.full_name.as_deref())
    .bind(request.preferred_name.as_deref())
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to hydrate profile: {}", e)))?;

    let display_name = request.preferred_name
        .or(request.full_name)
        .or(row.preferred_name)
        .or(row.full_name);

    tracing::info!(
        was_first = was_first_hydration,
        "Profile hydrated from virtues-api"
    );

    Ok(HydrateResponse {
        was_first_hydration,
        server_status: "ready".to_string(),
        display_name,
    })
}

/// Get current server status
pub async fn get_server_status(pool: &PgPool) -> Result<ServerStatus> {
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
pub async fn mark_server_ready(pool: &PgPool) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE app_user_profile 
        SET 
            server_status = 'ready',
            updated_at = now()
        WHERE id = '00000000-0000-0000-0000-000000000001'
        "#
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to mark server ready: {}", e)))?;

    Ok(())
}

/// Ensure server_status is correct on startup.
///
/// In pair-only auth there is no "wait for Atlas to send our owner email"
/// stage. If the row is still `provisioning` at boot, the migration is done
/// and we're ready — flip immediately. Atlas may still call `/internal/hydrate`
/// later to set the display name, but that no longer gates readiness.
pub async fn ensure_server_status(pool: &PgPool) -> Result<()> {
    let row: std::result::Result<(String,), sqlx::Error> = sqlx::query_as(
        "SELECT server_status FROM app_user_profile \
         WHERE id = '00000000-0000-0000-0000-000000000001'",
    )
    .fetch_one(pool)
    .await;

    match row {
        Ok((status,)) if status == "provisioning" => {
            mark_server_ready(pool).await?;
            tracing::info!("Server marked ready on startup");
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

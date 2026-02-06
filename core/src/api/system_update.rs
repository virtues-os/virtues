//! System Update API - Pull-based update checking via Tollbooth → Atlas
//!
//! Core periodically checks Tollbooth for the latest available version.
//! When a newer version is detected, the frontend shows an "Update available" toast.
//! The user can trigger an update which flows: Core → Tollbooth → Atlas → Nomad.
//!
//! Endpoints (registered in server/mod.rs):
//! - GET /api/system/update-available → returns { available, current, latest }
//! - POST /api/system/update → triggers rolling update via Tollbooth → Atlas

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};

/// Current build commit (baked in at compile time)
const CURRENT_COMMIT: &str = env!("GIT_COMMIT");

/// Polling interval for version checks (5 minutes)
const VERSION_CHECK_INTERVAL_SECS: u64 = 300;

/// Version info from Tollbooth/Atlas
/// Fields are Option because standalone mode returns nulls
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VersionResponse {
    pub version: Option<String>,
    /// Accepts both "image" and "image_tag" from Atlas for backward compatibility
    #[serde(alias = "image_tag")]
    pub image: Option<String>,
}

/// Resolved version info (non-null fields only)
#[derive(Debug, Clone)]
pub struct VersionInfo {
    pub version: String,
    pub image: String,
}

/// Cached update status exposed to the frontend
#[derive(Debug, Clone, Serialize)]
pub struct UpdateStatus {
    /// Whether a newer version is available
    pub available: bool,
    /// Current running version (git SHA)
    pub current: String,
    /// Latest available version (git SHA), if known
    pub latest: Option<String>,
    /// Docker image for the latest version, if known
    pub latest_image: Option<String>,
}

/// Shared update state, accessible from route handlers and the background task
#[derive(Debug, Clone)]
pub struct UpdateState {
    inner: Arc<RwLock<UpdateStatus>>,
}

impl UpdateState {
    /// Create a new update state with no update available
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(UpdateStatus {
                available: false,
                current: CURRENT_COMMIT.to_string(),
                latest: None,
                latest_image: None,
            })),
        }
    }

    /// Get the current update status
    pub async fn get(&self) -> UpdateStatus {
        self.inner.read().await.clone()
    }

    /// Update the latest version info
    async fn set_latest(&self, info: VersionInfo) {
        let mut lock = self.inner.write().await;
        let is_different = info.version != CURRENT_COMMIT;
        lock.available = is_different;
        lock.latest = Some(info.version);
        lock.latest_image = Some(info.image);
    }
}

/// Background task that polls Tollbooth for version updates
pub async fn run_version_checker(update_state: UpdateState) {
    let tollbooth_url = match std::env::var("TOLLBOOTH_URL") {
        Ok(url) => url,
        Err(_) => {
            tracing::debug!("TOLLBOOTH_URL not set, version checker disabled");
            return;
        }
    };

    let secret = match std::env::var("TOLLBOOTH_INTERNAL_SECRET") {
        Ok(s) => s,
        Err(_) => {
            tracing::debug!("TOLLBOOTH_INTERNAL_SECRET not set, version checker disabled");
            return;
        }
    };

    tracing::info!(
        "Version checker started (current: {}, interval: {}s)",
        CURRENT_COMMIT,
        VERSION_CHECK_INTERVAL_SECS
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("Failed to build HTTP client for version checker");

    let mut tick = interval(Duration::from_secs(VERSION_CHECK_INTERVAL_SECS));

    loop {
        tick.tick().await;

        match check_version(&client, &tollbooth_url, &secret).await {
            Ok(Some(info)) => {
                let is_update = info.version != CURRENT_COMMIT;
                if is_update {
                    tracing::info!(
                        "Update available: current={}, latest={}",
                        CURRENT_COMMIT,
                        info.version
                    );
                }
                update_state.set_latest(info).await;
            }
            Ok(None) => {
                // Standalone mode — no version info from Atlas, nothing to do
            }
            Err(e) => {
                tracing::debug!("Version check failed (will retry): {}", e);
            }
        }
    }
}

/// Check Tollbooth for the latest available version
///
/// Returns None if Tollbooth is in standalone mode (no Atlas configured)
async fn check_version(
    client: &reqwest::Client,
    tollbooth_url: &str,
    secret: &str,
) -> Result<Option<VersionInfo>> {
    let resp = crate::tollbooth::with_system_auth(
        client.get(format!("{}/v1/version", tollbooth_url)),
        secret,
    )
    .send()
    .await
    .map_err(|e| Error::Network(format!("Tollbooth version check failed: {}", e)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(Error::Network(format!(
            "Tollbooth version check error ({}): {}",
            status, body
        )));
    }

    let response: VersionResponse = resp
        .json()
        .await
        .map_err(|e| Error::Network(format!("Failed to parse version response: {}", e)))?;

    // In standalone mode, version is null — no update info available
    match (response.version, response.image) {
        (Some(version), Some(image)) => Ok(Some(VersionInfo { version, image })),
        (Some(version), None) => Ok(Some(VersionInfo {
            image: String::new(),
            version,
        })),
        _ => Ok(None),
    }
}

/// Trigger a rolling update via Tollbooth → Atlas
///
/// Called when the user clicks "Update" in the frontend.
pub async fn trigger_update() -> Result<serde_json::Value> {
    let tollbooth_url = std::env::var("TOLLBOOTH_URL")
        .map_err(|_| Error::Configuration("TOLLBOOTH_URL not set".to_string()))?;
    let secret = std::env::var("TOLLBOOTH_INTERNAL_SECRET")
        .map_err(|_| Error::Configuration("TOLLBOOTH_INTERNAL_SECRET not set".to_string()))?;

    let client = reqwest::Client::new();
    let resp = crate::tollbooth::with_system_auth(
        client.post(format!("{}/v1/update", tollbooth_url)),
        &secret,
    )
    .send()
    .await
    .map_err(|e| Error::Network(format!("Tollbooth update request failed: {}", e)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(Error::Network(format!(
            "Update trigger error ({}): {}",
            status, body
        )));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| Error::Network(format!("Failed to parse update response: {}", e)))?;

    Ok(body)
}

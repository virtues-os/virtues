//! Version Cache - Shared state for tracking latest available version
//!
//! A lightweight cache the `/v1/version` route handler fills (from Atlas) on a
//! cache-miss and reads on subsequent requests.

use std::sync::Arc;
use tokio::sync::RwLock;

/// Cached version info from Atlas
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VersionInfo {
    /// Latest available version (git SHA)
    pub version: String,
    /// Docker image reference (e.g., "ghcr.io/virtues-os/virtues-core:abc1234")
    /// Accepts both "image" and "image_tag" from Atlas for backward compatibility
    #[serde(alias = "image_tag")]
    pub image: String,
    /// When this version was set in Atlas (ISO-8601)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Thread-safe version cache shared with the version route handlers
#[derive(Debug, Clone)]
pub struct VersionCache {
    inner: Arc<RwLock<Option<VersionInfo>>>,
}

impl VersionCache {
    /// Create a new empty version cache
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
        }
    }

    /// Update the cached version info (called by the version route on a cache-miss fetch)
    pub async fn set(&self, info: VersionInfo) {
        let mut lock = self.inner.write().await;
        *lock = Some(info);
    }

    /// Get the cached version info (called by version route handler)
    pub async fn get(&self) -> Option<VersionInfo> {
        let lock = self.inner.read().await;
        lock.clone()
    }
}

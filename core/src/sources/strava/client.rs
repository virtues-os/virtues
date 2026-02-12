//! Strava API client - thin wrapper over OAuthHttpClient
//!
//! This client delegates all OAuth HTTP operations to the base OAuthHttpClient,
//! providing Strava-specific configuration.

use serde::de::DeserializeOwned;
use std::sync::Arc;

use crate::{
    error::Result,
    sources::base::{OAuthHttpClient, RetryConfig, TokenManager},
};

/// Strava API client with automatic token refresh and retry logic
///
/// This is a thin wrapper that configures OAuthHttpClient for Strava APIs.
/// All HTTP logic (retry, token refresh, error handling) is delegated to the base client.
pub struct StravaClient {
    http: OAuthHttpClient,
}

impl StravaClient {
    /// Create a new Strava API client
    pub fn new(source_id: String, token_manager: Arc<TokenManager>) -> Self {
        Self {
            http: OAuthHttpClient::new(source_id, token_manager)
                .with_base_url("https://www.strava.com/api/v3")
                .with_retry_config(RetryConfig::default()),
        }
    }

    /// Make an authenticated GET request
    pub async fn get<T>(&self, path: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.http.get(path).await
    }

    /// Make an authenticated GET request with query parameters
    pub async fn get_with_params<T>(&self, path: &str, params: &[(&str, &str)]) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.http.get_with_params(path, params).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_client_creation() {
        let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        let token_manager = Arc::new(TokenManager::new_insecure(pool));
        let _client = StravaClient::new("test-source".to_string(), token_manager);
    }
}

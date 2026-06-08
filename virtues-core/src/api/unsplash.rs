//! Unsplash Image Search API
//!
//! Provides image search for cover images using the Unsplash API.
//! Requests are proxied through virtues-api for budget enforcement.
//! @see https://unsplash.com/documentation for API documentation

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::error::{Error, Result};
use crate::virtues_api::client::BearerClient;

/// Search request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRequest {
    /// The search query
    pub query: String,

    /// Page number (default 1)
    #[serde(default = "default_page")]
    pub page: u32,

    /// Results per page (default 20, max 30)
    #[serde(default = "default_per_page")]
    pub per_page: u32,
}

fn default_page() -> u32 { 1 }
fn default_per_page() -> u32 { 20 }

/// A single photo result (simplified from Unsplash response)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhotoResult {
    pub id: String,
    pub description: Option<String>,
    pub urls: PhotoUrls,
    pub user: PhotoUser,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhotoUrls {
    pub raw: String,
    pub full: String,
    pub regular: String,
    pub small: String,
    pub thumb: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhotoUser {
    pub name: String,
    pub username: String,
}

/// Search response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub total: u32,
    pub total_pages: u32,
    pub results: Vec<PhotoResult>,
}

/// Search Unsplash photos (proxied through virtues-api)
pub async fn search(pool: &PgPool, request: SearchRequest) -> Result<SearchResponse> {
    if request.query.trim().is_empty() {
        return Err(Error::InvalidInput("Search query cannot be empty".into()));
    }

    let body = serde_json::to_value(&request)
        .map_err(|e| Error::ExternalApi(format!("Failed to serialize Unsplash request: {}", e)))?;

    let response = BearerClient::from_env(pool.clone())
        .post_json("/v1/unsplash/search", &body)
        .await
        .map_err(|e| Error::ExternalApi(format!("virtues-api/Unsplash request failed: {}", e)))?;

    if !response.is_success() {
        return Err(Error::ExternalApi(format!(
            "virtues-api/Unsplash error ({}): {}",
            response.status, response.body
        )));
    }

    let search_response: SearchResponse = serde_json::from_value(response.body)
        .map_err(|e| Error::ExternalApi(format!("Failed to parse Unsplash response: {}", e)))?;

    Ok(search_response)
}

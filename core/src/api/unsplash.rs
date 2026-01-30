//! Unsplash Image Search API
//!
//! Provides image search for cover images using the Unsplash API.
//! Requests are proxied through Tollbooth for budget enforcement.
//! @see https://unsplash.com/documentation for API documentation

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::tollbooth;

/// Get Tollbooth URL from environment (defaults to localhost:9002 for development)
fn get_tollbooth_url() -> String {
    std::env::var("TOLLBOOTH_URL").unwrap_or_else(|_| "http://localhost:9002".to_string())
}

/// Get Tollbooth internal secret from environment
fn get_tollbooth_secret() -> Result<String> {
    std::env::var("TOLLBOOTH_INTERNAL_SECRET").map_err(|_| {
        Error::Configuration("TOLLBOOTH_INTERNAL_SECRET environment variable not set".into())
    })
}

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

/// Search Unsplash photos (proxied through Tollbooth)
pub async fn search(request: SearchRequest) -> Result<SearchResponse> {
    let secret = get_tollbooth_secret()?;

    if request.query.trim().is_empty() {
        return Err(Error::InvalidInput("Search query cannot be empty".into()));
    }

    let tollbooth_url = get_tollbooth_url();

    let client = reqwest::Client::new();
    let response = tollbooth::with_system_auth(
        client.post(format!("{}/v1/services/unsplash/search", tollbooth_url)),
        &secret,
    )
    .header("Content-Type", "application/json")
    .json(&request)
    .send()
    .await
    .map_err(|e| Error::ExternalApi(format!("Tollbooth/Unsplash API request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(Error::ExternalApi(format!(
            "Tollbooth/Unsplash API error ({}): {}",
            status, error_text
        )));
    }

    let search_response: SearchResponse = response
        .json()
        .await
        .map_err(|e| Error::ExternalApi(format!("Failed to parse Unsplash response: {}", e)))?;

    Ok(search_response)
}

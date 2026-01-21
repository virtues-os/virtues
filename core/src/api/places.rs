//! Google Places API proxy for address autocomplete
//!
//! This module provides server-side proxy to Google Places API,
//! avoiding client-side JavaScript origin restrictions.
//!
//! Requests are proxied through Tollbooth for budget enforcement.

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

/// Request for place autocomplete
#[derive(Debug, Deserialize)]
pub struct AutocompleteRequest {
    /// The search query (partial address)
    pub query: String,
    /// Optional session token for billing (groups requests)
    pub session_token: Option<String>,
}

/// A single autocomplete prediction
#[derive(Debug, Serialize)]
pub struct AutocompletePrediction {
    /// The place ID for fetching details
    pub place_id: String,
    /// Human-readable description
    pub description: String,
    /// Main text (typically street address)
    pub main_text: String,
    /// Secondary text (city, state, country)
    pub secondary_text: String,
}

/// Response from autocomplete endpoint
#[derive(Debug, Serialize)]
pub struct AutocompleteResponse {
    pub predictions: Vec<AutocompletePrediction>,
}

/// Request for place details
#[derive(Debug, Deserialize)]
pub struct PlaceDetailsRequest {
    /// The place ID from autocomplete
    pub place_id: String,
    /// Optional session token (should match autocomplete session)
    #[allow(dead_code)]
    pub session_token: Option<String>,
}

/// Response with full place details
#[derive(Debug, Serialize)]
pub struct PlaceDetailsResponse {
    pub place_id: String,
    pub formatted_address: String,
    pub latitude: f64,
    pub longitude: f64,
}

// Tollbooth response types for Google Places (New API format)
#[derive(Debug, Deserialize)]
struct TollboothAutocompleteResponse {
    suggestions: Option<Vec<TollboothSuggestion>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TollboothSuggestion {
    place_prediction: Option<TollboothPlacePrediction>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TollboothPlacePrediction {
    place_id: String,
    text: TollboothText,
    structured_format: Option<TollboothStructuredFormat>,
}

#[derive(Debug, Deserialize)]
struct TollboothText {
    text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TollboothStructuredFormat {
    main_text: TollboothText,
    secondary_text: Option<TollboothText>,
}

// Tollbooth response types for place details (New API format)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TollboothPlaceDetailsResponse {
    id: String,
    #[allow(dead_code)]
    display_name: Option<TollboothText>,
    formatted_address: Option<String>,
    location: Option<TollboothLocation>,
}

#[derive(Debug, Deserialize)]
struct TollboothLocation {
    latitude: f64,
    longitude: f64,
}

/// Get autocomplete predictions for a query (proxied through Tollbooth)
pub async fn autocomplete(request: AutocompleteRequest) -> Result<AutocompleteResponse> {
    let secret = get_tollbooth_secret()?;

    if request.query.trim().is_empty() {
        return Ok(AutocompleteResponse {
            predictions: vec![],
        });
    }

    let tollbooth_url = get_tollbooth_url();

    // Build request body for Tollbooth (which forwards to Google Places New API)
    let mut body = serde_json::json!({
        "input": request.query
    });

    if let Some(token) = &request.session_token {
        body["sessionToken"] = serde_json::json!(token);
    }

    let client = reqwest::Client::new();
    let response = tollbooth::with_system_auth(
        client.post(format!(
            "{}/v1/services/google/places/autocomplete",
            tollbooth_url
        )),
        &secret,
    )
    .header("Content-Type", "application/json")
    .json(&body)
    .send()
    .await
    .map_err(|e| {
        Error::ExternalApi(format!("Tollbooth/Google Places API request failed: {}", e))
    })?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(Error::ExternalApi(format!(
            "Tollbooth/Google Places API error ({}): {}",
            status, error_text
        )));
    }

    let tollbooth_response: TollboothAutocompleteResponse = response.json().await.map_err(|e| {
        Error::ExternalApi(format!("Failed to parse Google Places response: {}", e))
    })?;

    let predictions = tollbooth_response
        .suggestions
        .unwrap_or_default()
        .into_iter()
        .filter_map(|s| s.place_prediction)
        .map(|p| AutocompletePrediction {
            place_id: p.place_id,
            description: p.text.text.clone(),
            main_text: p
                .structured_format
                .as_ref()
                .map(|sf| sf.main_text.text.clone())
                .unwrap_or_else(|| p.text.text.clone()),
            secondary_text: p
                .structured_format
                .as_ref()
                .and_then(|sf| sf.secondary_text.as_ref())
                .map(|t| t.text.clone())
                .unwrap_or_default(),
        })
        .collect();

    Ok(AutocompleteResponse { predictions })
}

/// Get details for a specific place (proxied through Tollbooth)
pub async fn get_place_details(request: PlaceDetailsRequest) -> Result<PlaceDetailsResponse> {
    let secret = get_tollbooth_secret()?;
    let tollbooth_url = get_tollbooth_url();

    let client = reqwest::Client::new();
    let response = tollbooth::with_system_auth(
        client.get(format!(
            "{}/v1/services/google/places/{}",
            tollbooth_url, request.place_id
        )),
        &secret,
    )
    .send()
    .await
    .map_err(|e| {
        Error::ExternalApi(format!("Tollbooth/Google Places API request failed: {}", e))
    })?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(Error::ExternalApi(format!(
            "Tollbooth/Google Places API error ({}): {}",
            status, error_text
        )));
    }

    let tollbooth_response: TollboothPlaceDetailsResponse = response.json().await.map_err(|e| {
        Error::ExternalApi(format!("Failed to parse Google Places response: {}", e))
    })?;

    let location = tollbooth_response
        .location
        .ok_or_else(|| Error::ExternalApi("No location in place details".into()))?;

    Ok(PlaceDetailsResponse {
        place_id: tollbooth_response.id,
        formatted_address: tollbooth_response.formatted_address.unwrap_or_default(),
        latitude: location.latitude,
        longitude: location.longitude,
    })
}

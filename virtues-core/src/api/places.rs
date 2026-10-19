//! Google Places API proxy for address autocomplete
//!
//! This module provides server-side proxy to Google Places API,
//! avoiding client-side JavaScript origin restrictions.
//!
//! Requests are proxied through virtues-api for budget enforcement.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::error::{Error, Result};
use crate::virtues_api::client::BearerClient;

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

// virtues-api response types for Google Places (New API format)
#[derive(Debug, Deserialize)]
struct VirtuesApiAutocompleteResponse {
    suggestions: Option<Vec<VirtuesApiSuggestion>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VirtuesApiSuggestion {
    place_prediction: Option<VirtuesApiPlacePrediction>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VirtuesApiPlacePrediction {
    place_id: String,
    text: VirtuesApiText,
    structured_format: Option<VirtuesApiStructuredFormat>,
}

#[derive(Debug, Deserialize)]
struct VirtuesApiText {
    text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VirtuesApiStructuredFormat {
    main_text: VirtuesApiText,
    secondary_text: Option<VirtuesApiText>,
}

// virtues-api response types for place details (New API format)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VirtuesApiPlaceDetailsResponse {
    id: String,
    #[allow(dead_code)]
    display_name: Option<VirtuesApiText>,
    formatted_address: Option<String>,
    location: Option<VirtuesApiLocation>,
}

#[derive(Debug, Deserialize)]
struct VirtuesApiLocation {
    latitude: f64,
    longitude: f64,
}

/// Get autocomplete predictions for a query (proxied through virtues-api)
pub async fn autocomplete(
    pool: &PgPool,
    request: AutocompleteRequest,
) -> Result<AutocompleteResponse> {
    if request.query.trim().is_empty() {
        return Ok(AutocompleteResponse {
            predictions: vec![],
        });
    }

    // Build request body for virtues-api (which forwards to Google Places New API)
    let mut body = serde_json::json!({
        "input": request.query
    });

    if let Some(token) = &request.session_token {
        body["sessionToken"] = serde_json::json!(token);
    }

    let response = BearerClient::from_env(pool.clone())
        .post_json("/v1/places/autocomplete", &body)
        .await
        .map_err(|e| {
            Error::ExternalApi(format!("virtues-api/Google Places request failed: {}", e))
        })?;

    if !response.is_success() {
        return Err(Error::ExternalApi(format!(
            "virtues-api/Google Places error ({}): {}",
            response.status, response.body
        )));
    }

    let virtues_api_response: VirtuesApiAutocompleteResponse =
        serde_json::from_value(response.body).map_err(|e| {
            Error::ExternalApi(format!("Failed to parse Google Places response: {}", e))
        })?;

    let predictions = virtues_api_response
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

/// Get details for a specific place (proxied through virtues-api)
pub async fn get_place_details(
    pool: &PgPool,
    request: PlaceDetailsRequest,
) -> Result<PlaceDetailsResponse> {
    let response = BearerClient::from_env(pool.clone())
        .get_json(&format!("/v1/places/{}", request.place_id))
        .await
        .map_err(|e| {
            Error::ExternalApi(format!("virtues-api/Google Places request failed: {}", e))
        })?;

    if !response.is_success() {
        return Err(Error::ExternalApi(format!(
            "virtues-api/Google Places error ({}): {}",
            response.status, response.body
        )));
    }

    let virtues_api_response: VirtuesApiPlaceDetailsResponse =
        serde_json::from_value(response.body).map_err(|e| {
            Error::ExternalApi(format!("Failed to parse Google Places response: {}", e))
        })?;

    let location = virtues_api_response
        .location
        .ok_or_else(|| Error::ExternalApi("No location in place details".into()))?;

    Ok(PlaceDetailsResponse {
        place_id: virtues_api_response.id,
        formatted_address: virtues_api_response.formatted_address.unwrap_or_default(),
        latitude: location.latitude,
        longitude: location.longitude,
    })
}

//! Google Places API proxy for address autocomplete
//!
//! This module provides server-side proxy to Google Places API,
//! avoiding client-side JavaScript origin restrictions.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

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

// Google Places API response types (internal)
#[derive(Debug, Deserialize)]
struct GoogleAutocompleteResponse {
    predictions: Vec<GooglePrediction>,
    status: String,
    error_message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GooglePrediction {
    place_id: String,
    description: String,
    structured_formatting: GoogleStructuredFormatting,
}

#[derive(Debug, Deserialize)]
struct GoogleStructuredFormatting {
    main_text: String,
    secondary_text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GooglePlaceDetailsResponse {
    result: Option<GooglePlaceResult>,
    status: String,
    error_message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GooglePlaceResult {
    place_id: String,
    formatted_address: String,
    geometry: GoogleGeometry,
}

#[derive(Debug, Deserialize)]
struct GoogleGeometry {
    location: GoogleLatLng,
}

#[derive(Debug, Deserialize)]
struct GoogleLatLng {
    lat: f64,
    lng: f64,
}

/// Get autocomplete predictions for a query
pub async fn autocomplete(request: AutocompleteRequest) -> Result<AutocompleteResponse> {
    let api_key = std::env::var("GOOGLE_API_KEY").map_err(|_| {
        Error::Configuration("GOOGLE_API_KEY environment variable not set".into())
    })?;

    if request.query.trim().is_empty() {
        return Ok(AutocompleteResponse {
            predictions: vec![],
        });
    }

    let client = reqwest::Client::new();
    let mut url = format!(
        "https://maps.googleapis.com/maps/api/place/autocomplete/json?input={}&types=address&key={}",
        urlencoding::encode(&request.query),
        api_key
    );

    if let Some(token) = &request.session_token {
        url.push_str(&format!("&sessiontoken={}", urlencoding::encode(token)));
    }

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| Error::ExternalApi(format!("Google Places API request failed: {}", e)))?;

    let google_response: GoogleAutocompleteResponse = response
        .json()
        .await
        .map_err(|e| Error::ExternalApi(format!("Failed to parse Google Places response: {}", e)))?;

    if google_response.status != "OK" && google_response.status != "ZERO_RESULTS" {
        return Err(Error::ExternalApi(format!(
            "Google Places API error: {} - {}",
            google_response.status,
            google_response.error_message.unwrap_or_default()
        )));
    }

    let predictions = google_response
        .predictions
        .into_iter()
        .map(|p| AutocompletePrediction {
            place_id: p.place_id,
            description: p.description,
            main_text: p.structured_formatting.main_text,
            secondary_text: p.structured_formatting.secondary_text.unwrap_or_default(),
        })
        .collect();

    Ok(AutocompleteResponse { predictions })
}

/// Get details for a specific place
pub async fn get_place_details(request: PlaceDetailsRequest) -> Result<PlaceDetailsResponse> {
    let api_key = std::env::var("GOOGLE_API_KEY").map_err(|_| {
        Error::Configuration("GOOGLE_API_KEY environment variable not set".into())
    })?;

    let client = reqwest::Client::new();
    let mut url = format!(
        "https://maps.googleapis.com/maps/api/place/details/json?place_id={}&fields=place_id,formatted_address,geometry&key={}",
        urlencoding::encode(&request.place_id),
        api_key
    );

    if let Some(token) = &request.session_token {
        url.push_str(&format!("&sessiontoken={}", urlencoding::encode(token)));
    }

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| Error::ExternalApi(format!("Google Places API request failed: {}", e)))?;

    let google_response: GooglePlaceDetailsResponse = response
        .json()
        .await
        .map_err(|e| Error::ExternalApi(format!("Failed to parse Google Places response: {}", e)))?;

    if google_response.status != "OK" {
        return Err(Error::ExternalApi(format!(
            "Google Places API error: {} - {}",
            google_response.status,
            google_response.error_message.unwrap_or_default()
        )));
    }

    let result = google_response
        .result
        .ok_or_else(|| Error::ExternalApi("No place result returned".into()))?;

    Ok(PlaceDetailsResponse {
        place_id: result.place_id,
        formatted_address: result.formatted_address,
        latitude: result.geometry.location.lat,
        longitude: result.geometry.location.lng,
    })
}

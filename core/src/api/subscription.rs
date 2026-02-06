//! Subscription API - Proxies to Tollbooth for subscription status and billing portal
//!
//! Core acts as a proxy between the frontend and Tollbooth for subscription-related
//! endpoints. This keeps Tollbooth's internal API hidden from the frontend.
//!
//! Routes (registered in server/mod.rs):
//! - GET /api/subscription → Tollbooth GET /v1/subscription
//! - POST /api/billing/portal → Tollbooth POST /v1/billing/portal

use crate::error::{Error, Result};
use std::sync::OnceLock;

/// Shared HTTP client for subscription requests (connection pool reuse)
static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn http_client() -> &'static reqwest::Client {
    HTTP_CLIENT.get_or_init(reqwest::Client::new)
}

/// Get subscription status from Tollbooth for the given user
///
/// Returns: { status, trial_expires_at, days_remaining, is_active }
pub async fn get_subscription_status(user_id: &str) -> Result<serde_json::Value> {
    let tollbooth_url = std::env::var("TOLLBOOTH_URL")
        .unwrap_or_else(|_| "http://localhost:9002".to_string());
    let secret = std::env::var("TOLLBOOTH_INTERNAL_SECRET")
        .map_err(|_| Error::Configuration("TOLLBOOTH_INTERNAL_SECRET not set".to_string()))?;

    let resp = crate::tollbooth::with_tollbooth_auth(
        http_client().get(format!("{}/v1/subscription", tollbooth_url)),
        user_id,
        &secret,
    )
    .send()
    .await
    .map_err(|e| Error::Network(format!("Tollbooth request failed: {}", e)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(Error::Network(format!(
            "Tollbooth subscription error ({}): {}",
            status, body
        )));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| Error::Network(format!("Failed to parse Tollbooth response: {}", e)))?;

    Ok(body)
}

/// Create a Stripe billing portal session via Tollbooth → Atlas
///
/// Returns: { url: "https://billing.stripe.com/session/..." }
pub async fn create_billing_portal(user_id: &str) -> Result<serde_json::Value> {
    let tollbooth_url = std::env::var("TOLLBOOTH_URL")
        .unwrap_or_else(|_| "http://localhost:9002".to_string());
    let secret = std::env::var("TOLLBOOTH_INTERNAL_SECRET")
        .map_err(|_| Error::Configuration("TOLLBOOTH_INTERNAL_SECRET not set".to_string()))?;

    let resp = crate::tollbooth::with_tollbooth_auth(
        http_client().post(format!("{}/v1/billing/portal", tollbooth_url)),
        user_id,
        &secret,
    )
    .send()
    .await
    .map_err(|e| Error::Network(format!("Tollbooth request failed: {}", e)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(Error::Network(format!(
            "Billing portal error ({}): {}",
            status, body
        )));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| Error::Network(format!("Failed to parse billing portal response: {}", e)))?;

    Ok(body)
}

//! Header-based Authentication for Internal Requests
//!
//! Core backend authenticates to virtues-api using shared secret headers.
//! This is NOT for end-user authentication - it's for internal service communication.
//!
//! Headers:
//!   X-Internal-Secret: <shared_secret>  (required)
//!
//! Security model:
//! - Network isolation ensures only Core can reach virtues-api (host sidecar)
//! - Shared secret validates request origin

use axum::{
    extract::FromRequestParts,
    http::request::Parts,
};
use std::sync::Arc;

use crate::AppState;

/// Header names for internal authentication
pub const INTERNAL_SECRET_HEADER: &str = "x-internal-secret";

/// Authenticated request — a validated shared-secret gate (carries no payload).
#[derive(Debug, Clone)]
pub struct AuthenticatedRequest;

/// Error type for authentication failures
#[derive(Debug)]
pub enum AuthError {
    MissingSecret,
    InvalidSecret,
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::MissingSecret => write!(f, "Missing X-Internal-Secret header"),
            AuthError::InvalidSecret => write!(f, "Invalid internal secret"),
        }
    }
}

impl virtues_helpers::error::StructuredError for AuthError {
    fn status(&self) -> u16 {
        401
    }
    fn code(&self) -> &str {
        match self {
            AuthError::MissingSecret => "missing_secret",
            AuthError::InvalidSecret => "invalid_secret",
        }
    }
    fn message(&self) -> String {
        self.to_string()
    }
    fn extra(&self) -> serde_json::Value {
        let hint = match self {
            AuthError::MissingSecret => "Include 'X-Internal-Secret' header with shared secret",
            AuthError::InvalidSecret => {
                "Check VIRTUES_API_INTERNAL_SECRET matches between Core and virtues-api"
            }
        };
        serde_json::json!({ "type": "authentication_error", "hint": hint })
    }
}
virtues_helpers::impl_into_response!(AuthError);

/// Axum extractor for authenticated requests
///
/// Validates X-Internal-Secret header against config.
/// Validates the X-Internal-Secret header; carries no payload.
#[axum::async_trait]
impl FromRequestParts<Arc<AppState>> for AuthenticatedRequest {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        // Extract X-Internal-Secret header
        let secret = parts
            .headers
            .get(INTERNAL_SECRET_HEADER)
            .and_then(|v| v.to_str().ok())
            .ok_or(AuthError::MissingSecret)?;

        // Validate secret using constant-time comparison
        if !constant_time_eq(secret.as_bytes(), state.config.internal_secret.as_bytes()) {
            tracing::warn!("Invalid internal secret - check VIRTUES_API_INTERNAL_SECRET");
            return Err(AuthError::InvalidSecret);
        }

        Ok(AuthenticatedRequest)
    }
}

/// Constant-time comparison to prevent timing attacks
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constant_time_eq() {
        assert!(constant_time_eq(b"secret123", b"secret123"));
        assert!(!constant_time_eq(b"secret123", b"secret456"));
        assert!(!constant_time_eq(b"short", b"longer_string"));
    }
}

//! Header-based Authentication for Internal Requests
//!
//! Core backend authenticates to Tollbooth using shared secret headers.
//! This is NOT for end-user authentication - it's for internal service communication.
//!
//! Headers:
//!   X-Internal-Secret: <shared_secret>  (required)
//!   X-User-Id: <user_id>                (optional, defaults to "system")
//!
//! Security model:
//! - Network isolation ensures only Core can reach Tollbooth (host sidecar)
//! - Shared secret validates request origin
//! - User ID tracks budget usage

use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
};
use std::sync::Arc;

use crate::AppState;

/// Header names for internal authentication
pub const INTERNAL_SECRET_HEADER: &str = "x-internal-secret";
pub const USER_ID_HEADER: &str = "x-user-id";

/// Default user ID for system/background operations
pub const SYSTEM_USER_ID: &str = "system";

/// Authenticated request with validated credentials
#[derive(Debug, Clone)]
pub struct AuthenticatedRequest {
    pub user_id: String,
}

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

impl axum::response::IntoResponse for AuthError {
    fn into_response(self) -> axum::response::Response {
        let (code, hint) = match &self {
            AuthError::MissingSecret => (
                "missing_secret",
                "Include 'X-Internal-Secret' header with shared secret",
            ),
            AuthError::InvalidSecret => (
                "invalid_secret",
                "Check TOLLBOOTH_INTERNAL_SECRET matches between Core and Tollbooth",
            ),
        };

        let body = serde_json::json!({
            "error": {
                "message": self.to_string(),
                "type": "authentication_error",
                "code": code,
                "hint": hint
            }
        });

        (StatusCode::UNAUTHORIZED, axum::Json(body)).into_response()
    }
}

/// Axum extractor for authenticated requests
///
/// Validates X-Internal-Secret header against config.
/// Extracts X-User-Id header (defaults to "system" if not provided).
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
            tracing::warn!("Invalid internal secret - check TOLLBOOTH_INTERNAL_SECRET");
            return Err(AuthError::InvalidSecret);
        }

        // Extract X-User-Id header (optional, defaults to "system")
        let user_id = parts
            .headers
            .get(USER_ID_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| SYSTEM_USER_ID.to_string());

        Ok(AuthenticatedRequest { user_id })
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

//! Tollbooth Client - Header-Based Authentication for Budget-Enforced API Proxy
//!
//! Provides helpers for authenticating Core backend requests to Tollbooth.
//! Tollbooth uses headers to identify users and track their API usage.
//!
//! Headers:
//!   X-Internal-Secret: <shared_secret>  (required)
//!   X-User-Id: <user_id>                (optional, defaults to "system")
//!
//! IMPORTANT: Header names must stay in sync with apps/tollbooth/src/auth.rs

/// Header name for internal secret authentication
pub const INTERNAL_SECRET_HEADER: &str = "X-Internal-Secret";

/// Header name for user ID (for budget tracking)
pub const USER_ID_HEADER: &str = "X-User-Id";

/// Minimum secret length for security (256 bits = 32 bytes)
/// Must match apps/tollbooth/src/config.rs MIN_SECRET_LENGTH
pub const MIN_SECRET_LENGTH: usize = 32;

/// System user ID for background jobs (no specific user context)
/// This ID should have a dedicated budget in Tollbooth for background processing
pub const SYSTEM_USER_ID: &str = "system";

/// Validate that the secret meets minimum length requirements
pub fn validate_secret(secret: &str) -> crate::Result<()> {
    if secret.len() < MIN_SECRET_LENGTH {
        return Err(crate::Error::Configuration(format!(
            "TOLLBOOTH_INTERNAL_SECRET must be at least {} characters (got {})",
            MIN_SECRET_LENGTH,
            secret.len()
        )));
    }
    Ok(())
}

/// Add Tollbooth authentication headers to a request builder
///
/// # Arguments
/// * `builder` - The reqwest RequestBuilder to add headers to
/// * `user_id` - UUID of the user whose budget should be charged
/// * `secret` - Shared secret between Core and Tollbooth (TOLLBOOTH_INTERNAL_SECRET)
///
/// # Returns
/// * The RequestBuilder with authentication headers added
pub fn with_tollbooth_auth(
    builder: reqwest::RequestBuilder,
    user_id: &str,
    secret: &str,
) -> reqwest::RequestBuilder {
    builder
        .header(INTERNAL_SECRET_HEADER, secret)
        .header(USER_ID_HEADER, user_id)
}

/// Add Tollbooth authentication headers for system/background requests
///
/// Uses "system" as the user ID, which should have a dedicated budget in Tollbooth
/// for background processing tasks.
pub fn with_system_auth(builder: reqwest::RequestBuilder, secret: &str) -> reqwest::RequestBuilder {
    with_tollbooth_auth(builder, SYSTEM_USER_ID, secret)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test secret that meets minimum length requirement (32 chars)
    const TEST_SECRET: &str = "this-is-a-test-secret-32-chars!";

    #[test]
    fn test_validate_secret() {
        assert!(validate_secret(TEST_SECRET).is_ok());
    }

    #[test]
    fn test_secret_too_short() {
        let short_secret = "too-short";
        let result = validate_secret(short_secret);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("at least 32 characters"));
    }

    #[test]
    fn test_header_constants() {
        assert_eq!(INTERNAL_SECRET_HEADER, "X-Internal-Secret");
        assert_eq!(USER_ID_HEADER, "X-User-Id");
        assert_eq!(SYSTEM_USER_ID, "system");
    }
}

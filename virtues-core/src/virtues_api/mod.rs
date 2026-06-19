//! virtues-api client.
//!
//! Auth is **bearer + voucher** (`renew` submodule): the home server holds a
//! `billing_token` (stable, ≈OAuth refresh token) and a monthly `bearer`
//! (≈access token), both in the credential vault. `renew` runs the voucher
//! dance (Atlas `/voucher` → virtues-api `/v1/redeem`) to mint a fresh bearer.
//! The customer↔bearer link lives only here, on the box.

pub mod client;
pub mod link;
pub mod renew;

/// Minimum secret length for security (256 bits = 32 bytes)
/// Must match services/virtues-api/src/config.rs MIN_SECRET_LENGTH
pub const MIN_SECRET_LENGTH: usize = 32;

/// Validate that the secret meets minimum length requirements
pub fn validate_secret(secret: &str) -> crate::Result<()> {
    if secret.len() < MIN_SECRET_LENGTH {
        return Err(crate::Error::Configuration(format!(
            "VIRTUES_API_INTERNAL_SECRET must be at least {} characters (got {})",
            MIN_SECRET_LENGTH,
            secret.len()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test secret that meets minimum length requirement (32 chars)
    const TEST_SECRET: &str = "this-is-a-test-secret-32-chars!!";

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
}

//! virtues-api client.
//!
//! Auth is a single rotatable device **api_key** (`renew` submodule, kept for
//! its name): the box holds the key in the credential vault and sends it on
//! every proxy call. atlas mints it at link, registers it with virtues-api, and
//! credits the wallet server-side (renewal via Stripe webhook, top-ups via
//! card). No vouchers, no bearer rotation, no client-side renewal.

pub mod client;
pub mod completion;
pub mod link;
pub mod relay;
pub mod renew;

/// Default cloud endpoints. A real box always has `VIRTUES_API_URL` /
/// `VIRTUES_ATLAS_URL` set (the installer writes them into `virtues.env`;
/// `make dev` exports them), so these fallbacks only apply to a
/// misconfigured/raw invocation — in which case **prod** is the safe default:
/// a stray request just 401s without a valid bearer, whereas the old split
/// fallback (BearerClient → `localhost:9002`, main/diag → prod) meant a box
/// missing the env var would silently bill against localhost in one path and
/// prod in another. Single source of truth for both now.
pub const DEFAULT_API_URL: &str = "https://api.virtues.com";
pub const DEFAULT_ATLAS_URL: &str = "https://atlas.virtues.com";

/// The cloud `virtues-api` base URL: `VIRTUES_API_URL`, else the prod default.
pub fn api_url() -> String {
    std::env::var("VIRTUES_API_URL").unwrap_or_else(|_| DEFAULT_API_URL.to_string())
}

/// The atlas base URL: `VIRTUES_ATLAS_URL`, else the prod default.
pub fn atlas_url() -> String {
    std::env::var("VIRTUES_ATLAS_URL").unwrap_or_else(|_| DEFAULT_ATLAS_URL.to_string())
}

/// True when this box is talking to a non-prod cloud (dev or staging) — drives
/// the "⚠ staging environment" banner. Prefers the explicit `ENVIRONMENT=dev`
/// marker; falls back to sniffing the atlas URL so a box *manually* pointed at
/// staging is still flagged (the installer currently always writes
/// `ENVIRONMENT=production`, so a marker-only check can't see manual staging
/// until an install-time `--env staging` writer lands).
pub fn is_nonprod_cloud() -> bool {
    if std::env::var("ENVIRONMENT").map(|v| v == "dev").unwrap_or(false) {
        return true;
    }
    let a = atlas_url();
    a.contains("staging") || a.contains("localhost")
}

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

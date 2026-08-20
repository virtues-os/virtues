//! Shared HTTP request/response helpers used by handlers in `api::*`.
//!
//! Lives here (under `middleware/`) rather than in `crate::http_client`
//! (which owns the outbound HTTP client) so it sits next to other
//! request-scoped helpers like the auth extractor.

use axum::http::HeaderMap;

/// Singleton owner id. The same UUID is also used as the primary key on
/// `app_user_profile` (see `migrations/0003_app_shell.sql`) and is seeded
/// into `app_auth_user` by `migrations/0002_auth.sql`. v1 is single-tenant;
/// every device FKs to this row.
pub const OWNER_USER_ID: &str = "00000000-0000-0000-0000-000000000001";

/// Extract the originating client IP from `X-Forwarded-For`, taking the
/// left-most value. Returns `None` if the header isn't set — we don't
/// guess from the socket peer since the box almost always sits behind a
/// reverse proxy (Caddy / the box's own HTTPS sidecar) and the socket peer
/// would be loopback.
pub fn client_ip(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// A spoof-resistant client IP for *security decisions* (e.g. rate limiting) —
/// but ONLY when we actually sit behind a trusted reverse proxy.
///
/// `X-Forwarded-For` is meaningful only if some hop we trust appended it. On a
/// stock box there is NO proxy (the app answers `:8000` directly — verified), so
/// any `X-Forwarded-For` present is entirely attacker-supplied, and trusting its
/// right-most entry let a LAN client mint a fresh rate-limit bucket per request
/// and brute-force the pair code at full speed. So this reads the header only
/// when `VIRTUES_TRUSTED_PROXY` is set (the cloud/atlas deployment, which really
/// does sit behind Caddy); otherwise it returns `None` and the caller keys on
/// the actual socket peer instead. (Setup-runtime audit, 2026-08-19.)
///
/// When trusted, takes the right-most entry — a remote client can prepend
/// arbitrary hops but cannot stop the proxy from appending the real peer last.
pub fn rate_limit_ip(headers: &HeaderMap) -> Option<String> {
    if !trusted_proxy_configured() {
        return None;
    }
    rightmost_forwarded(headers)
}

/// The right-most `X-Forwarded-For` entry, or `None`. Pure — the trust decision
/// lives in `rate_limit_ip`.
fn rightmost_forwarded(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.rsplit(',').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Whether a trusted reverse proxy sits in front of this process, so that
/// `X-Forwarded-For` can be believed. Off by default — a box has no proxy.
fn trusted_proxy_configured() -> bool {
    std::env::var("VIRTUES_TRUSTED_PROXY")
        .map(|v| matches!(v.trim(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

/// True when the deployment is "secure" — meaning we should issue cookies
/// with the `Secure` attribute, use `__Secure-` cookie name prefixes, and
/// default URL minting to `https://`. False when `ENVIRONMENT` is `dev`,
/// `development`, `local`, or `test` (any plain-HTTP loop).
pub fn is_secure_environment() -> bool {
    std::env::var("ENVIRONMENT")
        .map(|v| !matches!(v.as_str(), "dev" | "development" | "local" | "test"))
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hm(xff: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", xff.parse().unwrap());
        h
    }

    #[test]
    fn client_ip_takes_leftmost() {
        assert_eq!(client_ip(&hm("1.1.1.1, 2.2.2.2, 3.3.3.3")).as_deref(), Some("1.1.1.1"));
    }

    #[test]
    fn rightmost_forwarded_takes_the_proxy_appended_value() {
        // A spoofed left value is ignored; the proxy-appended right wins — the
        // parsing that `rate_limit_ip` uses ONLY when a trusted proxy is set.
        assert_eq!(
            rightmost_forwarded(&hm("evil-spoof, 2.2.2.2, 9.9.9.9")).as_deref(),
            Some("9.9.9.9")
        );
        assert_eq!(rightmost_forwarded(&HeaderMap::new()), None);
    }

    #[test]
    fn rate_limit_ip_ignores_xff_without_a_trusted_proxy() {
        // Default (no VIRTUES_TRUSTED_PROXY): a client-supplied XFF must NOT be
        // trusted, so the caller falls back to the real socket peer. The test
        // suite never sets the env var, so this is the box's real posture.
        assert_eq!(rate_limit_ip(&hm("1.2.3.4")), None);
        assert_eq!(rate_limit_ip(&HeaderMap::new()), None);
    }
}

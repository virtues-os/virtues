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

/// A spoof-resistant client IP for *security decisions* (e.g. rate limiting).
///
/// Unlike [`client_ip`], this takes the **right-most** `X-Forwarded-For`
/// entry — the value appended by our OWN trusted reverse proxy (Caddy / the
/// box's HTTPS sidecar). A remote client can prepend arbitrary entries, but it
/// cannot stop the proxy from appending the real peer at the end, so the
/// right-most hop is the one we can trust.
///
/// Returns `None` when there's no `X-Forwarded-For` at all — meaning the
/// request did not transit our proxy (direct loopback / dev). Such requests
/// are only reachable by something already on the box, so callers should treat
/// `None` as "trusted, do not rate-limit" rather than collapsing every
/// header-less caller into one shared bucket (which would be a trivial DoS).
pub fn rate_limit_ip(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.rsplit(',').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
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
    fn rate_limit_ip_takes_rightmost_proxy_appended() {
        // A spoofed left value is ignored; the proxy-appended right wins.
        assert_eq!(
            rate_limit_ip(&hm("evil-spoof, 2.2.2.2, 9.9.9.9")).as_deref(),
            Some("9.9.9.9")
        );
    }

    #[test]
    fn rate_limit_ip_none_without_header() {
        assert_eq!(rate_limit_ip(&HeaderMap::new()), None);
    }
}

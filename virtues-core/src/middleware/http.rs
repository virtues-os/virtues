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

/// True when the deployment is "secure" — meaning we should issue cookies
/// with the `Secure` attribute, use `__Secure-` cookie name prefixes, and
/// default URL minting to `https://`. False when `ENVIRONMENT` is `dev`,
/// `development`, `local`, or `test` (any plain-HTTP loop).
pub fn is_secure_environment() -> bool {
    std::env::var("ENVIRONMENT")
        .map(|v| !matches!(v.as_str(), "dev" | "development" | "local" | "test"))
        .unwrap_or(true)
}

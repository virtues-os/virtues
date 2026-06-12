//! Defensive HTTP headers + CSRF for the pair-only cookie auth.
//!
//! Two layers, applied as `axum::middleware::from_fn`:
//!
//! 1. `headers_layer` — sets `X-Frame-Options: DENY`, `X-Content-Type-Options:
//!    nosniff`, and a conservative `Content-Security-Policy` on every response.
//!    Defeats clickjacking of `/virtues/devices` (an Add-Device modal in an
//!    iframe could otherwise be read by the parent page) and mitigates a
//!    compromised browser extension's blast radius.
//!
//! 2. `csrf_layer` — enforces a double-submit cookie on every state-changing
//!    request that arrives with a session cookie:
//!      - Issues a `virtues.csrf-token` cookie (Secure when applicable, NOT
//!        HttpOnly — JS must read it to forward).
//!      - Refuses non-GET/HEAD/OPTIONS requests whose `X-CSRF-Token` header
//!        doesn't match the cookie value AND that arrived with a session
//!        cookie. Unauth requests bypass (the pair-token consume + signout
//!        endpoints are CSRF-safe by construction: they either have nothing
//!        to forge against, or they consume a one-time token that's
//!        gated separately).
//!
//! The "with a session cookie" guard means the pair consume endpoint
//! (POST /api/pair/consume, anonymous) is exempt — the request body's pair
//! token is the capability; there's no ambient authority for CSRF to abuse.

use axum::{
    extract::Request,
    http::{header, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use rand::RngCore;
use serde_json::json;

use crate::middleware::auth::{SESSION_COOKIE_NAME, SESSION_COOKIE_NAME_SECURE};

/// Cookie name for the CSRF token (NOT HttpOnly — JS must read it).
const CSRF_COOKIE_NAME: &str = "virtues.csrf-token";
const CSRF_COOKIE_NAME_SECURE: &str = "__Host-virtues.csrf-token";
const CSRF_HEADER: &str = "x-csrf-token";

// ─── Security headers ───────────────────────────────────────────────────────

pub async fn headers_layer(req: Request, next: Next) -> Response {
    let mut resp = next.run(req).await;
    let h = resp.headers_mut();
    // Clickjacking — the box's UI must never render inside someone else's
    // frame. Add-Device modal carries a one-time token and we don't want
    // a parent page reading it via cross-frame DOM tricks.
    h.insert(
        "x-frame-options",
        HeaderValue::from_static("DENY"),
    );
    // MIME-sniffing defense for any served file content.
    h.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    // Conservative CSP — same-origin only. SvelteKit's dev server inlines
    // styles, so we allow 'unsafe-inline' for style-src; script-src stays
    // strict (no inline, no eval). The web UI doesn't load third-party
    // scripts in production. `connect-src` is the one operator-tunable axis
    // (a BYO api/atlas on a custom domain) — see [`csp_header`].
    h.insert("content-security-policy", csp_header().clone());
    // Don't leak the full URL on outbound nav.
    h.insert(
        "referrer-policy",
        HeaderValue::from_static("same-origin"),
    );
    resp
}

/// The `Content-Security-Policy` header value, built once on first use and
/// cached. Everything is same-origin-locked except `connect-src`, which defaults
/// to same-origin plus the Virtues cloud endpoints (api/atlas) and ws/wss for
/// live updates. A BYO deployment that fronts its own api/atlas on a custom
/// domain appends space-separated origins via `VIRTUES_CSP_CONNECT_SRC_EXTRA`
/// so the browser doesn't block fetch/XHR/websocket to them. Malformed operator
/// input falls back to the locked-down default (never to a weaker policy).
fn csp_header() -> &'static HeaderValue {
    static CSP: std::sync::OnceLock<HeaderValue> = std::sync::OnceLock::new();
    const DEFAULT_CONNECT_SRC: &str =
        "'self' https://api.virtues.com https://atlas.virtues.com ws: wss:";
    CSP.get_or_init(|| {
        let mut connect_src = String::from(DEFAULT_CONNECT_SRC);
        if let Ok(extra) = std::env::var("VIRTUES_CSP_CONNECT_SRC_EXTRA") {
            let extra = extra.trim();
            if !extra.is_empty() {
                connect_src.push(' ');
                connect_src.push_str(extra);
            }
        }
        let csp = format!(
            "default-src 'self'; \
             script-src 'self'; \
             style-src 'self' 'unsafe-inline'; \
             img-src 'self' data: blob:; \
             connect-src {connect_src}; \
             frame-ancestors 'none'; \
             base-uri 'self'; \
             form-action 'self'"
        );
        HeaderValue::from_str(&csp).unwrap_or_else(|_| {
            // A bad VIRTUES_CSP_CONNECT_SRC_EXTRA (illegal header bytes) must not
            // weaken or drop the policy — fall back to the built-in default.
            tracing::warn!(
                "VIRTUES_CSP_CONNECT_SRC_EXTRA produced an invalid CSP header; \
                 ignoring it and using the default connect-src"
            );
            HeaderValue::from_static(
                "default-src 'self'; script-src 'self'; \
                 style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; \
                 connect-src 'self' https://api.virtues.com https://atlas.virtues.com ws: wss:; \
                 frame-ancestors 'none'; base-uri 'self'; form-action 'self'",
            )
        })
    })
}

/// Best-effort startup advisory: warn when the box is advertised at a plain-HTTP
/// origin on a non-local host. The session + CSRF cookies are designed for one
/// of two safe transports — the box's own localhost (a W3C Secure Context, no
/// TLS required) or a WireGuard tunnel (encryption + authentication). A BYO
/// operator who instead points browsers straight at `http://<public-ip>:8000`
/// gets neither:
///   - in a secure `ENVIRONMENT` the cookies carry `Secure`, so the browser
///     refuses them over plain HTTP and login silently fails;
///   - in a dev `ENVIRONMENT` they're sent without `Secure`, riding the network
///     in cleartext (session-hijack risk).
/// Either way the fix is the same: put TLS in front (e.g. Caddy), or reach the
/// box via localhost / the WG tunnel. Advisory only — never blocks startup.
pub fn warn_insecure_cookie_origin() {
    let url = std::env::var("PUBLIC_API_URL")
        .or_else(|_| std::env::var("BACKEND_URL"))
        .unwrap_or_default();
    let host = match insecure_cookie_origin_host(&url) {
        Some(h) => h,
        None => return, // https, empty, or a safe local plain-HTTP context
    };

    if is_secure_environment() {
        tracing::warn!(
            origin = %url, host = %host,
            "this URL is plain HTTP on a non-local host: browsers will REJECT the \
             Secure session cookie over http:// and login will fail. Put TLS in \
             front (e.g. Caddy), or reach the box via localhost / the WireGuard \
             tunnel."
        );
    } else {
        tracing::warn!(
            origin = %url, host = %host,
            "this URL is plain HTTP on a non-local host: session cookies will \
             travel UNENCRYPTED (session-hijack risk). Put TLS in front (e.g. \
             Caddy), or reach the box via localhost / the WireGuard tunnel."
        );
    }
}

/// Classify a configured external URL for [`warn_insecure_cookie_origin`].
/// Returns `Some(host)` when the URL is a cookie-security concern — plain HTTP
/// to a non-local host — and `None` when it's fine (HTTPS, empty, or a safe
/// plain-HTTP context: loopback or an mDNS `.local` LAN name). Pure: no env, no
/// I/O, so the edge cases (bracketed IPv6, port/path stripping) are unit-tested.
fn insecure_cookie_origin_host(url: &str) -> Option<String> {
    let url = url.trim();
    // Only plain HTTP is a concern; HTTPS terminates transport security upstream.
    let rest = url.strip_prefix("http://")?;
    // Host = everything before the port (':') or path ('/'). Handle a bracketed
    // IPv6 literal ([::1]) by reading to the closing ']'.
    let host = if let Some(after) = rest.strip_prefix('[') {
        after.split(']').next().unwrap_or("")
    } else {
        rest.split([':', '/']).next().unwrap_or("")
    };
    let host = host.to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    // Safe plain-HTTP contexts: the box's own loopback (a Secure Context) and
    // mDNS `.local` names (LAN-only — the normal appliance/dev case).
    let is_local = matches!(host.as_str(), "localhost" | "::1" | "0.0.0.0")
        || host.starts_with("127.")
        || host.ends_with(".local");
    if is_local {
        None
    } else {
        Some(host)
    }
}

// ─── CSRF (double-submit cookie) ────────────────────────────────────────────

pub async fn csrf_layer(req: Request, next: Next) -> Response {
    let jar = CookieJar::from_headers(req.headers());
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    // Read the existing CSRF cookie if present.
    let existing_csrf = jar
        .get(CSRF_COOKIE_NAME_SECURE)
        .or_else(|| jar.get(CSRF_COOKIE_NAME))
        .map(|c| c.value().to_string());

    // Whether the request arrived with ambient session authority.
    let has_session = jar
        .get(SESSION_COOKIE_NAME_SECURE)
        .or_else(|| jar.get(SESSION_COOKIE_NAME))
        .is_some();

    let is_state_change = !matches!(
        method,
        Method::GET | Method::HEAD | Method::OPTIONS
    );
    let exempt = path == "/api/pair/consume"  // anonymous, body-token = capability
        || path == "/auth/signout"            // idempotent + only kills your own session
        || path.starts_with("/webhook/")      // bearer auth, no cookie ambient
        || path.starts_with("/internal/")     // shared-secret header auth
        || path.starts_with("/oauth/callback"); // signed-state callback

    if is_state_change && has_session && !exempt {
        let header_token = req
            .headers()
            .get(CSRF_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let valid = match (existing_csrf.as_deref(), header_token.as_deref()) {
            (Some(cookie_tok), Some(header_tok)) => constant_eq(cookie_tok, header_tok),
            _ => false,
        };
        if !valid {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({"error": "csrf_mismatch"})),
            )
                .into_response();
        }
    }

    let mut resp = next.run(req).await;

    // If we don't have a CSRF cookie yet, mint one. Pair-only auth issues
    // session cookies via `/api/pair/consume`; the CSRF cookie is companion
    // to the session and rides alongside on every subsequent response so
    // a freshly-paired browser has both ready before its first state-change.
    if existing_csrf.is_none() && has_session {
        let token = generate_csrf_token();
        let is_secure = is_secure_environment();
        let name = if is_secure {
            CSRF_COOKIE_NAME_SECURE
        } else {
            CSRF_COOKIE_NAME
        };
        let cookie = Cookie::build((name, token))
            .path("/")
            .secure(is_secure)
            .http_only(false) // JS must read
            .same_site(SameSite::Lax)
            .max_age(time::Duration::days(30))
            .build();
        if let Ok(set_cookie) = HeaderValue::from_str(&cookie.to_string()) {
            resp.headers_mut().append(header::SET_COOKIE, set_cookie);
        }
    }

    resp
}

fn constant_eq(a: &str, b: &str) -> bool {
    let aa = a.as_bytes();
    let bb = b.as_bytes();
    if aa.len() != bb.len() {
        return false;
    }
    let mut diff = 0u8;
    for i in 0..aa.len() {
        diff |= aa[i] ^ bb[i];
    }
    diff == 0
}

fn generate_csrf_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

use super::http::is_secure_environment;

#[cfg(test)]
mod tests {
    use super::insecure_cookie_origin_host;

    #[test]
    fn https_is_never_a_concern() {
        assert_eq!(insecure_cookie_origin_host("https://box.example.com"), None);
        assert_eq!(insecure_cookie_origin_host("https://1.2.3.4:8000"), None);
    }

    #[test]
    fn empty_or_non_http_is_ignored() {
        assert_eq!(insecure_cookie_origin_host(""), None);
        assert_eq!(insecure_cookie_origin_host("   "), None);
        assert_eq!(insecure_cookie_origin_host("ftp://1.2.3.4"), None);
        assert_eq!(insecure_cookie_origin_host("http://"), None);
    }

    #[test]
    fn local_plain_http_is_safe() {
        for u in [
            "http://localhost:8000",
            "http://127.0.0.1:8000",
            "http://127.5.5.5",
            "http://[::1]:8000",
            "http://0.0.0.0:8000",
            "http://my-box.local",
            "http://my-box.local:3000/path",
        ] {
            assert_eq!(insecure_cookie_origin_host(u), None, "{u} should be safe");
        }
    }

    #[test]
    fn public_plain_http_is_flagged() {
        assert_eq!(
            insecure_cookie_origin_host("http://203.0.113.7:8000"),
            Some("203.0.113.7".to_string())
        );
        assert_eq!(
            insecure_cookie_origin_host("http://box.example.com/path"),
            Some("box.example.com".to_string())
        );
        // Bracketed public IPv6 literal — host parsed without the brackets.
        assert_eq!(
            insecure_cookie_origin_host("http://[2001:db8::1]:8000"),
            Some("2001:db8::1".to_string())
        );
    }
}

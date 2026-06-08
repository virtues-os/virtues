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
    // scripts in production.
    h.insert(
        "content-security-policy",
        HeaderValue::from_static(
            "default-src 'self'; \
             script-src 'self'; \
             style-src 'self' 'unsafe-inline'; \
             img-src 'self' data: blob:; \
             connect-src 'self' https://api.virtues.com https://atlas.virtues.com ws: wss:; \
             frame-ancestors 'none'; \
             base-uri 'self'; \
             form-action 'self'",
        ),
    );
    // Don't leak the full URL on outbound nav.
    h.insert(
        "referrer-policy",
        HeaderValue::from_static("same-origin"),
    );
    resp
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

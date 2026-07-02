//! Defensive HTTP response headers.
//!
//! `headers_layer` (applied as `axum::middleware::from_fn`) sets
//! `X-Frame-Options: DENY`, `X-Content-Type-Options: nosniff`, a conservative
//! `Content-Security-Policy`, and `Referrer-Policy: same-origin` on every
//! response. Defeats clickjacking of the box UI and mitigates a compromised
//! browser extension's blast radius.
//!
//! There is no CSRF layer: auth is the device's proven iroh key (or on-box
//! loopback), not an ambient session cookie, so there is no cross-site cookie
//! authority for CSRF to abuse.

use axum::{
    extract::Request,
    http::HeaderValue,
    middleware::Next,
    response::Response,
};

// ─── Security headers ───────────────────────────────────────────────────────

pub async fn headers_layer(req: Request, next: Next) -> Response {
    let mut resp = next.run(req).await;
    let h = resp.headers_mut();
    // Clickjacking — the box's UI must never render inside someone else's frame.
    h.insert("x-frame-options", HeaderValue::from_static("DENY"));
    // MIME-sniffing defense for any served file content.
    h.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    // Conservative CSP — same-origin only. SvelteKit inlines styles, so style-src
    // allows 'unsafe-inline'; script-src stays strict. `connect-src` is the one
    // operator-tunable axis (a BYO api/atlas on a custom domain) — see csp_header.
    h.insert("content-security-policy", csp_header().clone());
    // Don't leak the full URL on outbound nav.
    h.insert("referrer-policy", HeaderValue::from_static("same-origin"));
    resp
}

/// The `Content-Security-Policy` header value, built once on first use and
/// cached. Everything is same-origin-locked except `connect-src`, which defaults
/// to same-origin plus the Virtues cloud endpoints (api/atlas) and ws/wss for
/// live updates. A BYO deployment appends space-separated origins via
/// `VIRTUES_CSP_CONNECT_SRC_EXTRA`. Malformed operator input falls back to the
/// locked-down default (never to a weaker policy).
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

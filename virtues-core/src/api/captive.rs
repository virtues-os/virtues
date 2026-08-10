//! Captive-portal detection — makes `/provision` open by itself.
//!
//! When a phone joins a wifi network, iOS, Android and Windows each fetch a
//! known URL and check for an exact response. Get it, and the network is
//! "connected". Get anything else, and the OS declares a captive portal and
//! **opens the page for you**. That auto-open is the entire reason this module
//! exists: without it, someone who has just scanned the QR and joined
//! `Virtues-XXXX` is sitting on a network with no internet and no instructions,
//! and the only way forward is to be told an IP address to type.
//!
//! So this is a small deception, on purpose, and only in the one window where
//! it is true: the box really is a network that cannot reach the internet, and
//! there really is something the owner must do before it can.
//!
//! **Two phases, and the second is the one everybody forgets.**
//!
//!   * Setup unfinished → answer the probe with something that is NOT the
//!     expected token. The OS shows the portal.
//!   * Box online → answer with EXACTLY what the OS wants. It clears the
//!     captive flag and stops asking.
//!
//! Skipping the second phase is what makes a captive portal feel broken: the
//! network never "settles", the OS keeps re-probing, and iOS will eventually
//! drop the association. Measured on the Dragon while testing a portal that had
//! only the first phase — 22 probes in 90 seconds from two devices, forever.
//!
//! This runs as middleware rather than as routes because Apple's second probe
//! host (`netcts.cdn-apple.com`) asks for `/`, which is the SPA's own route.
//! Only the `Host` header distinguishes them.

use axum::{
    body::Body,
    extract::Request,
    http::{header, HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

/// Hosts whose probes we answer. Matching the host — rather than just the path
/// — keeps this from firing on the box's own UI, which serves `/` too.
const PROBE_HOSTS: &[&str] = &[
    "captive.apple.com",
    "netcts.cdn-apple.com",
    "connectivitycheck.gstatic.com",
    "clients3.google.com",
    "connectivitycheck.android.com",
    "www.msftconnecttest.com",
    "msftconnecttest.com",
    "detectportal.firefox.com",
];

/// Apple wants exactly this document, byte for byte.
const APPLE_SUCCESS: &str =
    "<HTML><HEAD><TITLE>Success</TITLE></HEAD><BODY>Success</BODY></HTML>\n";

/// Windows wants exactly this body.
const MS_SUCCESS: &str = "Microsoft Connect Test";

/// Firefox wants this.
const FIREFOX_SUCCESS: &str = "success\n";

fn probe_host(headers: &HeaderMap) -> Option<String> {
    let host = headers.get(header::HOST)?.to_str().ok()?;
    // Strip any port before comparing; probes are plain :80 but be exact anyway.
    let host = host.split(':').next().unwrap_or(host).to_ascii_lowercase();
    PROBE_HOSTS
        .iter()
        .any(|h| *h == host)
        .then_some(host)
}

/// Middleware: intercept OS connectivity probes, pass everything else through.
pub async fn intercept(request: Request, next: Next) -> Response {
    let Some(host) = probe_host(request.headers()) else {
        return next.run(request).await;
    };

    // "Is the box on a real network yet?" is the honest form of "is setup
    // done". Once it is, the phone should stop being told it is captive — the
    // box is about to stop being the phone's network at all.
    if crate::cli::link::primary_ip().is_some() && !crate::maintenance::setup_ap::provisioning_in_flight() {
        return success_for(&host);
    }

    // Not online yet: send them to the page that fixes that. A 302 is what the
    // OS's own portal detector follows; the body is for anything that doesn't.
    //
    // **`/portal`, not `/provision`.** `/provision` is a SvelteKit route, and
    // the frontend is `adapter-static` with no server-side rendering — an empty
    // document until ES modules load and a client router boots. The browser
    // that receives this redirect is iOS's Captive Network Assistant, a
    // stripped-down WebKit in a sheet, and on hardware 2026-08-10 it rendered
    // that as a blank white page. Worse, because this middleware tells iOS the
    // network IS captive, the OS kept forcing the blank sheet back open and
    // would not let the owner reach Safari to work around it. Pointing a
    // captive redirect at a JS-only page builds a trap and then locks it.
    // `/portal` is plain server-rendered HTML with a real form. See
    // `api::portal`.
    (
        StatusCode::FOUND,
        [(header::LOCATION, "http://10.42.0.1:8000/portal")],
        "Connect your Virtues box: http://10.42.0.1:8000/portal\n",
    )
        .into_response()
}

/// The exact response each vendor treats as "this network is fine".
fn success_for(host: &str) -> Response {
    if host.contains("apple") {
        return (StatusCode::OK, [(header::CONTENT_TYPE, "text/html")], APPLE_SUCCESS)
            .into_response();
    }
    if host.contains("msftconnecttest") {
        return (StatusCode::OK, [(header::CONTENT_TYPE, "text/plain")], MS_SUCCESS)
            .into_response();
    }
    if host.contains("firefox") {
        return (StatusCode::OK, [(header::CONTENT_TYPE, "text/plain")], FIREFOX_SUCCESS)
            .into_response();
    }
    // Google/Android: an empty 204 is the whole contract.
    (StatusCode::NO_CONTENT, Body::empty()).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(host: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(header::HOST, HeaderValue::from_str(host).unwrap());
        h
    }

    #[test]
    fn recognises_each_vendor_probe() {
        for h in ["captive.apple.com", "connectivitycheck.gstatic.com", "www.msftconnecttest.com"] {
            assert!(probe_host(&headers(h)).is_some(), "missed {h}");
        }
    }

    #[test]
    fn ignores_the_boxs_own_hosts() {
        // The SPA serves `/` too — only the Host header separates it from
        // Apple's netcts probe, so this is the check that keeps the app from
        // being replaced by a portal redirect.
        for h in ["virtues.local", "10.42.0.1:8000", "localhost:8000", "evil.example.com"] {
            assert!(probe_host(&headers(h)).is_none(), "wrongly matched {h}");
        }
    }

    #[test]
    fn host_matching_ignores_port_and_case() {
        assert!(probe_host(&headers("Captive.Apple.Com:80")).is_some());
    }

    #[test]
    fn apple_success_is_byte_exact() {
        // iOS compares this document, not the status code. A stray edit here
        // makes every joined network look captive forever.
        assert!(APPLE_SUCCESS.contains("<TITLE>Success</TITLE>"));
        assert!(APPLE_SUCCESS.contains("<BODY>Success</BODY>"));
    }
}

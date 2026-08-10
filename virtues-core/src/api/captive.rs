//! Connectivity-probe handling on the setup AP — and the story of a reversal.
//!
//! When a phone joins a wifi network, iOS, Android and Windows each fetch a
//! known URL and check for an exact response. Get it, and the network is
//! "connected". Get anything else, and the OS declares a captive portal and
//! opens its Captive Network Assistant over the page.
//!
//! **This module used to exploit that on purpose** — answer "captive" while
//! setup was unfinished so the CNA would auto-open `/provision`. Hardware
//! (2026-08-10) killed the idea in three separate ways: the CNA is a
//! stripped-down WebKit that rendered our SPA as a blank sheet; the OS
//! force-reopens the sheet and refuses to let the user leave it; and the CNA
//! caches portal pages per-SSID *across box upgrades*, so a fixed box kept
//! showing a broken page. Every failure was on an OS surface we cannot patch.
//!
//! The deeper realization that made the reversal easy: **the captive portal
//! served a user who cannot exist.** Pairing requires an app holding an iroh
//! key, so an app-less user who provisioned wifi via a portal still could not
//! finish onboarding. The app drives setup now (it joins the AP itself via
//! `NEHotspotConfiguration` and runs the wifi picker natively); `/portal`
//! survives as an unadvertised manual hatch.
//!
//! So today the job is the opposite: **answer every probe with its vendor's
//! exact success token so the CNA never opens.** The OS concludes the network
//! is fine, keeps the association, and leaves our app alone. Consumer IoT
//! setup (Echo, Sonos) converged on the same suppression.
//!
//! Still middleware rather than routes, because Apple's second probe host
//! (`netcts.cdn-apple.com`) asks for `/`, which is the SPA's own route — only
//! the `Host` header distinguishes them. The byte-exactness of the success
//! bodies is what all of this rests on: get one wrong and every joined network
//! looks captive forever.

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

/// Middleware: answer OS connectivity probes with SUCCESS, always.
///
/// **This used to answer "captive" during setup, and that was the mistake.**
/// Telling iOS the network is captive summons the Captive Network Assistant —
/// a stripped-down WebKit sheet the OS force-reopens and will not let the user
/// leave. On hardware 2026-08-10 that sheet rendered our SPA as a blank white
/// page, then a *cached* three-hour-old page after we fixed it (the CNA caches
/// portal pages per-SSID, across box upgrades). Every failure was on an OS
/// surface we do not control.
///
/// The deeper realization: the captive portal served a user who cannot exist.
/// Pairing requires an app holding an iroh key, so an app-less user who
/// provisioned wifi through a portal still could not finish onboarding. The
/// app drives setup (`wifi_join` + the connect-screen flow); the portal at
/// `/portal` remains as an unadvertised manual hatch.
///
/// So: probes get the exact success token their vendor expects, the OS
/// concludes "this network is fine", and no sheet ever opens. The phone shows
/// "No Internet" on the wifi row at worst, keeps the association, and our app
/// works undisturbed. Suppressing the CNA this way is what consumer IoT setup
/// (Echo, Sonos) has converged on.
pub async fn intercept(request: Request, next: Next) -> Response {
    if let Some(host) = probe_host(request.headers()) {
        return success_for(&host);
    }

    // The manual hatch: someone told to "open 10.42.0.1 in a browser" lands on
    // the SPA fallback — a blank page in exactly the browsers that need the
    // hatch. Send bare-IP roots to the portal instead. Scoped to the setup
    // AP's own address so the box's real UI (via mDNS name, LAN IP, loopback)
    // is untouched.
    let is_hatch = request.uri().path() == "/"
        && request
            .headers()
            .get(header::HOST)
            .and_then(|h| h.to_str().ok())
            .map(|h| h.split(':').next().unwrap_or(h) == "10.42.0.1")
            .unwrap_or(false);
    if is_hatch {
        return (
            StatusCode::FOUND,
            [(header::LOCATION, "/portal")],
            "",
        )
            .into_response();
    }

    next.run(request).await
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

//! `GET /api/display/state` — everything the attached screen renders.
//!
//! The display is the 7" panel on the front of an appliance, driven by a WebKit
//! kiosk running *on the box* (see the installer's `virtues-display.service`).
//! It is the only interface a Dragon owner has before any device is paired, so
//! this endpoint must answer before a session exists — like `/api/box/health`
//! and `/api/setup/state`.
//!
//! **Unlike those two, this one carries a secret**, so it is loopback-only.
//!
//! The standing pair code is the box's live credential: anyone who can read it
//! can enroll a device. The whole trust model is that reading it requires
//! standing in front of the box — proximity is the authority, exactly as with
//! `virtues sudo`. That is why the raw code is stored encrypted and shown only
//! on physical surfaces, and why it must never be served over the LAN: a
//! stranger on the wifi who cannot see the screen must not be able to claim the
//! box. `/api/setup/state` stays LAN-public precisely because it carries no
//! secret; this endpoint is its box-local sibling.
//!
//! The loopback test mirrors `middleware::auth`: a peer address of `127.0.0.1`
//! or `::1` **and** no forwarding header, because a reverse proxy in front of
//! the box also connects from loopback while forwarding a remote client.

use axum::{
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Serialize;
use std::net::SocketAddr;

use crate::server::AppState;

#[derive(Debug, Serialize)]
pub struct DisplayState {
    /// The live standing pair code, digits only (rendered "123 456"). `None`
    /// when the box could not mint one — the display shows an honest fault
    /// rather than a blank space.
    pub pair_code: Option<String>,
    /// SSID of the setup access point, when it is up.
    pub ap_ssid: Option<String>,
    /// The AP's passphrase, shown as TEXT beside the QR.
    ///
    /// The QR already carries it, so this looks redundant — it is not. A QR is
    /// only readable by a camera, and the device that needs this network is
    /// often a laptop, which has none. It also covers a camera that will not
    /// focus, a cracked screen, and the case that stranded the lab box: the
    /// owner needing to reach the machine from something other than a phone.
    /// Printing it costs nothing — anyone who can read this screen is already
    /// standing in front of the box, which is the whole trust model.
    pub ap_passphrase: Option<String>,
    /// True once at least one device has paired — the display stops showing
    /// setup and moves to the ambient screen.
    pub claimed: bool,
    /// Whether the box has a usable network yet.
    pub online: bool,
    /// NetworkManager's connectivity verdict (`full` | `portal` | `limited` |
    /// `none` | `unknown`). `online` is derived from it, but the display needs
    /// the raw word for one job: with honest online-detection, a captive-portal
    /// join reads as still-offline, and screen 1 must say WHY ("joined, but
    /// that network wants a browser sign-in") or the commonest office failure
    /// is a silent one.
    pub connectivity: String,
    /// SSID the box's wifi is associated with, when there is one and the box
    /// is still in setup — the captive hint names the network it is stuck on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wifi_ssid: Option<String>,
    /// Paired, unrevoked devices — the ambient screen's headline number.
    pub devices: i64,
    /// The box's codename ("Quaint Tern") — shown on every screen so a human
    /// can match THIS box to its chip in the app when two share a house.
    pub box_name: String,
    /// Whether the box holds an account key. THE gate between setup screens 2
    /// and 3: an unlinked box has no relay, and a box without relay reach is a
    /// LAN-hostage — fine at home, broken in a dorm or office. Discovered as
    /// a wall on 2026-08-11: a freshly-onboarded office box was paired but
    /// unreachable by the app, because linking had been treated as optional.
    pub linked: bool,
    /// Present only on setup screen 2 (online + unclaimed + !linked): the
    /// device-authorization code for linking, shown as text beside the QR.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_code: Option<String>,
}

pub async fn display_state_handler(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !is_box_local(&peer, &headers) {
        // Deliberately terse: a LAN caller learns only that this door is shut,
        // not whether a code exists or what shape it has.
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({
            "error": "display state is available only on the box itself"
        })))
        .into_response();
    }

    let pool = state.db.pool();

    let pair_code = match crate::api::pair::ensure_standing(pool).await {
        Ok(minted) => Some(minted.token),
        Err(e) => {
            // Not fatal: the rest of the screen is still worth drawing, and the
            // display renders a fault line for the missing code.
            tracing::warn!(error = %e, "display: could not ensure a standing pair code");
            None
        }
    };

    // Excludes `local-console` — otherwise a box nobody has paired reports one
    // device and the display skips its setup screen entirely. See
    // `api::pair::paired_device_count`.
    let devices = crate::api::pair::paired_device_count(pool).await;

    let linked = crate::virtues_api::renew::read_api_key(pool)
        .await
        .ok()
        .flatten()
        .is_some();
    let connectivity = crate::cli::link::connectivity();
    let online = crate::cli::link::verdict_means_online(&connectivity);
    // Only during setup, and only when the verdict needs explaining: the
    // ambient screen doesn't render it, and `nmcli` ssid lookups aren't free.
    let wifi_ssid = if devices == 0 && !online && connectivity != "none" {
        crate::api::provision::active_client_ssid().await
    } else {
        None
    };

    // Screen 2's device-auth session: started lazily the first time the
    // display needs it, cached and re-polled here. The display's own 2s state
    // poll is the heartbeat that drives the whole link to completion — no
    // extra task, and it stops the moment the box is linked or claimed.
    let link_code = if !linked && online && devices == 0 {
        link_session::code_and_poll(pool).await
    } else {
        None
    };

    let ap_ssid = current_ap_ssid();
    (
        StatusCode::OK,
        Json(DisplayState {
            pair_code,
            // Only when the AP is actually up: a passphrase for a network that
            // is not broadcasting is noise on a screen with no room for it.
            ap_passphrase: ap_ssid.as_ref().and_then(|_| ap_passphrase()),
            ap_ssid,
            claimed: devices > 0,
            online,
            connectivity,
            wifi_ssid,
            devices,
            box_name: crate::codename::pretty(&crate::codename::box_codename()),
            linked,
            link_code,
        }),
    )
        .into_response()
}

/// `GET /api/display/qr` — the setup AP's join code, as an SVG.
///
/// Takes **no parameters**: the server renders the payload for the AP it is
/// actually running, rather than encoding whatever a caller hands it. An
/// endpoint that turns arbitrary text into a scannable QR on the owner's own
/// screen is a small but real oracle — the panel is a trusted surface, and
/// anything it displays should originate on the box.
///
/// `T:WPA` and not an open network: the customer's home wifi password crosses
/// this link during provisioning, and on an open AP that is cleartext to anyone
/// in range. The QR carries the passphrase, so a WPA2 network costs the user
/// nothing — the phone camera joins either way, with no typing.
pub async fn display_qr_handler(
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !is_box_local(&peer, &headers) {
        return (StatusCode::FORBIDDEN, "not available off-box").into_response();
    }
    let Some(ssid) = current_ap_ssid() else {
        return (StatusCode::NOT_FOUND, "no setup network is up").into_response();
    };
    let Some(psk) = ap_passphrase() else {
        return (StatusCode::NOT_FOUND, "no AP passphrase available").into_response();
    };
    let svg = crate::api::pair::render_qr_svg(&wifi_payload(&ssid, &psk));
    (
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "image/svg+xml"),
            // The AP can be raised or dropped at any moment, so this must never
            // be cached — a stale QR points a camera at a network that is gone.
            (axum::http::header::CACHE_CONTROL, "no-store"),
        ],
        svg,
    )
        .into_response()
}

/// Where an owner gets the app. Public, so it is safe to put in a QR.
///
/// A plain landing page rather than a store deep link: the primary client is
/// the desktop app, the box does not know what scanned it, and the page can
/// route to the right store without the box shipping a guess that goes stale.
const DOWNLOADS_URL: &str = "https://virtues.com/downloads";

/// `GET /api/display/app-qr` — where to get the app, as an SVG.
///
/// Separate endpoint from `/api/display/qr` because the two are shown at
/// different moments and mean different things, and merging them would put the
/// screen back in the state this whole sequence exists to undo.
///
/// **This QR is only useful once the box is online**, which is exactly when the
/// display shows it. Before that the owner's phone is joined to a setup network
/// with no uplink, and a download link is an instruction they cannot follow —
/// the ordering bug that shipped in the first version of this screen: it
/// offered `virtues.com/downloads` to a phone it had just told to join an AP
/// with no internet.
pub async fn display_app_qr_handler(
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !is_box_local(&peer, &headers) {
        return (StatusCode::FORBIDDEN, "not available off-box").into_response();
    }
    let svg = crate::api::pair::render_qr_svg(DOWNLOADS_URL);
    (
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "image/svg+xml"),
            // Constant payload, but the panel is long-lived and a cached SVG
            // buys nothing on a loopback request.
            (axum::http::header::CACHE_CONTROL, "no-store"),
        ],
        svg,
    )
        .into_response()
}

/// `GET /api/display/link-qr` — the account-linking URL, as an SVG.
///
/// Box-local like everything here. Renders the verification URL of the
/// CURRENT cached device-auth session (the one whose code the state endpoint
/// is showing) — the two must agree or the owner scans a QR for one session
/// while reading the code of another.
pub async fn display_link_qr_handler(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !is_box_local(&peer, &headers) {
        return (StatusCode::FORBIDDEN, "not available off-box").into_response();
    }
    let Some(url) = link_session::verification_url(state.db.pool()).await else {
        return (StatusCode::NOT_FOUND, "no link session").into_response();
    };
    let svg = crate::api::pair::render_qr_svg(&url);
    (
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "image/svg+xml"),
            (axum::http::header::CACHE_CONTROL, "no-store"),
        ],
        svg,
    )
        .into_response()
}

/// The display's device-authorization session: lazily started, cached until it
/// expires, opportunistically polled.
///
/// The design constraint is that the DISPLAY is the driver: it polls state
/// every 2s during setup, and that heartbeat both keeps the session fresh and
/// redeems it when the owner completes sign-in on their phone. Success stores
/// the api key (`link::poll` does that internally), after which the state
/// endpoint reports `linked: true` and this cache is never consulted again.
///
/// Atlas polling is rate-limited to the interval the device-auth response
/// asked for — the display's 2s heartbeat must not become a 2s hammer.
mod link_session {
    use sqlx::PgPool;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    struct Session {
        start: crate::virtues_api::link::LinkStart,
        born: Instant,
        last_poll: Instant,
    }

    static SESSION: Mutex<Option<Session>> = Mutex::new(None);

    fn take_valid() -> Option<crate::virtues_api::link::LinkStart> {
        let g = SESSION.lock().ok()?;
        let s = g.as_ref()?;
        let ttl = Duration::from_secs(s.start.expires_in.max(0) as u64);
        (s.born.elapsed() < ttl).then(|| s.start.clone())
    }

    /// The code to display — starting or refreshing the session as needed,
    /// and polling atlas (rate-limited) so completion is noticed.
    pub async fn code_and_poll(db: &PgPool) -> Option<String> {
        if take_valid().is_none() {
            // Before minting a session, adopt any link already in flight in
            // the DB — a session surviving a service restart, or a claim
            // grant the app injected over BLE (RPC 0x82). `start` OVERWRITES
            // the stored device_code, so starting here would orphan a code
            // someone may be mid-redeeming on their phone.
            let adopted = crate::virtues_api::link::inflight(db).await.ok().flatten();
            let start = match adopted {
                Some(s) => s,
                None => {
                    let http = crate::http_client::virtues_api_client();
                    let atlas = crate::virtues_api::atlas_url();
                    match crate::virtues_api::link::start(db, &http, &atlas).await {
                        Ok(s) => s,
                        Err(e) => {
                            // Atlas unreachable: no code to show. The display
                            // renders the waiting state; the next heartbeat
                            // retries.
                            tracing::debug!(error = %format!("{e:#}"), "display: link start failed");
                            return None;
                        }
                    }
                }
            };
            if let Ok(mut g) = SESSION.lock() {
                *g = Some(Session {
                    start,
                    born: Instant::now(),
                    last_poll: Instant::now(),
                });
            }
        }

        // Poll at most every `interval` seconds, driven by the state heartbeat.
        let due = {
            let g = SESSION.lock().ok()?;
            let s = g.as_ref()?;
            s.last_poll.elapsed() >= Duration::from_secs(s.start.interval.max(2))
        };
        if due {
            if let Ok(mut g) = SESSION.lock() {
                if let Some(s) = g.as_mut() {
                    s.last_poll = Instant::now();
                }
            }
            let http = crate::http_client::virtues_api_client();
            let atlas = crate::virtues_api::atlas_url();
            match crate::virtues_api::link::poll(db, &http, &atlas).await {
                Ok(crate::virtues_api::link::LinkStatus::Ready) => {
                    tracing::info!("display: box linked via screen-2 device auth");
                    if let Ok(mut g) = SESSION.lock() {
                        *g = None;
                    }
                    // Relay reach comes up in-process: `link::poll` fetched
                    // the relay config and asked the reach loop to rebind
                    // (`crate::relay::request_rebind`) — no restart involved.
                    return None;
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::debug!(error = %format!("{e:#}"), "display: link poll failed");
                }
            }
        }
        // Empty = an adopted app grant (no screen code ever existed): the
        // display shows its pending state for the seconds the redeem takes.
        take_valid().map(|s| s.user_code).filter(|c| !c.is_empty())
    }

    /// The QR payload for the current session — never a fresh session (the QR
    /// must match the code on screen).
    pub async fn verification_url(_db: &PgPool) -> Option<String> {
        take_valid()
            .map(|s| s.verification_uri_complete)
            .filter(|u| !u.is_empty())
    }
}

/// The AP's passphrase, read the same way `maintenance::setup_ap` derives it.
///
/// Loopback-only like everything else here, and by the same argument: it is a
/// secret whose whole protection is that reading it requires being in the room.
fn ap_passphrase() -> Option<String> {
    if let Ok(s) = std::fs::read_to_string("/var/lib/virtues/ap-passphrase") {
        let s = s.trim().to_string();
        if s.len() >= 8 {
            return Some(s);
        }
    }
    let id = std::fs::read_to_string("/etc/machine-id").ok()?;
    let derived: String = id.trim().chars().take(12).collect();
    (derived.len() >= 8).then_some(derived)
}

/// The `WIFI:` URI both iOS and Android cameras join natively.
///
/// **`P:` is the whole point.** Without the passphrase field this encodes only
/// a network name, so scanning it prompts for a password instead of joining —
/// which defeats the one job the QR has. Shipped that way briefly and it
/// stranded the lab box: the AP was up, the passphrase existed only inside a
/// QR that did not contain it, and there was no other way to read it.
fn wifi_payload(ssid: &str, passphrase: &str) -> String {
    // `;` `:` `\\` and `"` are separators in this grammar and must be escaped,
    // or a passphrase containing one silently truncates the payload.
    let esc = |v: &str| {
        v.replace('\\', "\\\\")
            .replace(';', "\\;")
            .replace(':', "\\:")
            .replace('"', "\\\"")
            .replace(',', "\\,")
    };
    format!("WIFI:S:{};T:WPA;P:{};;", esc(ssid), esc(passphrase))
}

/// Is this request from a process on the box itself?
///
/// Pure so the proxy case is testable without a socket. Mirrors
/// `middleware::auth`'s rule exactly — if these two ever disagree, the looser
/// one is a hole.
fn is_box_local(peer: &SocketAddr, headers: &HeaderMap) -> bool {
    let proxied =
        headers.contains_key("x-forwarded-for") || headers.contains_key("forwarded");
    // Canonicalized, or an IPv4 loopback caller is refused: on the dual-stack
    // `*:8000` socket `127.0.0.1` arrives as `::ffff:127.0.0.1`, which
    // `is_loopback()` does not match. The kiosk happens to resolve `localhost`
    // to `::1` and so was unaffected, which is why this hid. See
    // `crate::peer_addr`.
    crate::peer_addr::canonical_peer(peer).is_loopback() && !proxied
}

/// SSID of the setup AP, if one is up right now.
///
/// Read from NetworkManager rather than stored: the AP is raised and dropped by
/// the provisioning flow, and a cached value would leave the display advertising
/// a network that no longer exists — which is worse than showing nothing, since
/// the owner would sit there scanning for it.
fn current_ap_ssid() -> Option<String> {
    // Ask for the AP connection BY NAME, then read its SSID out of the profile.
    // These are two different strings and conflating them is easy: the
    // connection is named `virtues-setup-ap` while the SSID it broadcasts is
    // `Virtues-XXXX`. Matching the connection list for a `Virtues-` prefix — as
    // this did originally — never matches anything, so the display silently
    // rendered "no setup network" while the AP was up and broadcasting.
    let active = std::process::Command::new("nmcli")
        .args(["-t", "-f", "NAME", "connection", "show", "--active"])
        .output()
        .ok()?;
    let is_up = String::from_utf8_lossy(&active.stdout)
        .lines()
        .any(|l| l.trim() == crate::maintenance::setup_ap::AP_CON_NAME);
    if !is_up {
        return None;
    }

    let out = std::process::Command::new("nmcli")
        .args([
            "-g",
            "802-11-wireless.ssid",
            "connection",
            "show",
            crate::maintenance::setup_ap::AP_CON_NAME,
        ])
        .output()
        .ok()?;
    let ssid = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!ssid.is_empty()).then_some(ssid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn addr(s: &str) -> SocketAddr {
        s.parse().unwrap()
    }

    #[test]
    fn wifi_payload_carries_the_passphrase() {
        // The bug this pins: without P:, scanning prompts for a password
        // instead of joining, and the passphrase becomes unreadable.
        let p = wifi_payload("Virtues-E3C7", "abc123def456");
        assert!(p.contains("P:abc123def456"), "no passphrase in payload: {p}");
        assert!(p.starts_with("WIFI:S:Virtues-E3C7;"));
    }

    #[test]
    fn wifi_payload_escapes_grammar_characters() {
        let p = wifi_payload("My:Net", "pa;ss");
        assert!(p.contains(r"S:My\:Net"), "unescaped ssid: {p}");
        assert!(p.contains(r"P:pa\;ss"), "unescaped passphrase: {p}");
    }

    #[test]
    fn loopback_is_box_local() {
        assert!(is_box_local(&addr("127.0.0.1:5000"), &HeaderMap::new()));
        assert!(is_box_local(&addr("[::1]:5000"), &HeaderMap::new()));
    }

    #[test]
    fn v4_mapped_loopback_is_box_local() {
        // On the dual-stack `*:8000` socket, `curl http://127.0.0.1:8000`
        // arrives as this and was refused with a 403 the display could not
        // explain. Measured on hardware 2026-08-10.
        assert!(is_box_local(&addr("[::ffff:127.0.0.1]:5000"), &HeaderMap::new()));
    }

    #[test]
    fn v4_mapped_lan_peer_is_still_not_box_local() {
        assert!(!is_box_local(&addr("[::ffff:192.168.1.44]:5000"), &HeaderMap::new()));
    }

    #[test]
    fn lan_peer_is_not_box_local() {
        // The whole point: a stranger on the wifi cannot read the pair code.
        assert!(!is_box_local(&addr("192.168.1.44:5000"), &HeaderMap::new()));
        assert!(!is_box_local(&addr("10.42.0.169:5000"), &HeaderMap::new()));
    }

    #[test]
    fn proxied_loopback_is_not_box_local() {
        // A reverse proxy on the box connects from loopback while forwarding a
        // remote client — trusting the peer address alone would hand the code
        // to whoever is on the other side of it.
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", HeaderValue::from_static("203.0.113.9"));
        assert!(!is_box_local(&addr("127.0.0.1:5000"), &h));

        let mut h = HeaderMap::new();
        h.insert("forwarded", HeaderValue::from_static("for=203.0.113.9"));
        assert!(!is_box_local(&addr("127.0.0.1:5000"), &h));
    }
}

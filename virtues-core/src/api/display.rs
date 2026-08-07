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
    /// Paired, unrevoked devices — the ambient screen's headline number.
    pub devices: i64,
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
            online: crate::cli::link::primary_ip().is_some(),
            devices,
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
    peer.ip().is_loopback() && !proxied
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

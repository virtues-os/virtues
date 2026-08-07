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

use crate::server::webhook::AppState;

#[derive(Debug, Serialize)]
pub struct DisplayState {
    /// The live standing pair code, digits only (rendered "123 456"). `None`
    /// when the box could not mint one — the display shows an honest fault
    /// rather than a blank space.
    pub pair_code: Option<String>,
    /// SSID of the setup access point, when it is up.
    pub ap_ssid: Option<String>,
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

    let devices: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app_device WHERE revoked_at IS NULL")
            .fetch_one(pool)
            .await
            .unwrap_or(0);

    (
        StatusCode::OK,
        Json(DisplayState {
            pair_code,
            ap_ssid: current_ap_ssid(),
            claimed: devices > 0,
            online: crate::cli::link::primary_ip().is_some(),
            devices,
        }),
    )
        .into_response()
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
    let out = std::process::Command::new("nmcli")
        .args(["-t", "-f", "NAME,TYPE,DEVICE", "connection", "show", "--active"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let mut f = line.split(':');
        let name = f.next()?;
        let kind = f.next().unwrap_or("");
        if kind.contains("wireless") && name.starts_with("Virtues-") {
            return Some(name.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn addr(s: &str) -> SocketAddr {
        s.parse().unwrap()
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

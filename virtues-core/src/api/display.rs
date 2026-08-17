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
    ///
    /// The PANEL does not render this and must not start: RPC 0x85 hands the
    /// code to an authorized BLE session, and the only box that would need it
    /// printed here is one with no session — which is a box waiting for
    /// someone to START, and that needs the phrase. The two secrets want the
    /// same slot, and showing the wrong one stopped a live run dead
    /// (2026-08-13). Kept in the payload for `virtues pair` on the box's own
    /// terminal, which is the documented door when there is no Bluetooth.
    pub pair_code: Option<String>,
    /// The box cannot find the disk its record lives on.
    ///
    /// `None` on every healthy box, and on every DIY box unconditionally — a
    /// self-hosted state root is a directory on the root filesystem by design.
    /// When set, this is the ONLY thing the ambient screen says: an appliance
    /// with no data disk still boots (fstab carries `nofail`, deliberately, so
    /// that it can come up far enough to tell someone), and the whole value of
    /// that is spent if the screen then reports business as usual.
    /// See `crate::data_disk`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_disk_fault: Option<&'static str>,
    /// Seconds the case button has been held, when it is down right now.
    ///
    /// The button forgets every paired device after three seconds, and without
    /// this the owner holds an unlabelled button on a silent box and lets go —
    /// the failure mode of every long-press control that does not narrate
    /// itself. Showing the count also makes an *unintended* hold visible while
    /// there is still time to stop: a cable resting on the button announces
    /// itself instead of quietly unpairing the house.
    ///
    /// `None` between presses, and always on a DIY box — the watcher does not
    /// run there. See `maintenance::reset_button`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub button_held_secs: Option<u64>,
    /// How long the hold has to last. Sent rather than hardcoded in the panel
    /// so the two cannot disagree about what the owner is waiting for.
    pub button_hold_target: u64,
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
    /// The box's setup phrase — four words, the one secret that proves
    /// ownership (`api::setup_phrase`). Shown on the panel ONLY while the box
    /// is unclaimed, where it rotates; `None` forever once frozen at first
    /// claim. That asymmetry is the whole security argument, so this field
    /// going quiet is the feature, not a fault.
    ///
    /// Loopback-only like everything else here, and for the same reason: a
    /// stranger on the wifi who cannot see the screen must not learn it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setup_phrase: Option<String>,
    /// True when this box has been claimed before and its phrase is frozen —
    /// i.e. it was RESET, not unboxed.
    ///
    /// Without this the panel cannot tell the two apart, because both are
    /// `claimed: false` with no phrase to print, and it would render the virgin
    /// screen with a blank where the words go — which reads as a fault at
    /// exactly the moment an owner is most worried. What they need to be told
    /// instead is that the words they saved still work and their record is
    /// still here.
    pub phrase_frozen: bool,
    /// The device currently configuring this box over Bluetooth, if any, as it
    /// named itself ("Adam's Mac"). `Some("")` means a session is live but the
    /// client sent no name.
    ///
    /// The panel shows this INSTEAD of the phrase: the words are spent the
    /// moment they are accepted, so they stop being readable by anyone who
    /// wanders past. Goes quiet 90s after the last command
    /// (`setup_phrase::PANEL_SESSION_SECS`) so a setup that dies halfway puts
    /// the words back rather than stranding the owner in front of a box that
    /// will not say how to start over.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setup_session: Option<String>,
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

    // The panel's device-auth session is GONE, and with it the last reason
    // this endpoint reached out to atlas.
    //
    // It existed for a screen that printed a link code for the owner to type
    // on their phone. The app carries the account grant over BLE now (RPC
    // 0x82/0x84), so nothing rendered the code — but the session was still
    // being minted and polled on the display's 2s heartbeat, which meant an
    // unclaimed box quietly held a live device-authorization with atlas that
    // no surface would ever complete. See `virtues_api::link` for the path
    // that replaced it.

    // Unclaimed only — `display_phrase` returns None once frozen, but skipping
    // the query entirely on a claimed box keeps the ambient screen off the
    // encryptor and the DB.
    let (setup_phrase, phrase_frozen) = if devices == 0 {
        let frozen = crate::api::setup_phrase::is_frozen(pool).await;
        // A frozen box has nothing to print and `display_phrase` would only
        // confirm that; don't ask.
        let phrase = if frozen {
            None
        } else {
            match crate::api::setup_phrase::display_phrase(pool).await {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(error = %e, "display: could not read the setup phrase");
                    None
                }
            }
        };
        (phrase, frozen)
    } else {
        (None, false)
    };

    // Only while the panel is on a setup screen: `session()` is a live mirror
    // of the BLE layer, and the ambient screen has no line for it.
    let setup_session =
        (devices == 0).then(crate::api::setup_phrase::session).flatten();

    (
        StatusCode::OK,
        Json(DisplayState {
            pair_code,
            data_disk_fault: crate::data_disk::status().message(),
            button_held_secs: crate::maintenance::reset_button::hold_secs(),
            button_hold_target: crate::maintenance::reset_button::HOLD_SECS,
            claimed: devices > 0,
            online,
            connectivity,
            wifi_ssid,
            devices,
            box_name: crate::codename::pretty(&crate::codename::box_codename()),
            linked,
            setup_phrase,
            phrase_frozen,
            setup_session,
        }),
    )
        .into_response()
}

// The QR endpoints are gone (2026-08-17), and nothing was rendering them.
//
// `/api/display/qr` encoded the setup AP's `WIFI:` join string and
// `/api/display/link-qr` the account-link URL. Both belonged to a panel that
// showed numbered setup screens; that panel is one screen now (the app, and
// the four words), and the app carries wifi and the account grant over BLE.
// They stayed routed for days after their last caller went away, which is the
// specific way a trusted surface accumulates endpoints nobody can account for.
//
// `link_session` went with them. It lazily minted a device-authorization
// session with atlas and polled it on the display's 2s heartbeat — so an
// unclaimed box held a live link nothing on any screen could complete.
//
// If a QR is ever wanted here again, note what the deleted handlers got right
// and rebuild it the same way: they took NO parameters and rendered only
// payloads the box itself produced. An endpoint that turns caller-supplied
// text into a scannable code on the owner's own panel is a small but real
// oracle.

/// `GET /api/display/updating` — is an upgrade running right now?
///
/// Exists so the panel can latch that fact **while the box is still able to
/// say it**. `virtues upgrade` stops this very server for the length of a flip,
/// a migration and a start; the kiosk is a page served by it, so after that
/// moment there is nothing left to ask. The panel polls this on a slow timer,
/// remembers a `true`, and renders "Updating — back in a minute" through the
/// outage instead of its ambient screen with a NO SERVER badge.
///
/// Box-local like the rest of this module. It carries no secret, but it is a
/// panel endpoint and the panel runs on the box; there is no reason for the
/// LAN to be able to ask, and every reason to keep this module's rule
/// uniform — a single exception is how the next one gets argued for.
pub async fn display_updating_handler(
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !is_box_local(&peer, &headers) {
        return (StatusCode::FORBIDDEN, "not available off-box").into_response();
    }
    (
        StatusCode::OK,
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Json(serde_json::json!({ "active": upgrade_unit_active() })),
    )
        .into_response()
}

/// Is the transient upgrade unit running?
///
/// `systemctl is-active` on the unit `api::updates` starts the upgrade under.
/// Only the upgrade unit, deliberately — not `virtues-prepare`: a prepare is a
/// background download that changes nothing the owner would notice, and a
/// panel announcing "Updating" for six hours of scheduled fetching would teach
/// them to ignore the word by the time it mattered.
fn upgrade_unit_active() -> bool {
    std::process::Command::new("systemctl")
        .args(["is-active", "--quiet", "virtues-upgrade.service"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

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

//! The box's device-authorization session with atlas — started lazily, cached
//! until it expires, opportunistically polled.
//!
//! ## Why this is its own module
//!
//! It used to live inside `api::display`, because the panel was its only
//! consumer: setup screen 2 printed a link code for the owner to retype on
//! their phone, and the display's 2s heartbeat both kept the session fresh and
//! redeemed it when sign-in completed.
//!
//! That screen is gone — the app fetches the code over Bluetooth (RPC 0x84) and
//! puts it straight in a URL, so nothing is retyped. But the SESSION is not
//! gone, and moving it out is the correction to a real mistake: it was deleted
//! along with the screen on 2026-08-17, on the reasoning that no surface
//! rendered it. `maintenance::ble_provision` did, and does — it is Linux-only,
//! so a macOS `cargo check` never compiled the call site, and the account-link
//! step of onboarding was broken until CI said so.
//!
//! The lesson is in the placement: a cache named after the screen that happened
//! to poll it will be deleted with that screen. This one is named for what it
//! is.
//!
//! ## The heartbeat is the driver
//!
//! Nothing here runs on a timer of its own. Whoever asks for the code — the BLE
//! layer, on the app's behalf — also drives the poll that notices the owner
//! completing sign-in on their phone. Success stores the api key
//! (`link::poll` does that internally), after which the box reports
//! `linked: true` and this cache is never consulted again.
//!
//! Atlas polling is rate-limited to the interval the device-auth response asked
//! for, so a fast caller cannot become a hammer.

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
            // Expired/denied, or no in-flight link at all: `link::poll`
            // has ALREADY deleted the stored device_code in both cases, so
            // the cached session is now a code nobody can redeem. Drop it
            // so the next heartbeat mints a fresh one.
            //
            // Swallowing these (as `Ok(_) => {}` did) put a GHOST CODE on
            // the glass: the panel kept showing a dead `user_code` for the
            // rest of its 15-minute TTL, atlas correctly refused it, and
            // the owner got "that code didn't work" with no way to
            // discover why. Seen live 2026-08-11 with the box holding one
            // row — `iroh_secret_key` — and a code still on screen.
            Ok(crate::virtues_api::link::LinkStatus::Expired)
            | Ok(crate::virtues_api::link::LinkStatus::None) => {
                tracing::info!("display: link session expired — minting a fresh code");
                if let Ok(mut g) = SESSION.lock() {
                    *g = None;
                }
                return None;
            }
            Ok(crate::virtues_api::link::LinkStatus::Pending) => {}
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

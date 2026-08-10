//! Driving a box's wifi setup from the app, over the box's own setup AP.
//!
//! The appliance boots with no network and raises `Virtues-XXXX`. The owner's
//! phone joins it — by scanning the QR on the display, or from the wifi menu
//! using the passphrase printed beside it — and lands on `10.42.0.x`, with the
//! box at `10.42.0.1`. From there the app can reach `/api/provision/*` and do
//! the job the captive portal used to do alone.
//!
//! **Why the app and not the captive portal.** The portal works, and stays as
//! the path for laptops and Android. But it collects the owner's *home wifi
//! password* in a captive webview: no password manager, no autofill, one shot,
//! and a typo surfaces twenty seconds later as a switchover failure. In the app
//! that is a native field, and — the part that matters more — the app holds the
//! setup session across the network handoff, so provisioning and pairing are
//! one continuous flow instead of two disconnected ones.
//!
//! **Order is provision → pair, and that is not arbitrary.** Every route here
//! 404s the moment a device pairs (`api::provision`'s "setup is a phase, not a
//! feature"), `maintenance::setup_ap` drops the AP on pair, and the display
//! leaves its setup screen on pair. Pairing first would require relaxing all
//! three. So: wifi first, over the AP; then the code, over the owner's LAN.
//!
//! Requests go through this crate rather than `fetch` in the webview
//! deliberately — plain HTTP to `10.42.0.1` from a `tauri://` origin is exactly
//! what App Transport Security exists to block, and native `reqwest` is not
//! subject to it. Same reason `pair::consume` lives here.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Short: everything here is one hop away on a link with no contention. A box
/// that does not answer in this window is not on the other end of the AP.
const PROBE_SECS: u64 = 8;

/// Generous, because a join is `nmcli device wifi connect` end to end — DHCP
/// included. In practice the request rarely lives this long: the AP comes down
/// as the *first* step of the join, so the caller's socket usually dies first.
/// See [`JoinOutcome::Unknown`].
const JOIN_SECS: u64 = 25;

/// A network the BOX can see. Not the phone's list — the box is the thing that
/// has to reach it, and offering the phone's would let someone pick a network
/// the box cannot hear.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Network {
    pub ssid: String,
    /// 0–100, as NetworkManager reports it.
    pub signal: u8,
    /// False for an open network — the UI skips the password field.
    pub secured: bool,
}

/// Where the box's radio ended up.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Status {
    pub online: bool,
    pub ssid: Option<String>,
}

/// How a join attempt ended.
///
/// Three outcomes, not two, and the third is the common one.
#[derive(Debug, Clone)]
pub enum JoinOutcome {
    /// The box reported a successful association.
    Joined,
    /// The box reported a failure, in NetworkManager's own words. Passed
    /// through unreworded on purpose: *"Secrets were required, but not
    /// provided"* tells an owner their password was wrong far better than
    /// anything we would write.
    Failed(String),
    /// **The request did not come back, and that is expected.**
    ///
    /// AP+STA concurrency does not work on the Q6A, so the switchover is
    /// sequential: the box drops the AP *before* it attempts the join. The
    /// phone issuing this request is sitting on that AP, so its socket dies
    /// mid-flight — on the success path just as often as the failure path.
    ///
    /// Treating a transport error as failure here would report "couldn't join"
    /// to someone whose box just came online perfectly. The only honest move is
    /// to stop guessing and go ask: poll [`status`] once the phone has a link
    /// again, whichever network that turns out to be.
    Unknown,
}

fn client(secs: u64) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(secs))
        .build()
        .context("build provisioning http client")
}

/// `GET /api/provision/networks` — what the box can see.
///
/// Doubles as the **feature probe** for the whole flow: the endpoint exists
/// only for a caller on the AP subnet talking to an unclaimed box, and 404s
/// otherwise. So a success here means "this box is unclaimed and I am on its
/// setup network" without the app having to work either fact out for itself.
pub async fn networks(origin: &str) -> Result<Vec<Network>> {
    let url = format!("{}/api/provision/networks", origin.trim_end_matches('/'));
    let resp = client(PROBE_SECS)?
        .get(&url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;
    if !resp.status().is_success() {
        anyhow::bail!("provisioning is not open on this box ({})", resp.status());
    }
    resp.json().await.context("decode provision/networks")
}

/// Is this origin a box in setup mode, reachable from where we are standing?
///
/// Cheap enough to run against every candidate the subnet scan turned up.
/// Returns false for any error — a box that will not answer this is not one we
/// can provision, and the reason does not change what the app does next.
pub async fn is_open(origin: &str) -> bool {
    let url = format!("{}/api/provision/status", origin.trim_end_matches('/'));
    match client(PROBE_SECS) {
        Ok(c) => matches!(c.get(&url).send().await, Ok(r) if r.status().is_success()),
        Err(_) => false,
    }
}

/// `POST /api/provision/join` — put the box on the owner's network.
///
/// Never returns `Err` for a dead socket; that is [`JoinOutcome::Unknown`].
/// `Err` is reserved for "we could not even form the request."
pub async fn join(origin: &str, ssid: &str, psk: Option<&str>) -> Result<JoinOutcome> {
    #[derive(Serialize)]
    struct Body<'a> {
        ssid: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        psk: Option<&'a str>,
    }
    #[derive(Deserialize)]
    struct Reply {
        ok: bool,
        #[serde(default)]
        detail: Option<String>,
    }

    let url = format!("{}/api/provision/join", origin.trim_end_matches('/'));
    let body = Body {
        ssid,
        // An empty string is not a passphrase; sending one makes an open
        // network look secured to nmcli and the join fails obscurely.
        psk: psk.filter(|p| !p.is_empty()),
    };

    // No retry, unlike `pair::consume`. A join is not idempotent from the
    // owner's point of view — a second attempt lands on a box whose radio is
    // mid-association, and the retry is what breaks it.
    let resp = match client(JOIN_SECS)?.post(&url).json(&body).send().await {
        Ok(r) => r,
        Err(_) => return Ok(JoinOutcome::Unknown),
    };
    if !resp.status().is_success() {
        return Ok(JoinOutcome::Failed(format!(
            "the box refused the request ({})",
            resp.status()
        )));
    }
    match resp.json::<Reply>().await {
        Ok(r) if r.ok => Ok(JoinOutcome::Joined),
        Ok(r) => Ok(JoinOutcome::Failed(
            r.detail.unwrap_or_else(|| "couldn't join that network".into()),
        )),
        // Body truncated by the AP going down mid-response. Same situation as a
        // dead socket, and the same answer: ask, don't guess.
        Err(_) => Ok(JoinOutcome::Unknown),
    }
}

/// `GET /api/provision/status` — did it work?
///
/// Reachable from the AP *and* from the owner's LAN once the box is on it, so
/// this is what the app polls across the handoff.
pub async fn status(origin: &str) -> Result<Status> {
    let url = format!("{}/api/provision/status", origin.trim_end_matches('/'));
    let resp = client(PROBE_SECS)?
        .get(&url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;
    if !resp.status().is_success() {
        anyhow::bail!("status unavailable ({})", resp.status());
    }
    resp.json().await.context("decode provision/status")
}

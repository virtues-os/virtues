//! Wifi provisioning over the setup AP — `/api/provision/*`.
//!
//! The one unauthenticated write surface on the box, and it is unauthenticated
//! by necessity: the phone joining the setup AP has no credential yet, because
//! obtaining one is what the rest of onboarding is for. So the gates are not
//! "who are you" but "where are you, and is setup still open":
//!
//!   1. **Only while the box is unclaimed.** The moment any device pairs, every
//!      route here 404s. Setup is a phase, not a feature, and leaving a wifi
//!      writer live for the life of the appliance would be indefensible.
//!   2. **Only from the setup AP's own subnet, or loopback.** Not the LAN. A
//!      caller on the AP is a device the owner physically joined by scanning a
//!      QR off the box's screen — the same proximity argument the pair code
//!      rests on. A caller on the home LAN has proved nothing.
//!
//! Both gates are checked on every route, not once at mount: a box can be
//! claimed while a request is in flight, and the narrow window where a stale
//! router would still accept a write is exactly the kind of thing nobody
//! notices until it matters.
//!
//! **Scope is deliberately tiny.** Three routes: list what the *box* can see,
//! join one, report how that went. No arbitrary `nmcli` passthrough, no
//! connection editing, no disconnect. Everything here is reachable by someone
//! who managed to associate with the AP, so the surface must be small enough to
//! read in one sitting.
//!
//! Why the box's scan list and not the phone's: the box is the thing that has
//! to reach the network. Different antenna, different place in the room, maybe
//! different bands. Offering the phone's list would let someone pick a network
//! the box cannot see, producing a failure with no explanation.

use axum::{
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr};
use std::process::Stdio;
use tokio::process::Command;

use crate::server::AppState;

/// NetworkManager's shared-mode subnet — the addresses it hands out on our own
/// AP. Anything else is somewhere we did not put it.
const AP_SUBNET: [u8; 3] = [10, 42, 0];

#[derive(Debug, Serialize)]
pub struct Network {
    pub ssid: String,
    /// 0–100, as NetworkManager reports it.
    pub signal: u8,
    /// False for an open network — the app skips the password field.
    pub secured: bool,
}

#[derive(Debug, Deserialize)]
pub struct JoinRequest {
    pub ssid: String,
    /// Absent for an open network.
    #[serde(default)]
    pub psk: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct JoinResult {
    pub ok: bool,
    /// Present on failure. Passed through from NetworkManager rather than
    /// reworded: "Secrets were required, but not provided" tells the owner
    /// their password was wrong far better than a generic failure would.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

// ─── gates ──────────────────────────────────────────────────────────────────

/// Is the caller on our own setup AP (or the box itself)?
///
/// Pure, so every branch is testable without a socket.
fn is_setup_peer(peer: &SocketAddr, headers: &HeaderMap) -> bool {
    // A reverse proxy on the box connects from loopback while forwarding a
    // remote client, so a forwarding header disqualifies regardless of peer.
    if headers.contains_key("x-forwarded-for") || headers.contains_key("forwarded") {
        return false;
    }
    match peer.ip() {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            v4.is_loopback() || (o[0] == AP_SUBNET[0] && o[1] == AP_SUBNET[1] && o[2] == AP_SUBNET[2])
        }
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}

/// Setup is open only until the box has an owner.
async fn setup_is_open(state: &AppState) -> bool {
    let claimed: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app_device WHERE revoked_at IS NULL")
            .fetch_one(state.db.pool())
            .await
            .unwrap_or(1); // fail closed: an unreadable DB is not an open door
    claimed == 0
}

/// Both gates. Returns the response to send when refused.
async fn refuse(
    state: &AppState,
    peer: &SocketAddr,
    headers: &HeaderMap,
) -> Option<axum::response::Response> {
    if !is_setup_peer(peer, headers) || !setup_is_open(state).await {
        // 404, not 403: off the AP or after setup, this surface should not
        // appear to exist at all.
        return Some((StatusCode::NOT_FOUND, "not found").into_response());
    }
    None
}

// ─── routes ─────────────────────────────────────────────────────────────────

/// `GET /api/provision/networks` — what the BOX can see.
pub async fn networks_handler(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Some(r) = refuse(&state, &peer, &headers).await {
        return r;
    }
    match scan().await {
        Ok(nets) => (StatusCode::OK, Json(nets)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

/// `POST /api/provision/join` — join one of them.
pub async fn join_handler(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<JoinRequest>,
) -> impl IntoResponse {
    if let Some(r) = refuse(&state, &peer, &headers).await {
        return r;
    }
    if req.ssid.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(JoinResult { ok: false, detail: Some("ssid is required".into()) }),
        )
            .into_response();
    }

    // THE SWITCHOVER IS SEQUENTIAL, and it has to be.
    //
    // Measured on the Q6A 2026-08-07: the radio can scan happily while hosting
    // the AP, but it cannot hold an AP and a client association at the same
    // time. A second virtual interface is created and adopted by NetworkManager,
    // and then the join fails with "object is in an unsuitable state". So the
    // AP must come down for the join to have any chance.
    //
    // That means the phone issuing this request loses its link to us partway
    // through, and there is no way to avoid it — which is why /provision warns
    // the owner up front rather than letting it read as a crash. The lock stops
    // maintenance::setup_ap from noticing "unclaimed and no AP" and putting the
    // AP straight back on top of the association being formed.
    let _lock = ProvisioningLock::take();

    let ap_was_up = ap_is_up().await;
    if ap_was_up {
        let _ = nmcli(&["connection", "down", crate::maintenance::setup_ap::AP_CON_NAME]).await;
    }

    let out = nmcli_join(&req.ssid, req.psk.as_deref()).await;
    match out {
        Some(o) if o.status.success() => {
            // Leave the AP down. The box is on the owner's network now, their
            // phone rejoins its own wifi on its own, and both meet again on the
            // LAN. setup_ap will not re-raise it while unclaimed... it will, in
            // fact, on the next tick — deliberately, because the owner still has
            // to pair, and until they do the AP is the only guaranteed route to
            // a box whose new network might yet prove flaky.
            (StatusCode::OK, Json(JoinResult { ok: true, detail: None })).into_response()
        }
        Some(o) => {
            let detail = String::from_utf8_lossy(&o.stderr).trim().to_string();
            tracing::warn!(ssid = %req.ssid, %detail, "provision: join failed");
            // Put the AP back immediately rather than waiting for the
            // reconciler: the owner's phone is trying to get back to us right
            // now, and every second it cannot is a second they spend believing
            // they have bricked the thing.
            if ap_was_up {
                let _ = nmcli(&["connection", "up", crate::maintenance::setup_ap::AP_CON_NAME]).await;
            }
            (
                StatusCode::OK,
                Json(JoinResult { ok: false, detail: Some(detail) }),
            )
                .into_response()
        }
        None => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(JoinResult { ok: false, detail: Some("nmcli unavailable".into()) }),
        )
            .into_response(),
    }
}

/// Holds the provisioning lock for the life of a join, and releases it however
/// the join ends — including a panic. A leaked lock would leave the box unable
/// to raise its own setup network, which is the one failure an owner has no way
/// to diagnose; the file also carries a TTL as a second backstop.
struct ProvisioningLock;

impl ProvisioningLock {
    fn take() -> Self {
        let _ = std::fs::write(crate::maintenance::setup_ap::PROVISIONING_LOCK, b"");
        Self
    }
}

impl Drop for ProvisioningLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(crate::maintenance::setup_ap::PROVISIONING_LOCK);
    }
}

/// Is our setup AP the active connection right now?
async fn ap_is_up() -> bool {
    let Some(out) = nmcli(&["-t", "-f", "NAME", "connection", "show", "--active"]).await else {
        return false;
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| l.trim() == crate::maintenance::setup_ap::AP_CON_NAME)
}

/// `GET /api/provision/status` — did it work?
pub async fn status_handler(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Some(r) = refuse(&state, &peer, &headers).await {
        return r;
    }
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "online": crate::cli::link::primary_ip().is_some(),
            "ssid": active_client_ssid().await,
        })),
    )
        .into_response()
}

// ─── nmcli ──────────────────────────────────────────────────────────────────

async fn scan() -> Result<Vec<Network>, String> {
    let out = nmcli(&["-t", "-f", "SSID,SIGNAL,SECURITY", "device", "wifi", "list", "--rescan", "yes"])
        .await
        .ok_or_else(|| "nmcli unavailable".to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }

    let text = String::from_utf8_lossy(&out.stdout);
    let mut nets: Vec<Network> = Vec::new();
    for line in text.lines() {
        // `-t` escapes embedded colons as `\:`; split on unescaped ones only.
        let fields = split_terse(line);
        if fields.len() < 3 {
            continue;
        }
        let ssid = fields[0].trim().to_string();
        // Hidden networks report an empty SSID and cannot be picked from a
        // list; they need typing, which is a later feature.
        if ssid.is_empty() {
            continue;
        }
        // Our own AP is in the box's own scan results. Offering the owner the
        // chance to join the box to itself is pure foot-gun.
        if ssid.starts_with("Virtues-") {
            continue;
        }
        let signal = fields[1].trim().parse::<u8>().unwrap_or(0);
        let secured = !fields[2].trim().is_empty();
        // Strongest wins when an SSID appears on several bands/APs.
        match nets.iter_mut().find(|n| n.ssid == ssid) {
            Some(existing) if existing.signal < signal => existing.signal = signal,
            Some(_) => {}
            None => nets.push(Network { ssid, signal, secured }),
        }
    }
    nets.sort_by(|a, b| b.signal.cmp(&a.signal));
    Ok(nets)
}

/// Split one `nmcli -t` line, honouring its `\:` escaping.
fn split_terse(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut escaped = false;
    for c in line.chars() {
        if escaped {
            cur.push(c);
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == ':' {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    out.push(cur);
    out
}

async fn nmcli_join(ssid: &str, psk: Option<&str>) -> Option<std::process::Output> {
    let mut args: Vec<&str> = vec!["device", "wifi", "connect", ssid];
    if let Some(p) = psk {
        if !p.is_empty() {
            args.push("password");
            args.push(p);
        }
    }
    nmcli(&args).await
}

/// SSID of the active client connection, ignoring our own AP.
async fn active_client_ssid() -> Option<String> {
    let out = nmcli(&["-t", "-f", "NAME,TYPE", "connection", "show", "--active"]).await?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let f = split_terse(l);
            (f.len() >= 2).then(|| (f[0].clone(), f[1].clone()))
        })
        .find(|(name, kind)| kind.contains("wireless") && !name.starts_with("Virtues-")
            && name != "virtues-setup-ap")
        .map(|(name, _)| name)
}

/// The passphrase never reaches a shell: `Command` passes argv directly, so an
/// SSID or password containing shell metacharacters is inert.
async fn nmcli(args: &[&str]) -> Option<std::process::Output> {
    Command::new("nmcli")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .await
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn addr(s: &str) -> SocketAddr {
        s.parse().unwrap()
    }

    #[test]
    fn ap_subnet_and_loopback_are_setup_peers() {
        let h = HeaderMap::new();
        assert!(is_setup_peer(&addr("10.42.0.169:41000"), &h));
        assert!(is_setup_peer(&addr("127.0.0.1:41000"), &h));
        assert!(is_setup_peer(&addr("[::1]:41000"), &h));
    }

    #[test]
    fn the_home_lan_is_not_a_setup_peer() {
        // The gate the whole module rests on: being on the owner's wifi proves
        // nothing, while being on the box's own AP means they scanned a QR off
        // its screen.
        let h = HeaderMap::new();
        assert!(!is_setup_peer(&addr("192.168.1.44:41000"), &h));
        assert!(!is_setup_peer(&addr("10.0.0.5:41000"), &h));
        assert!(!is_setup_peer(&addr("10.42.1.5:41000"), &h)); // adjacent /24
    }

    #[test]
    fn a_forwarded_request_is_never_a_setup_peer() {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", HeaderValue::from_static("203.0.113.9"));
        assert!(!is_setup_peer(&addr("10.42.0.169:41000"), &h));
        assert!(!is_setup_peer(&addr("127.0.0.1:41000"), &h));
    }

    #[test]
    fn terse_split_honours_escaped_colons() {
        // An SSID with a colon in it — nmcli escapes it, and naive splitting
        // would silently truncate the network name.
        assert_eq!(
            split_terse(r"my\:net:74:WPA2"),
            vec!["my:net".to_string(), "74".into(), "WPA2".into()]
        );
        assert_eq!(
            split_terse("plain:52:"),
            vec!["plain".to_string(), "52".into(), "".into()]
        );
    }
}

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

#[derive(Debug, Serialize, Deserialize)]
pub struct Network {
    pub ssid: String,
    /// 0–100, as NetworkManager reports it.
    pub signal: u8,
    /// False for an open network — the app skips the password field.
    pub secured: bool,
    /// 802.1X (WPA-Enterprise): credential-per-user networks — offices,
    /// campuses, WeWork. The UI must collect a USERNAME too, and the join
    /// takes the EAP branch. A PSK join against one of these fails after a
    /// long timeout with an error no one can act on, which is how this field
    /// earned its place.
    #[serde(default)]
    pub enterprise: bool,
}

#[derive(Debug, Deserialize)]
pub struct JoinRequest {
    pub ssid: String,
    /// Absent for an open network.
    #[serde(default)]
    pub psk: Option<String>,
    /// Present = 802.1X: `psk` is then the account password, and this is the
    /// account username.
    #[serde(default)]
    pub identity: Option<String>,
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
    // CANONICALIZE FIRST. The server binds `*:8000` (dual-stack), so an IPv4
    // caller arrives as `::ffff:10.42.0.169` and matches the `V6` arm below,
    // never reaching the subnet test. That closed this door on every phone that
    // ever joined the setup AP — and since a closed door here is a 404, which is
    // also what correct operation looks like, it read as a bad venue for days.
    // Found on hardware 2026-08-10. See `crate::peer_addr`.
    match crate::peer_addr::canonical_peer(peer) {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            v4.is_loopback() || (o[0] == AP_SUBNET[0] && o[1] == AP_SUBNET[1] && o[2] == AP_SUBNET[2])
        }
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}

/// Setup is open only until the box has an owner.
async fn setup_is_open(state: &AppState) -> bool {
    // Excludes `local-console`, which every box mints at boot — counting it
    // closed this door before anyone had walked through it. Fails CLOSED on a
    // DB error (a blip must not reopen the wifi-provisioning surface). See
    // `api::pair::is_unclaimed`.
    crate::api::pair::is_unclaimed(state.db.pool()).await
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

/// The box's scan, for `maintenance::setup_ap`'s cache.
pub(crate) async fn scan_networks() -> Result<Vec<Network>, String> {
    scan().await
}

/// The scan the provisioning surfaces actually serve: live if it worked,
/// cached-from-before-the-AP-rose if it did not.
///
/// The live scan fails in the exact situation the portal exists for: a phone
/// associated to our AP pins the radio to its channel, so off-channel scanning
/// returns nothing (empty on hardware 2026-08-10, with the portal rendering
/// "No networks found" to the one person the page was built for). The cache is
/// written by `setup_ap` while the radio is still free, immediately before
/// every raise — see [`crate::maintenance::setup_ap::SCAN_CACHE`].
///
/// The live attempt still goes first: on a radio with no client attached it
/// works and is fresher than any cache.
pub(crate) async fn scan_or_cached() -> Result<Vec<Network>, String> {
    let live = scan().await;
    if matches!(&live, Ok(nets) if !nets.is_empty()) {
        return live;
    }
    match std::fs::read(crate::maintenance::setup_ap::SCAN_CACHE) {
        Ok(bytes) => match serde_json::from_slice::<Vec<Network>>(&bytes) {
            Ok(nets) if !nets.is_empty() => Ok(nets),
            _ => live,
        },
        Err(_) => live,
    }
}

/// The switchover, factored out so the JSON route and the HTML portal perform
/// exactly the same sequence. Returns `None` on success, or NetworkManager's
/// own words on failure.
///
/// **Sequential, and it has to be.** Measured on the Q6A 2026-08-07: the radio
/// scans happily while hosting the AP, but cannot hold an AP and a client
/// association at once — a second virtual interface is created and adopted by
/// NetworkManager, then the join fails with "object is in an unsuitable state".
/// So the AP comes down first, and the caller loses its link to us partway
/// through. The lock stops `maintenance::setup_ap` from putting the AP back on
/// top of the association being formed.
pub(crate) async fn perform_join(ssid: &str, psk: Option<&str>) -> Option<String> {
    perform_join_full(ssid, psk, None).await
}

/// The full join, including the 802.1X branch. `identity.is_some()` selects
/// EAP; `psk` is then the account password rather than a pre-shared key.
pub(crate) async fn perform_join_full(
    ssid: &str,
    psk: Option<&str>,
    identity: Option<&str>,
) -> Option<String> {
    let _lock = ProvisioningLock::take();

    let ap_was_up = ap_is_up().await;
    if ap_was_up {
        let _ = nmcli(&["connection", "down", crate::maintenance::setup_ap::AP_CON_NAME]).await;
    }

    let joined = match identity {
        Some(user) => nmcli_join_enterprise(ssid, user, psk.unwrap_or_default()).await,
        None => nmcli_join(ssid, psk).await,
    };
    match joined {
        Some(o) if o.status.success() => None,
        Some(o) => {
            let detail = String::from_utf8_lossy(&o.stderr).trim().to_string();
            tracing::warn!(%ssid, %detail, "provision: join failed");
            // Put the AP back immediately rather than waiting for the
            // reconciler: the owner's phone is trying to get back to us right
            // now, and every second it cannot is a second they spend believing
            // they have bricked the thing.
            if ap_was_up {
                let _ =
                    nmcli(&["connection", "up", crate::maintenance::setup_ap::AP_CON_NAME]).await;
            }
            Some(if detail.is_empty() { "couldn't join that network".into() } else { detail })
        }
        None => Some("nmcli unavailable".into()),
    }
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
    match scan_or_cached().await {
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

    // The switchover itself lives in `perform_join_full`, shared with the HTML
    // portal. On success the AP stays down: the box is on the owner's network,
    // their phone rejoins its own wifi, and both meet again on the LAN to
    // finish with pairing. `setup_ap` used to re-raise it here on the next tick
    // — which, on a radio that cannot do AP+STA, knocked the box straight back
    // off the network it had just joined. Fixed 2026-08-10.
    match perform_join_full(&req.ssid, req.psk.as_deref(), req.identity.as_deref()).await {
        None => (StatusCode::OK, Json(JoinResult { ok: true, detail: None })).into_response(),
        Some(detail) => (
            StatusCode::OK,
            Json(JoinResult { ok: false, detail: Some(detail) }),
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
            "online": crate::cli::link::has_internet(),
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
        let security = fields[2].trim();
        let secured = !security.is_empty();
        let enterprise = security.contains("802.1X");
        // Strongest wins when an SSID appears on several bands/APs.
        match nets.iter_mut().find(|n| n.ssid == ssid) {
            Some(existing) if existing.signal < signal => existing.signal = signal,
            Some(_) => {}
            None => nets.push(Network { ssid, signal, secured, enterprise }),
        }
    }
    nets.sort_by(|a, b| b.signal.cmp(&a.signal));
    Ok(nets)
}

/// Split one `nmcli -t` line, honouring its `\:` escaping.
///
/// Shared with `maintenance::setup_ap`, which reads the same terse format to
/// decide whether the box has a network of its own. A connection NAME is an
/// SSID, and SSIDs contain colons.
pub(crate) fn split_terse(line: &str) -> Vec<String> {
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

/// Join an 802.1X (WPA-Enterprise) network: PEAP + MSCHAPv2, the scheme
/// credential-per-user office wifi (WeWork et al) actually runs.
///
/// Unlike the PSK path there is no one-shot `device wifi connect` for EAP, so
/// this writes a connection profile and activates it, deleting the profile on
/// failure so retries start clean.
///
/// **Certificate validation is disabled (`802-1x.system-ca-certs no`), and
/// that is a real tradeoff, made with eyes open.** Requiring CA validation
/// makes NM refuse RADIUS servers with private-CA certs — which is most
/// offices — and turns every join into a certificate-provisioning
/// conversation the setup flow cannot host. Every consumer OS offers exactly
/// this "don't validate" mode for the same reason. The exposure is an
/// evil-twin AP harvesting the MSCHAPv2 exchange during the join window;
/// the box's own credentials never ride this link. Cert pinning can come
/// later as a settings-level option.
async fn nmcli_join_enterprise(ssid: &str, identity: &str, password: &str) -> Option<std::process::Output> {
    // One profile per attempt, replaced wholesale — stale credentials in a
    // half-configured profile produce the least explicable failures NM has.
    let con_name = "virtues-enterprise";
    let _ = nmcli(&["connection", "delete", con_name]).await;
    let add = nmcli(&[
        "connection", "add", "type", "wifi", "con-name", con_name, "ssid", ssid,
        "wifi-sec.key-mgmt", "wpa-eap",
        "802-1x.eap", "peap",
        "802-1x.phase2-auth", "mschapv2",
        "802-1x.identity", identity,
        "802-1x.password", password,
        "802-1x.system-ca-certs", "no",
    ])
    .await;
    match add {
        Some(o) if o.status.success() => {}
        other => return other,
    }
    let up = nmcli(&["connection", "up", con_name]).await;
    if !matches!(&up, Some(o) if o.status.success()) {
        // Leave nothing behind: a failed profile would auto-retry with bad
        // credentials forever and lock the account out of the RADIUS server.
        let _ = nmcli(&["connection", "delete", con_name]).await;
    }
    up
}

/// SSID of the active client connection, ignoring our own AP.
pub(crate) async fn active_client_ssid() -> Option<String> {
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
    fn v4_mapped_ap_peers_are_setup_peers() {
        // THE BUG THAT CLOSED THIS DOOR ON EVERYONE. The server binds `*:8000`,
        // so a phone on the setup AP arrives as `::ffff:10.42.0.169`, matched
        // the V6 arm, and was refused — a 404 indistinguishable from the gate
        // working correctly. Verified on hardware 2026-08-10.
        let h = HeaderMap::new();
        assert!(is_setup_peer(&addr("[::ffff:10.42.0.169]:41000"), &h));
        assert!(is_setup_peer(&addr("[::ffff:127.0.0.1]:41000"), &h));
    }

    #[test]
    fn v4_mapped_lan_peers_are_still_refused() {
        // Canonicalizing must not widen the gate — only make it reachable.
        let h = HeaderMap::new();
        assert!(!is_setup_peer(&addr("[::ffff:192.168.1.44]:41000"), &h));
        assert!(!is_setup_peer(&addr("[::ffff:10.42.1.5]:41000"), &h));
    }

    #[test]
    fn v4_compatible_addresses_cannot_forge_the_subnet() {
        // `::10.42.0.169` is IPv4-COMPATIBLE, not IPv4-mapped — a deprecated
        // format no real client uses. Unwrapping it would let a caller wear an
        // AP-subnet address without being on the AP.
        let h = HeaderMap::new();
        assert!(!is_setup_peer(&addr("[::10.42.0.169]:41000"), &h));
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

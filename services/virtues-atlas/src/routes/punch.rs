//! Stateless hole-punch coordinator under `/v1/rendezvous/punch/*`.
//!
//! See `[[remote-access-decision]]` in MEMORY.md for the architectural commitment.
//! In short: atlas brokers a simultaneous-fire UDP handshake between the box
//! and a paired device so both can punch through their NATs without atlas
//! ever seeing traffic. The wall stays intact:
//!
//! - State here is **ephemeral** — `RwLock<HashMap<publish_id, PunchSlot>>`,
//!   no persistence, no Stripe/customer linkage.
//! - Each slot auto-evicts after 30 seconds via a background sweeper task.
//! - Atlas never relays packets. It hands each peer the other's reflected
//!   public address and a synchronized fire-time; the actual handshake is
//!   direct.
//!
//! ## Flow
//!
//! 1. Daemon tries direct WG handshake first via the published endpoint.
//! 2. On 3s timeout, daemon `POST /announce` with its reflected address and
//!    `my_role = device`.
//! 3. Box (on its periodic rendezvous publish loop) also `POST /announce`
//!    with `my_role = box` for the same publish_id.
//! 4. When both halves of a slot are present, atlas stamps a `fire_time`
//!    ~250 ms in the future.
//! 5. Either side `GET /peer/<publish_id>?my_role=…` and receives the
//!    matched peer's reflected address + `fire_time` (404 until both halves
//!    are present).
//! 6. At `fire_time`, both peers fire a UDP packet to the other's reflected
//!    address simultaneously. NAT mappings open in both directions; WG
//!    handshake completes over the now-symmetric path.
//! 7. Either peer `POST /complete` for cleanup + success telemetry.
//!
//! ## Privacy invariants
//!
//! Atlas sees: opaque `publish_id`s and reflected `ip:port`. It does NOT
//! see: customer identity, WG pubkeys, traffic content, byte counts. The
//! `publish_id ↔ customer` mapping has no entry in atlas's data model and
//! must never be added (this would erode the wall — see
//! `[[network-topology-star]]`).

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json};
use axum::routing::{get, post};
use axum::Router;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::routes::AppState;

/// How long after a slot's match (or first announcement, if unmatched) it
/// stays accessible. After this it's swept regardless of state.
const SLOT_TTL: Duration = Duration::from_secs(30);

/// How far in the future to schedule the simultaneous-fire instant.
/// Tuned so both peers receive the `/peer` response with enough wall-clock
/// margin to schedule their UDP send before T.
const FIRE_DELAY: Duration = Duration::from_millis(250);

/// How often the sweeper purges expired slots.
const SWEEP_INTERVAL: Duration = Duration::from_secs(5);

/// Maximum slots held in memory at once. Each slot is < 200 bytes, so 10k
/// caps memory at ~2MB even if every slot is full. New announcements
/// arriving when full are rejected with 503 (after the next 5-second sweep,
/// expired slots free up). Without this bound a single attacker can OOM
/// atlas by spamming distinct publish_ids.
const MAX_SLOTS: usize = 10_000;

/// Which side of the pair is announcing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PunchRole {
    Box,
    Device,
}

#[derive(Debug, Clone)]
struct Announcement {
    reflected_addr: String,
    received_at: Instant,
}

/// One in-flight punch coordination. A slot is fully matched when both
/// halves are Some; at that point `fire_time` is stamped.
#[derive(Debug, Clone)]
struct PunchSlot {
    box_side: Option<Announcement>,
    device_side: Option<Announcement>,
    fire_time: Option<DateTime<Utc>>,
    /// Latest `received_at` of either announcement. Used by the sweeper to
    /// age the slot out 30s after the most recent activity.
    last_touched: Instant,
}

impl PunchSlot {
    fn empty() -> Self {
        Self {
            box_side: None,
            device_side: None,
            fire_time: None,
            last_touched: Instant::now(),
        }
    }

    fn record(&mut self, role: PunchRole, ann: Announcement) {
        match role {
            PunchRole::Box => self.box_side = Some(ann.clone()),
            PunchRole::Device => self.device_side = Some(ann.clone()),
        }
        self.last_touched = ann.received_at;

        // First time both halves are present → stamp fire_time.
        if self.fire_time.is_none()
            && self.box_side.is_some()
            && self.device_side.is_some()
        {
            self.fire_time = Some(Utc::now() + chrono::Duration::from_std(FIRE_DELAY).unwrap());
        }
    }

    fn peer_for(&self, my_role: PunchRole) -> Option<&Announcement> {
        match my_role {
            PunchRole::Box => self.device_side.as_ref(),
            PunchRole::Device => self.box_side.as_ref(),
        }
    }
}

/// Shared coordinator state. Cheap to clone (Arc inside).
#[derive(Debug, Clone, Default)]
pub struct PunchCoordinator {
    inner: Arc<RwLock<HashMap<String, PunchSlot>>>,
}

impl PunchCoordinator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawn the eviction sweeper. Must be called once at startup; the
    /// returned `JoinHandle` is detached (sweeper runs forever).
    pub fn spawn_sweeper(&self) -> tokio::task::JoinHandle<()> {
        let inner = Arc::clone(&self.inner);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(SWEEP_INTERVAL).await;
                let cutoff = Instant::now() - SLOT_TTL;
                let mut map = inner.write().await;
                let before = map.len();
                map.retain(|_, slot| slot.last_touched >= cutoff);
                let after = map.len();
                if before != after {
                    tracing::debug!(
                        evicted = before - after,
                        remaining = after,
                        "punch sweeper"
                    );
                }
            }
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Wire types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct AnnounceBody {
    pub publish_id: String,
    pub my_role: PunchRole,
}

#[derive(Debug, Serialize)]
pub struct AnnounceResponse {
    /// Echo of the address the coordinator recorded (the request's socket
    /// peer, after the routability check). Useful for the daemon to confirm
    /// its NAT class against what atlas actually observed.
    pub reflected_addr: String,
}

/// Refuse non-routable IPs (unspecified, loopback, multicast, broadcast)
/// because (a) the matched peer would dial them and fail silently, and
/// (b) accepting loopback would make atlas a self-DDoS vector.
///
/// In production (`ENVIRONMENT=production`) we also reject RFC1918 / ULA
/// ranges — atlas only sees real public IPs through Caddy + XFF, so an
/// RFC1918 address there is either a misconfigured upstream or an attempt
/// to spam atlas with non-routable matches.
///
/// In dev (`ENVIRONMENT` unset, "development", "local", or anything else),
/// RFC1918 is accepted so a developer running atlas in a VPC + the box on
/// the same LAN can E2E-test the punch flow without exposing atlas to a
/// public IP. Controlled by [`is_production_env`] below.
fn is_routable_reflected(ip: IpAddr) -> bool {
    is_routable_reflected_with(ip, is_production_env())
}

/// Routability impl with the production flag explicit, so tests can exercise
/// both modes without racing on a shared env var.
fn is_routable_reflected_with(ip: IpAddr, production_strict: bool) -> bool {
    match ip {
        IpAddr::V4(v) => {
            if v.is_unspecified() || v.is_loopback() || v.is_multicast() || v.is_broadcast() {
                return false;
            }
            if production_strict && v.is_private() {
                return false;
            }
            true
        }
        IpAddr::V6(v) => {
            if v.is_unspecified() || v.is_loopback() || v.is_multicast() {
                return false;
            }
            if production_strict && is_unique_local_v6(&v) {
                return false;
            }
            true
        }
    }
}

/// True when the deployment is production-strict. Matches the standard
/// `ENVIRONMENT` env var ("production"). Any other value (or unset) is
/// dev/test → permissive (accept RFC1918 + ULA).
fn is_production_env() -> bool {
    std::env::var("ENVIRONMENT")
        .map(|s| s.eq_ignore_ascii_case("production"))
        .unwrap_or(false)
}

/// IPv6 unique-local range (fc00::/7). `Ipv6Addr::is_unique_local` is a
/// nightly-only API as of stable Rust today, so we open-code the check.
fn is_unique_local_v6(v: &std::net::Ipv6Addr) -> bool {
    (v.segments()[0] & 0xfe00) == 0xfc00
}

#[derive(Debug, Deserialize)]
pub struct PeerQuery {
    pub my_role: PunchRole,
}

#[derive(Debug, Serialize)]
pub struct PeerResponse {
    pub peer_reflected_addr: String,
    pub fire_time: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CompleteBody {
    pub publish_id: String,
    pub success: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Router + handlers
// ─────────────────────────────────────────────────────────────────────────────

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/rendezvous/punch/announce", post(announce))
        .route("/v1/rendezvous/punch/peer/:publish_id", get(peer))
        .route("/v1/rendezvous/punch/complete", post(complete))
}

async fn announce(
    State(state): State<AppState>,
    ConnectInfo(socket_peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<AnnounceBody>,
) -> impl IntoResponse {
    if body.publish_id.is_empty() {
        return err(StatusCode::BAD_REQUEST, "empty publish_id");
    }
    if body.publish_id.len() > 128 {
        return err(StatusCode::BAD_REQUEST, "publish_id too long");
    }

    // Atlas runs behind Caddy on EC2: the socket peer atlas sees is Caddy's
    // loopback. The real client IP is in `X-Forwarded-For` (Caddy sets this
    // — we trust the FIRST entry because Caddy strips any client-asserted
    // upstream chain before re-adding its own). Falls back to socket_peer
    // when there's no proxy in front (local dev).
    let observed_ip = observed_client_ip(&headers, &socket_peer);

    // Source-of-truth observation. Client-asserted addresses were dropped
    // in v0.2 because (a) they enabled punch poisoning, and (b) they let
    // atlas double as a UDP reflector toward arbitrary IPs. We use what
    // we observed; the matched peer will dial THAT address.
    if !is_routable_reflected(observed_ip) {
        return err(
            StatusCode::BAD_REQUEST,
            "observed client IP is not a public routable address",
        );
    }
    // Use the socket peer's port — that's the port the source kernel chose
    // for this connection. (Caddy preserves the upstream port for `:80/:443`
    // termination but not for HTTPS upstream connections to atlas, so this
    // is the source port AS SEEN BY ATLAS, not by the original client. For
    // the punch use case that's still useful as a one-time-use token.)
    let reflected_addr = SocketAddr::new(observed_ip, socket_peer.port()).to_string();

    let ann = Announcement {
        reflected_addr: reflected_addr.clone(),
        received_at: Instant::now(),
    };

    let mut map = state.punch.inner.write().await;

    // Bound memory under spam: refuse new slots when full. Updates to an
    // already-existing publish_id are always allowed since they don't grow
    // the map. The sweeper (5s wakeups) reclaims expired entries.
    if !map.contains_key(&body.publish_id) && map.len() >= MAX_SLOTS {
        drop(map);
        tracing::warn!(
            slots = MAX_SLOTS,
            "punch coordinator at capacity; rejecting new announcement"
        );
        return err(
            StatusCode::SERVICE_UNAVAILABLE,
            "coordinator at capacity; retry in a moment",
        );
    }

    let slot = map
        .entry(body.publish_id.clone())
        .or_insert_with(PunchSlot::empty);
    slot.record(body.my_role, ann);

    drop(map);

    (
        StatusCode::OK,
        Json(AnnounceResponse { reflected_addr }),
    )
        .into_response()
}

/// Resolve the real client IP. Trust `X-Forwarded-For` (Caddy strips
/// caller-supplied values before adding its own — our deployment doc tracks
/// this). Falls back to the socket peer for direct connections (local dev,
/// future direct-EC2 deploys).
fn observed_client_ip(headers: &HeaderMap, socket_peer: &SocketAddr) -> IpAddr {
    if let Some(xff) = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        if let Ok(ip) = xff.parse::<IpAddr>() {
            return ip;
        }
        // Fallthrough — XFF was set but unparseable; trust the socket
        // rather than 502ing the user.
    }
    socket_peer.ip()
}

async fn peer(
    State(state): State<AppState>,
    Path(publish_id): Path<String>,
    Query(q): Query<PeerQuery>,
) -> impl IntoResponse {
    let map = state.punch.inner.read().await;
    let Some(slot) = map.get(&publish_id) else {
        return err(StatusCode::NOT_FOUND, "no slot for this publish_id");
    };

    let Some(peer_ann) = slot.peer_for(q.my_role) else {
        return err(StatusCode::NOT_FOUND, "peer not yet announced");
    };

    let Some(fire_time) = slot.fire_time else {
        // Both sides aren't matched yet (shouldn't happen given peer_ann
        // exists, but guard anyway in case of races).
        return err(StatusCode::NOT_FOUND, "match pending");
    };

    (
        StatusCode::OK,
        Json(PeerResponse {
            peer_reflected_addr: peer_ann.reflected_addr.clone(),
            fire_time,
        }),
    )
        .into_response()
}

async fn complete(Json(body): Json<CompleteBody>) -> impl IntoResponse {
    // Telemetry only — slot cleanup is the sweeper's job. This endpoint is
    // intentionally non-destructive because it's unauthenticated; if we
    // removed the slot here, anyone who learned a `publish_id` (e.g. an
    // on-path observer between a paired device and atlas) could wipe a
    // live punch by guessing it. The 30 s sweeper handles eviction either
    // way, so removal here would be redundant at best and weaponizable at
    // worst.
    if body.success {
        tracing::debug!(publish_id = %body.publish_id, "punch reported success");
    } else {
        tracing::info!(publish_id = %body.publish_id, "punch reported failure");
    }
    StatusCode::OK.into_response()
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn err(status: StatusCode, msg: &str) -> axum::response::Response {
    (
        status,
        Json(serde_json::json!({
            "error": { "code": status.canonical_reason().unwrap_or("error"), "message": msg }
        })),
    )
        .into_response()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests — pure coordinator state (no Axum / HTTP). Endpoint tests would
// need to spin up the full AppState; the state-machine tests below cover
// the load-bearing matching + eviction logic.
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn ann(addr: &str) -> Announcement {
        Announcement {
            reflected_addr: addr.to_string(),
            received_at: Instant::now(),
        }
    }

    #[test]
    fn routability_dev_accepts_rfc1918() {
        let lan_v4: IpAddr = "10.0.0.5".parse().unwrap();
        let lan_v6: IpAddr = "fd00::1".parse().unwrap();
        // dev (production_strict=false): LAN ranges are allowed so a dev box
        // and atlas on the same VPC can E2E-test punch.
        assert!(is_routable_reflected_with(lan_v4, false));
        assert!(is_routable_reflected_with(lan_v6, false));
    }

    #[test]
    fn routability_production_rejects_rfc1918() {
        let lan_v4: IpAddr = "10.0.0.5".parse().unwrap();
        let lan_v6: IpAddr = "fd00::1".parse().unwrap();
        assert!(!is_routable_reflected_with(lan_v4, true));
        assert!(!is_routable_reflected_with(lan_v6, true));
    }

    #[test]
    fn routability_rejects_loopback_and_unspecified_always() {
        let lo: IpAddr = "127.0.0.1".parse().unwrap();
        let zero: IpAddr = "0.0.0.0".parse().unwrap();
        // Both modes reject — these are never legitimate punch endpoints.
        for strict in [false, true] {
            assert!(!is_routable_reflected_with(lo, strict));
            assert!(!is_routable_reflected_with(zero, strict));
        }
    }

    #[test]
    fn routability_production_accepts_public_v4() {
        let public: IpAddr = "12.34.56.78".parse().unwrap();
        assert!(is_routable_reflected_with(public, true));
    }

    #[tokio::test]
    async fn first_announcement_is_unmatched() {
        let coord = PunchCoordinator::new();
        let mut map = coord.inner.write().await;
        let slot = map.entry("PID".into()).or_insert_with(PunchSlot::empty);
        slot.record(PunchRole::Box, ann("10.0.0.5:51820"));

        // No fire_time until the second side shows up.
        assert!(slot.fire_time.is_none());

        // Box asking from its POV: device hasn't announced yet.
        assert!(slot.peer_for(PunchRole::Box).is_none());

        // Device asking from its POV: the box IS visible at the slot level
        // (its announcement is recorded). The /peer endpoint still 404s
        // because fire_time isn't set yet — see endpoint logic — but the
        // raw slot state shows the box has been seen.
        assert!(slot.peer_for(PunchRole::Device).is_some());
    }

    #[tokio::test]
    async fn second_announcement_completes_match() {
        let coord = PunchCoordinator::new();
        let mut map = coord.inner.write().await;
        let slot = map.entry("PID".into()).or_insert_with(PunchSlot::empty);

        slot.record(PunchRole::Box, ann("10.0.0.5:51820"));
        assert!(slot.fire_time.is_none());

        slot.record(PunchRole::Device, ann("203.0.113.7:62000"));
        assert!(slot.fire_time.is_some(), "fire_time should be set on second arrival");

        let device_view = slot.peer_for(PunchRole::Device).unwrap();
        assert_eq!(device_view.reflected_addr, "10.0.0.5:51820");

        let box_view = slot.peer_for(PunchRole::Box).unwrap();
        assert_eq!(box_view.reflected_addr, "203.0.113.7:62000");
    }

    #[tokio::test]
    async fn re_announcing_same_role_refreshes() {
        let coord = PunchCoordinator::new();
        let mut map = coord.inner.write().await;
        let slot = map.entry("PID".into()).or_insert_with(PunchSlot::empty);

        slot.record(PunchRole::Box, ann("10.0.0.5:51820"));
        slot.record(PunchRole::Box, ann("10.0.0.5:51999"));

        // Most recent announcement wins.
        assert_eq!(slot.box_side.as_ref().unwrap().reflected_addr, "10.0.0.5:51999");
    }

    #[tokio::test]
    async fn fire_time_is_stable_once_set() {
        let coord = PunchCoordinator::new();
        let mut map = coord.inner.write().await;
        let slot = map.entry("PID".into()).or_insert_with(PunchSlot::empty);

        slot.record(PunchRole::Box, ann("10.0.0.5:51820"));
        slot.record(PunchRole::Device, ann("203.0.113.7:62000"));
        let t1 = slot.fire_time;
        // Subsequent activity should NOT move the fire_time — both peers
        // are racing toward T and re-stamping would desync them.
        slot.record(PunchRole::Box, ann("10.0.0.5:51821"));
        let t2 = slot.fire_time;
        assert_eq!(t1, t2, "fire_time must not change after first match");
    }

    #[tokio::test]
    async fn sweeper_evicts_expired_slots() {
        let coord = PunchCoordinator::new();
        {
            let mut map = coord.inner.write().await;
            let slot = map.entry("OLD".into()).or_insert_with(PunchSlot::empty);
            // Fake an ancient announcement by hand.
            slot.box_side = Some(Announcement {
                reflected_addr: "10.0.0.5:51820".into(),
                received_at: Instant::now() - SLOT_TTL - Duration::from_secs(1),
            });
            slot.last_touched = slot.box_side.as_ref().unwrap().received_at;

            let fresh = map.entry("FRESH".into()).or_insert_with(PunchSlot::empty);
            fresh.record(PunchRole::Box, ann("10.0.0.5:51820"));
        }

        // Imitate the sweeper inline (we don't want to wait SWEEP_INTERVAL
        // in tests).
        let cutoff = Instant::now() - SLOT_TTL;
        let mut map = coord.inner.write().await;
        let before = map.len();
        map.retain(|_, slot| slot.last_touched >= cutoff);
        let after = map.len();
        assert_eq!(before - after, 1);
        assert!(map.contains_key("FRESH"));
        assert!(!map.contains_key("OLD"));
    }

    #[tokio::test]
    async fn coordinator_cap_blocks_new_slots() {
        // Stress the gating logic inline. Real handler does the same check.
        let coord = PunchCoordinator::new();
        {
            let mut map = coord.inner.write().await;
            for i in 0..MAX_SLOTS {
                let slot = map
                    .entry(format!("pid-{i}"))
                    .or_insert_with(PunchSlot::empty);
                slot.record(PunchRole::Box, ann("10.0.0.5:51820"));
            }
            assert_eq!(map.len(), MAX_SLOTS);
        }

        // New publish_id when full → rejected
        let map = coord.inner.read().await;
        let new_id = "pid-overflow";
        let would_block = !map.contains_key(new_id) && map.len() >= MAX_SLOTS;
        assert!(would_block, "new slot must be blocked when full");

        // Existing publish_id update → still allowed
        let existing = "pid-0";
        let would_block = !map.contains_key(existing) && map.len() >= MAX_SLOTS;
        assert!(!would_block, "update to existing slot must always work");
    }

    #[tokio::test]
    async fn role_serializes_snake_case() {
        let j = serde_json::to_string(&PunchRole::Box).unwrap();
        assert_eq!(j, "\"box\"");
        let j = serde_json::to_string(&PunchRole::Device).unwrap();
        assert_eq!(j, "\"device\"");
    }
}

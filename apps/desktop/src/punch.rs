//! Daemon-side NAT hole-punch coordinator client.
//!
//! Talks to atlas's `/v1/rendezvous/punch/*` endpoints to negotiate a
//! simultaneous-fire UDP punch with the box when direct WG dial fails.
//! Architecture in [[remote-access-decision]] and [[network-topology-star]];
//! coordinator-side code in `services/virtues-atlas/src/routes/punch.rs`.
//!
//! ## Flow this module implements
//!
//! 1. Discover our reflected `ip:port` (current path: ask atlas, which sees
//!    the request's socket peer — equivalent to a one-shot STUN bind).
//! 2. POST `/v1/rendezvous/punch/announce` with `my_role = device`.
//! 3. Poll `/v1/rendezvous/punch/peer/{publish_id}?my_role=device` until
//!    200 (atlas waits for the box to announce its side).
//! 4. Sleep until the `fire_time` returned by atlas.
//! 5. Send a single UDP packet to the peer's reflected address (opens our
//!    NAT mapping). The box does the same in the other direction at T;
//!    handshake then completes via the new bidirectional path.
//! 6. POST `/v1/rendezvous/punch/complete` with `success: true|false`.
//!
//! ## When to use this
//!
//! Only when the direct WG handshake from [`crate::tunnel`] hasn't completed
//! within a few seconds. Direct dial succeeds for ~70% of networks; this
//! adds ~20%. The remaining ~10% (strict symmetric NAT both sides, UDP
//! blocked, hostile MDM) cannot punch and falls into the "unsupported
//! network" bucket — see [[nat-reality]].

use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tokio::time::Instant;

/// Maximum total time we'll keep polling atlas for the box to announce.
/// After this we give up and surface "punch failed."
const POLL_DEADLINE: Duration = Duration::from_secs(15);

/// How long to wait between `/peer` polls.
const POLL_INTERVAL: Duration = Duration::from_millis(750);

/// Single UDP byte we fire as the punch packet. Content doesn't matter —
/// only that an outbound packet to the peer's reflected address opens our
/// NAT mapping.
const PUNCH_PAYLOAD: &[u8] = &[0u8; 1];

/// Outcome of a punch attempt. Used by [`crate::tunnel`] to decide whether
/// to retry the WG handshake (success) or surface "tunnel down" (failure).
#[derive(Debug, Clone)]
pub struct PunchOutcome {
    pub peer_reflected_addr: SocketAddr,
    pub fired_at: DateTime<Utc>,
}

/// Atlas client configuration. Just the base URL — endpoints are appended.
#[derive(Debug, Clone)]
pub struct AtlasClient {
    pub base_url: String,
    pub http: reqwest::Client,
}

impl AtlasClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .expect("reqwest client"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Wire types — must mirror the atlas-side shapes in
// services/virtues-atlas/src/routes/punch.rs.
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct AnnounceBody<'a> {
    publish_id: &'a str,
    my_role: &'a str,
}

#[derive(Debug, Deserialize)]
struct AnnounceResp {
    reflected_addr: String,
}

#[derive(Debug, Deserialize)]
struct PeerResp {
    peer_reflected_addr: String,
    fire_time: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct CompleteBody<'a> {
    publish_id: &'a str,
    success: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Run the full punch dance for `publish_id`. Returns once we've fired our
/// punch packet at `fire_time`; the caller retries the WG handshake.
///
/// `udp_socket` is the same UDP socket the WG transport will use after the
/// punch — sending from it opens the NAT mapping the box will receive
/// future packets through. If we punched from a different socket the NAT
/// would map a *different* outbound port and the box's response would
/// land in the wrong mapping.
pub async fn coordinate(
    atlas: &AtlasClient,
    publish_id: &str,
    udp_socket: &UdpSocket,
) -> Result<PunchOutcome> {
    // 1. Announce. Atlas observes our reflected `ip:port` from the request
    //    socket peer — we don't assert it, because client-asserted addresses
    //    enabled punch poisoning and turned atlas into a UDP reflector.
    //
    //    A caveat to acknowledge: atlas's observation is the daemon's HTTPS
    //    SOCKET peer, which is a different NAT mapping than the UDP socket
    //    GotaTun uses for WG. On symmetric NATs the external port for the
    //    UDP path differs from the HTTPS one, and punch won't work — that's
    //    the ~10% of networks documented in [[nat-reality]]. Fixing it
    //    properly needs the shared-UDP-transport work tracked as P0-5.
    let _ = udp_socket; // reserved for the P0-5 shared-socket rewire

    let _ann: AnnounceResp = post(
        &atlas.http,
        &format!("{}/v1/rendezvous/punch/announce", atlas.base_url),
        &AnnounceBody {
            publish_id,
            my_role: "device",
        },
    )
    .await
    .context("POST /punch/announce")?;

    // 2. Poll /peer until atlas has both sides matched.
    let peer = poll_for_peer(atlas, publish_id).await?;
    let peer_addr: SocketAddr = peer
        .peer_reflected_addr
        .parse()
        .with_context(|| format!("parse peer addr `{}`", peer.peer_reflected_addr))?;

    // 3. Sleep until fire_time, accounting for any small clock skew.
    let now = Utc::now();
    let delta = (peer.fire_time - now).to_std().unwrap_or(Duration::ZERO);
    tokio::time::sleep_until(Instant::now() + delta).await;

    // 4. Fire. A single UDP byte to peer_addr from our chosen socket.
    udp_socket
        .send_to(PUNCH_PAYLOAD, peer_addr)
        .await
        .with_context(|| format!("send punch packet to {peer_addr}"))?;
    tracing::info!(peer = %peer_addr, "punch fired");

    // 5. Telemetry. Don't fail the punch if /complete is unreachable.
    if let Err(e) = post::<_, serde_json::Value>(
        &atlas.http,
        &format!("{}/v1/rendezvous/punch/complete", atlas.base_url),
        &CompleteBody {
            publish_id,
            success: true,
        },
    )
    .await
    {
        tracing::debug!("punch /complete failed (non-fatal): {e:#}");
    }

    Ok(PunchOutcome {
        peer_reflected_addr: peer_addr,
        fired_at: peer.fire_time,
    })
}

/// Tell atlas the punch attempt failed (the WG handshake didn't come up
/// after the fire). Used by [`crate::tunnel`] when retries time out.
/// Best-effort — failures here are logged at debug, not surfaced.
pub async fn report_failure(atlas: &AtlasClient, publish_id: &str) {
    let body = CompleteBody {
        publish_id,
        success: false,
    };
    if let Err(e) = post::<_, serde_json::Value>(
        &atlas.http,
        &format!("{}/v1/rendezvous/punch/complete", atlas.base_url),
        &body,
    )
    .await
    {
        tracing::debug!("report_failure failed (non-fatal): {e:#}");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Internals
// ─────────────────────────────────────────────────────────────────────────────

async fn poll_for_peer(atlas: &AtlasClient, publish_id: &str) -> Result<PeerResp> {
    let deadline = Instant::now() + POLL_DEADLINE;
    loop {
        if Instant::now() >= deadline {
            bail!("punch coordinator: timed out waiting for peer ({POLL_DEADLINE:?})");
        }

        let url = format!(
            "{}/v1/rendezvous/punch/peer/{publish_id}?my_role=device",
            atlas.base_url
        );
        let resp = atlas
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;

        match resp.status() {
            reqwest::StatusCode::OK => {
                return resp.json::<PeerResp>().await.context("decode peer response");
            }
            reqwest::StatusCode::NOT_FOUND => {
                tokio::time::sleep(POLL_INTERVAL).await;
            }
            other => {
                let body = resp.text().await.unwrap_or_default();
                bail!("punch coordinator unexpected {other}: {body}");
            }
        }
    }
}

async fn post<B, R>(http: &reqwest::Client, url: &str, body: &B) -> Result<R>
where
    B: Serialize,
    R: serde::de::DeserializeOwned,
{
    let resp = http
        .post(url)
        .json(body)
        .send()
        .await
        .with_context(|| format!("POST {url}"))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("POST {url}: {status} — {body}"));
    }
    let parsed: R = resp.json().await.context("decode response")?;
    Ok(parsed)
}

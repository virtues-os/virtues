//! `POST /v1/net/probe` — inbound-reachability echo (IPv6-direct doctrine).
//!
//! A box self-tests whether it's reachable from the internet: it binds a UDP
//! socket, POSTs us its port + a nonce, and we fire ONE UDP datagram with that
//! nonce back at the REQUESTER's own observed source address. The box confirms
//! receipt locally and reports "reachable". A box can't test its own firewall
//! from inside — only an external party can — and this is the smallest, dumbest
//! such party: it fires one packet and remembers nothing.
//!
//! Safe to leave unauthenticated: we only ever send a packet to the address WE
//! OBSERVED on the request (`X-Forwarded-For`, set by Caddy) — never an address
//! from the body — so a caller can only make us send a tiny packet to ITSELF.
//! No reflection toward a third party, no amplification (one ~16-byte nonce).
//! We store nothing and forward nothing: a dumb echo, NOT a relay.
//!
//! For the doctrine's primary (IPv6) path the box must reach us over IPv6 so we
//! observe its global v6 source — which requires this service to have an AAAA.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::net::UdpSocket;

use crate::AppState;

#[derive(Deserialize)]
pub struct ProbeBody {
    /// The UDP port the box is listening on for the nonce.
    pub port: u16,
    /// Opaque token the box generated; we echo it verbatim so the box can match
    /// the datagram to this request. 1..=128 bytes.
    pub nonce: String,
}

#[derive(Serialize)]
struct ProbeResp {
    /// The source address we observed and fired at (the caller's own).
    fired_at: String,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/v1/net/probe", post(probe))
}

async fn probe(
    State(_state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<ProbeBody>,
) -> axum::response::Response {
    if body.nonce.is_empty() || body.nonce.len() > 128 {
        return err(StatusCode::BAD_REQUEST, "nonce must be 1..=128 bytes");
    }
    if body.port < 1024 {
        return err(StatusCode::BAD_REQUEST, "port must be >= 1024");
    }

    // Observe the caller's real address from XFF (Caddy sets it). We fire ONLY
    // at this — never a body-supplied address — so there is no reflection
    // vector. virtues-api always sits behind Caddy, so there's no ConnectInfo.
    let Some(ip) = observed_client_ip(&headers) else {
        return err(StatusCode::BAD_REQUEST, "could not determine source address");
    };
    if !is_routable(ip) {
        return err(StatusCode::BAD_REQUEST, "source is not a public routable address");
    }

    // Fire one UDP datagram with the nonce at the caller's own ip:port.
    let bind = if ip.is_ipv6() { "[::]:0" } else { "0.0.0.0:0" };
    if let Ok(sock) = UdpSocket::bind(bind).await {
        let _ = sock
            .send_to(body.nonce.as_bytes(), SocketAddr::new(ip, body.port))
            .await;
    }

    (StatusCode::OK, Json(ProbeResp { fired_at: ip.to_string() })).into_response()
}

fn err(status: StatusCode, msg: &str) -> axum::response::Response {
    (status, Json(json!({ "error": { "message": msg } }))).into_response()
}

/// First entry of `X-Forwarded-For` (Caddy strips any client-asserted chain and
/// re-adds its own observed peer, so the first entry is the real client).
fn observed_client_ip(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse::<IpAddr>().ok())
}

/// Public routable address — reject loopback/unspecified/multicast/link-local,
/// IPv6 ULA, IPv4 private/CGNAT/broadcast (we won't fire at a non-routable
/// address even though it can only ever be the caller's own).
fn is_routable(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let o = v.octets();
            let cgnat = o[0] == 100 && (o[1] & 0xc0) == 0x40;
            !v.is_loopback()
                && !v.is_unspecified()
                && !v.is_private()
                && !v.is_link_local()
                && !v.is_broadcast()
                && !v.is_multicast()
                && !cgnat
        }
        IpAddr::V6(v) => {
            let seg0 = v.segments()[0];
            !v.is_loopback()
                && !v.is_unspecified()
                && !v.is_multicast()
                && (seg0 & 0xffc0) != 0xfe80
                && (seg0 & 0xfe00) != 0xfc00
        }
    }
}

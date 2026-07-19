use anyhow::{Context, Result};
use iroh::endpoint::Connection;
use iroh::{Endpoint, EndpointAddr, EndpointId, RelayUrl};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::Mutex;

use crate::endpoint::VIRTUES_ALPN;

/// Max response body we'll buffer from a single request stream (64 MiB).
const MAX_RESPONSE: usize = 64 * 1024 * 1024;

/// Which network path the connection to the box is using right now — for a
/// live "Direct · LAN / Relay / Offline" status readout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    /// Direct peer-to-peer (LAN or hole-punched) — an IP transport addr is live.
    Direct,
    /// Reached via the relay — only a relay addr is live.
    Relay,
    /// No live path (nothing connected / box unreachable).
    Offline,
}

impl PathKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            PathKind::Direct => "direct",
            PathKind::Relay => "relay",
            PathKind::Offline => "offline",
        }
    }
}

/// A client holding a warm iroh `Endpoint` + reconnecting `Connection` to the
/// box. Dials an [`EndpointAddr`] — which may carry a relay URL (remote reach),
/// direct IP addresses (LAN-direct), or both; iroh negotiates the best path and
/// upgrades to hole-punched direct when possible. Used by the desktop `:7117`
/// helper and, via C-FFI, iOS `BoxTransport`.
pub struct VirtuesIrohClient {
    endpoint: Endpoint,
    addr: EndpointAddr,
    conn: Mutex<Option<Connection>>,
}

impl VirtuesIrohClient {
    /// Dial an explicit [`EndpointAddr`] (relay and/or direct addrs).
    pub fn new(endpoint: Endpoint, addr: EndpointAddr) -> Self {
        Self { endpoint, addr, conn: Mutex::new(None) }
    }

    /// Convenience: reach the box by `EndpointId` via our relay (the common
    /// remote case; direct paths get discovered + upgraded after connecting).
    pub fn from_relay(endpoint: Endpoint, box_id: EndpointId, relay_url: RelayUrl) -> Self {
        Self::new(endpoint, EndpointAddr::new(box_id).with_relay_url(relay_url))
    }

    /// Convenience: reach the box by `EndpointId` at explicit direct addresses
    /// (LAN-direct). No relay — pure peer-to-peer on the local network. Pair
    /// this with [`build_endpoint`](crate::build_endpoint)`(secret, None, None)`
    /// for a zero-third-party dial to an unclaimed box on the same network.
    pub fn from_direct(
        endpoint: Endpoint,
        box_id: EndpointId,
        direct_addrs: impl IntoIterator<Item = std::net::SocketAddr>,
    ) -> Self {
        let mut addr = EndpointAddr::new(box_id);
        for a in direct_addrs {
            addr = addr.with_ip_addr(a);
        }
        Self::new(endpoint, addr)
    }

    async fn dial(&self) -> Result<Connection> {
        self.endpoint
            .connect(self.addr.clone(), VIRTUES_ALPN)
            .await
            .context("dial box over iroh")
    }

    async fn connection(&self) -> Result<Connection> {
        let mut guard = self.conn.lock().await;
        if let Some(c) = guard.clone() {
            return Ok(c);
        }
        let c = self.dial().await?;
        *guard = Some(c.clone());
        Ok(c)
    }

    async fn drop_connection(&self) {
        *self.conn.lock().await = None;
    }

    /// Drop the cached connection so the next request re-dials fresh. Call on a
    /// network change (LTE↔Wi-Fi/LAN) to recover promptly instead of waiting for
    /// a request to fail on the stale connection first.
    pub async fn drop_conn(&self) {
        self.drop_connection().await;
    }

    /// Poke iroh to re-check the network — rebind UDP sockets, re-run net-report,
    /// reconnect the relay. iroh detects most changes itself, but on iOS the
    /// socket can silently die on suspend/network-switch and iroh's one-shot
    /// rebind can fail (iroh#4289); calling this on every NWPathMonitor / foreground
    /// event heals the common case. Idempotent — safe to call liberally.
    pub async fn network_change(&self) {
        self.endpoint.network_change().await;
    }

    /// Snapshot which path the connection to the box is using *right now*, from
    /// live iroh state. `Offline` if nothing is connected — call after a request
    /// (which dials) for a fresh reading. Prefers `Direct` when both are live
    /// (iroh upgrades relay→direct after hole-punching).
    pub async fn path_kind(&self) -> PathKind {
        use iroh::endpoint::TransportAddrUsage;
        let Some(info) = self.endpoint.remote_info(self.addr.id).await else {
            return PathKind::Offline;
        };
        let mut relay = false;
        for a in info.addrs() {
            if !matches!(a.usage(), TransportAddrUsage::Active) {
                continue;
            }
            if a.addr().is_ip() {
                return PathKind::Direct;
            }
            if a.addr().is_relay() {
                relay = true;
            }
        }
        if relay {
            PathKind::Relay
        } else {
            PathKind::Offline
        }
    }

    /// Send a raw HTTP/1 request over a fresh bi-stream and return the raw
    /// HTTP/1 response bytes. Transparently redials once if the warm connection
    /// has gone stale (network change, box restart, relay hiccup).
    pub async fn request(&self, raw_http: &[u8]) -> Result<Vec<u8>> {
        // Open a bi-stream on the warm connection; if that fails (stale/dead
        // cached connection), drop it, redial ONCE, and reopen. The retry covers
        // ONLY stream setup — never after the request bytes are written — so a
        // non-idempotent request is never executed twice on the box.
        let opened = match self.connection().await?.open_bi().await {
            Ok(streams) => Ok(streams),
            Err(_) => {
                self.drop_connection().await;
                self.connection().await?.open_bi().await
            }
        };
        let (mut send, mut recv) = opened.context("open_bi")?;
        send.write_all(raw_http).await.context("write request")?;
        // Do NOT finish() the send half before reading: on a bidirectional QUIC
        // stream that FIN reads as "peer closed" to the server's hyper, which
        // then aborts before responding. hyper completes the request from its
        // headers (Content-Length / no body) and, with `Connection: close`,
        // finishes its own send half after the response — the EOF we read to.
        let resp = recv.read_to_end(MAX_RESPONSE).await.context("read response")?;
        let _ = send.finish(); // now safe: response is in hand
        Ok(resp)
    }

    /// Splice a local byte stream (e.g. a browser TCP connection) to the box over
    /// a fresh iroh bi-stream, copying in both directions until either side ends.
    /// HTTP/1 keep-alive is preserved end-to-end because the box serves each
    /// bi-stream as a hyper connection. Used by the desktop `:7117` helper:
    /// one inbound TCP connection ⇄ one iroh bi-stream.
    pub async fn proxy_stream<S>(&self, io: &mut S) -> Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin + ?Sized,
    {
        // On ANY failure, drop the cached connection so the NEXT inbound
        // connection redials a fresh one — otherwise a single dead box
        // connection (restart / network change / relay hiccup) would wedge the
        // whole `:7117` helper, since every proxied connection shares this one
        // warm Connection.
        let (send, recv) = match self.connection().await?.open_bi().await {
            Ok(streams) => streams,
            Err(e) => {
                self.drop_connection().await;
                return Err(anyhow::Error::new(e)).context("open_bi");
            }
        };
        let mut box_io = tokio::io::join(recv, send);
        if let Err(e) = tokio::io::copy_bidirectional(io, &mut box_io).await {
            self.drop_connection().await;
            return Err(e).context("proxy copy_bidirectional");
        }
        Ok(())
    }

    /// Graceful shutdown — flush the QUIC close frame before exit.
    pub async fn close(self) {
        self.endpoint.close().await;
    }

    /// Graceful shutdown by shared reference — for `Arc`-managed callers (the
    /// uniffi/iOS FFI wrapper holds the client behind an `Arc` and can't consume
    /// it). `Endpoint::close` is idempotent, so this is safe to call once on the
    /// last handle drop. Native callers that own the client use [`close`] instead.
    pub async fn shutdown(&self) {
        self.endpoint.close().await;
    }
}

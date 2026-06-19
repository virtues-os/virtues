//! The WireGuard data plane: wraps `defguard_boringtun`'s packet-level `Tunn`
//! and a UDP socket to the box. Knows nothing about TCP — it converts between
//! plaintext IP packets (the netstack's currency) and encrypted WG datagrams
//! (the wire), and drives the Noise handshake + keepalive timers.

use std::net::{SocketAddr, UdpSocket};

use defguard_boringtun::noise::{Tunn, TunnResult};
use defguard_boringtun::x25519::{PublicKey, StaticSecret};
use virtues_protocol::WgParams;

use crate::keys::decode_key_b64;
use crate::TunnelError;

/// WG persistent keepalive, in seconds. Matches the desktop tunnel; keeps the
/// box's NAT/stateful-firewall binding alive and surfaces silent drops.
const KEEPALIVE_SECS: u16 = 25;

/// Max plaintext IP packet we handle (the tunnel MTU). The encrypted form adds
/// up to 32 bytes of WG overhead, so scratch buffers are sized MTU + 64.
pub(crate) const TUNNEL_MTU: usize = 1280;
const SCRATCH: usize = TUNNEL_MTU + 64;

pub(crate) struct WgTunnel {
    tunn: Tunn,
    udp: UdpSocket,
    /// Candidate endpoints to try, best-first (box LAN address(es) + global v6).
    /// The event loop cycles these until a handshake lands, then locks on; on a
    /// later expiry it restarts from index 0. Always non-empty.
    candidates: Vec<SocketAddr>,
    /// Index into `candidates` of the path we're currently handshaking against.
    current: usize,
    /// Reusable encrypt/decrypt destination buffer.
    scratch: Box<[u8]>,
    /// Material to rebuild `tunn` after boringtun marks it expired (it can't
    /// un-expire; a fresh `Tunn` is the only way to re-handshake).
    priv_bytes: [u8; 32],
    server_pub_bytes: [u8; 32],
    psk: [u8; 32],
}

/// Whether two endpoints need different socket binds (one v4, one v6). A
/// same-family switch can re-point the existing socket; a cross-family switch
/// needs a fresh bind.
fn family_differs(a: SocketAddr, b: SocketAddr) -> bool {
    a.is_ipv6() != b.is_ipv6()
}

/// Build a fresh `Tunn` from raw key material (used at construction and on
/// re-handshake after expiry).
fn new_tunn(priv_bytes: [u8; 32], server_pub_bytes: [u8; 32], psk: [u8; 32]) -> Tunn {
    Tunn::new(
        StaticSecret::from(priv_bytes),
        PublicKey::from(server_pub_bytes),
        Some(psk),
        Some(KEEPALIVE_SECS),
        0,
        None,
    )
}

impl WgTunnel {
    /// Build the data plane from a pairing bundle + the device's base64 private
    /// key. Binds a UDP socket for the first candidate and resolves the endpoint
    /// list, but does not handshake yet — call [`initiate`](Self::initiate).
    pub(crate) fn new(wg: &WgParams, private_key_b64: &str) -> Result<Self, TunnelError> {
        let priv_raw = decode_key_b64(private_key_b64)?;
        let server_pub_raw = decode_key_b64(&wg.server_public_key)?;
        let psk = decode_key_b64(&wg.preshared_key)?;

        // Candidate list: prefer the multi-endpoint field, then ensure the
        // single `server_endpoint` is present as a fallback (older boxes only
        // set that one). Skip unparseable entries rather than failing the whole
        // tunnel. Order is preserved (best-first as the box ranked them).
        let mut candidates: Vec<SocketAddr> = Vec::new();
        let mut push = |raw: &str| {
            if raw.is_empty() {
                return;
            }
            match raw.parse::<SocketAddr>() {
                Ok(addr) if !candidates.contains(&addr) => candidates.push(addr),
                Ok(_) => {}
                Err(e) => tracing::debug!("skipping unparseable endpoint '{raw}': {e}"),
            }
        };
        for ep in &wg.server_endpoints {
            push(ep);
        }
        push(&wg.server_endpoint);
        if candidates.is_empty() {
            return Err(TunnelError::BadBundle(format!(
                "no parseable endpoint in bundle (server_endpoint='{}', server_endpoints={:?})",
                wg.server_endpoint, wg.server_endpoints
            )));
        }

        let tunn = new_tunn(priv_raw, server_pub_raw, psk);
        let udp = Self::bind_for(&candidates[0])?;

        Ok(Self {
            tunn,
            udp,
            candidates,
            current: 0,
            scratch: vec![0u8; SCRATCH].into_boxed_slice(),
            priv_bytes: priv_raw,
            server_pub_bytes: server_pub_raw,
            psk,
        })
    }

    /// Bind a connected UDP socket for one endpoint, matching its address family
    /// (v6 endpoint → `[::]:0`, v4 endpoint → `0.0.0.0:0`).
    fn bind_for(addr: &SocketAddr) -> Result<UdpSocket, TunnelError> {
        let sock = match addr {
            SocketAddr::V6(_) => UdpSocket::bind("[::]:0")?,
            SocketAddr::V4(_) => UdpSocket::bind("0.0.0.0:0")?,
        };
        sock.connect(addr)?;
        Ok(sock)
    }

    /// The endpoint we're currently handshaking against.
    pub(crate) fn peer(&self) -> SocketAddr {
        self.candidates[self.current]
    }

    /// How many candidate endpoints we can cycle through.
    pub(crate) fn candidate_count(&self) -> usize {
        self.candidates.len()
    }

    /// Advance to the next candidate: rebuild `tunn`, re-initiate the handshake,
    /// and rebind the socket **only if the address family changed** (same-family
    /// switches re-point the existing socket, so the event loop's clone stays
    /// valid). Returns `Some(new_clone)` the loop must adopt iff a rebind
    /// happened, else `None`.
    pub(crate) fn advance_candidate(&mut self) -> Result<Option<UdpSocket>, TunnelError> {
        let prev = self.peer();
        self.current = (self.current + 1) % self.candidates.len();
        self.repoint(prev)
    }

    /// Replace the expired `Tunn` with a fresh one and restart cycling from the
    /// preferred (first) candidate, kicking off a new handshake. Same socket
    /// hand-off contract as [`advance_candidate`](Self::advance_candidate).
    pub(crate) fn rehandshake(&mut self) -> Result<Option<UdpSocket>, TunnelError> {
        let prev = self.peer();
        self.current = 0;
        self.repoint(prev)
    }

    /// Shared body for advance/rehandshake: rebuild `tunn`, point the socket at
    /// the (already-updated) current candidate, re-initiate. Rebinds only on a
    /// family change.
    fn repoint(&mut self, prev: SocketAddr) -> Result<Option<UdpSocket>, TunnelError> {
        let next = self.peer();
        self.tunn = new_tunn(self.priv_bytes, self.server_pub_bytes, self.psk);

        let rebound = if family_differs(prev, next) {
            // Different family → a brand-new socket (and a fresh fd the loop
            // must read from).
            self.udp = Self::bind_for(&next)?;
            Some(self.udp.try_clone()?)
        } else {
            // Same family → re-point the connected peer in place; the loop's
            // existing clone (same fd) follows automatically.
            self.udp.connect(next)?;
            None
        };
        self.initiate();
        Ok(rebound)
    }

    /// True once a Noise session has been established (handshake completed).
    pub(crate) fn is_established(&self) -> bool {
        // stats().0 is `time_since_last_handshake`: Some once a session exists.
        self.tunn.stats().0.is_some()
    }

    /// True once boringtun has given up retrying the handshake. The tunnel is
    /// dead and must be rebuilt to retry.
    pub(crate) fn is_expired(&self) -> bool {
        self.tunn.is_expired()
    }

    /// Clone the UDP socket so the event loop can block on reads with a timeout
    /// while this struct handles protocol state.
    pub(crate) fn udp_clone(&self) -> std::io::Result<UdpSocket> {
        self.udp.try_clone()
    }

    /// Kick off the Noise handshake. boringtun emits the handshake initiation
    /// as a `WriteToNetwork` from an empty `encapsulate`.
    pub(crate) fn initiate(&mut self) {
        if let TunnResult::WriteToNetwork(pkt) = self.tunn.encapsulate(&[], &mut self.scratch) {
            let _ = self.udp.send(pkt);
        }
    }

    /// Feed one received WG datagram. Sends any protocol replies back over UDP
    /// and pushes decapsulated **plaintext IP packets** into `out_ip` for the
    /// netstack to consume.
    pub(crate) fn process_datagram(&mut self, datagram: &[u8], out_ip: &mut Vec<Vec<u8>>) {
        // `decapsulate` may have more queued packets to flush; on a
        // `WriteToNetwork` we must re-call it with an empty datagram until it
        // returns `Done` (boringtun's documented drain protocol).
        let mut input: &[u8] = datagram;
        loop {
            match self.tunn.decapsulate(Some(self.peer().ip()), input, &mut self.scratch) {
                TunnResult::Done => break,
                TunnResult::Err(e) => {
                    tracing::debug!("wg decapsulate error: {e:?}");
                    break;
                }
                TunnResult::WriteToNetwork(pkt) => {
                    // A protocol packet (handshake/cookie) — send it, then re-call
                    // with an empty datagram to flush any further queued packets
                    // (boringtun's documented drain protocol).
                    let _ = self.udp.send(pkt);
                    input = &[];
                }
                TunnResult::WriteToTunnelV4(pkt, _) | TunnResult::WriteToTunnelV6(pkt, _) => {
                    // One WG transport datagram yields exactly one inbound IP
                    // packet — hand it up and stop (no empty re-call: that would
                    // be the *outbound* queue-flush path, not more inbound data).
                    out_ip.push(pkt.to_vec());
                    break;
                }
            }
        }
    }

    /// Encrypt one outgoing plaintext IP packet and send it to the box. Packets
    /// produced before the handshake completes are queued by boringtun and
    /// flushed automatically once a session is established.
    pub(crate) fn send_ip(&mut self, ip_pkt: &[u8]) {
        match self.tunn.encapsulate(ip_pkt, &mut self.scratch) {
            TunnResult::WriteToNetwork(pkt) => {
                let _ = self.udp.send(pkt);
            }
            TunnResult::Err(e) => tracing::debug!("wg encapsulate error: {e:?}"),
            _ => {}
        }
    }

    /// Drive the WG timers (handshake retries, keepalive). Call ~every 250ms.
    /// An `Err` here means boringtun expired the session — the caller detects
    /// that via [`is_expired`](Self::is_expired) and rebuilds.
    pub(crate) fn tick(&mut self) {
        if let TunnResult::WriteToNetwork(pkt) = self.tunn.update_timers(&mut self.scratch) {
            let _ = self.udp.send(pkt);
        }
    }
}

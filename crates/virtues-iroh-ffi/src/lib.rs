//! Swift/iOS FFI for the iroh reach layer.
//!
//! Exposes a single `IrohTransport` object that the iOS app dials by the box's
//! Ed25519 `EndpointId` + our relay URL, using this device's own iroh seed as its
//! identity (so the box's allowlist recognises it). `request()` sends a raw HTTP/1
//! request over a fresh iroh bi-stream and returns the raw response bytes — the
//! Swift side keeps its existing `URLRequest` building + response parsing and only
//! swaps the transport underneath.
//!
//! uniffi generates idiomatic Swift `async`/`throws` methods; the object is
//! `Arc`-managed, so Swift's ARC frees it (no manual `free`). Async futures are
//! driven on the tokio runtime uniffi manages (`async_runtime = "tokio"`), which
//! needs the multi-thread flavor for iroh's reactor — hence `rt-multi-thread`.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use virtues_iroh::{
    build_endpoint, EndpointAddr, EndpointId, RelayUrl, SecretKey, VirtuesIrohClient,
};

uniffi::setup_scaffolding!();

/// Wall-clock cap on binding the endpoint + first connect (foreground). Bounds a
/// cold dial so it can't hang indefinitely.
const DIAL_TIMEOUT: Duration = Duration::from_secs(20);
/// Shorter cap when dialing from an iOS background task (~30s total budget): bail
/// fast rather than get force-killed mid-dial. Data stays durable in the client's
/// queue and drains on the next wake.
const DIAL_TIMEOUT_BG: Duration = Duration::from_secs(8);
/// Wall-clock cap on a single request (connect-if-needed + write + read). Without
/// this a stuck stream would block the caller's transport forever — iOS drives
/// this from a serialized actor, so one hang would wedge all uploads. Matches the
/// old URLSession 30s.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Shorter request cap in an iOS background task (connect + first byte must fit
/// the ~30s budget alongside a cold dial). Warm-path requests finish well under.
const REQUEST_TIMEOUT_BG: Duration = Duration::from_secs(12);

/// Errors surfaced to Swift. Each variant maps to a Swift enum case with an
/// attached message, so the app can log/branch without string-matching.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum IrohError {
    /// A hex input (box EndpointId or device seed) was malformed or wrong-length.
    #[error("invalid hex input: {0}")]
    BadHex(String),
    /// The relay URL didn't parse.
    #[error("invalid relay url: {0}")]
    BadRelayUrl(String),
    /// Binding the local endpoint / dialing the box failed.
    #[error("dial failed: {0}")]
    Dial(String),
    /// Sending the request / reading the response over iroh failed.
    #[error("request failed: {0}")]
    Request(String),
}

/// Decode a 32-byte hex seed/key, tolerating surrounding whitespace.
fn decode_32(hex_str: &str, what: &str) -> Result<[u8; 32], IrohError> {
    let bytes = hex_decode(hex_str.trim())
        .ok_or_else(|| IrohError::BadHex(format!("{what}: not valid hex")))?;
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| IrohError::BadHex(format!("{what}: expected 32 bytes, got {}", bytes.len())))
}

/// Minimal hex decoder so the FFI crate doesn't pull the `hex` crate just for
/// this. Operates on bytes (never string slices) so non-ASCII input returns
/// `None` instead of panicking on a non-char-boundary slice.
fn hex_decode(s: &str) -> Option<Vec<u8>> {
    let b = s.as_bytes();
    if b.len() % 2 != 0 {
        return None;
    }
    (0..b.len())
        .step_by(2)
        .map(|i| {
            let hi = (b[i] as char).to_digit(16)?;
            let lo = (b[i + 1] as char).to_digit(16)?;
            Some((hi * 16 + lo) as u8)
        })
        .collect()
}

/// A warm iroh transport to one box. Construct with [`IrohTransport::dial`]; reuse
/// it across many `request()` calls (the underlying connection is kept warm and
/// redialed transparently on staleness). Hold a single instance for the app's
/// lifetime — a cold dial won't fit iOS's ~30s background-task budget.
#[derive(uniffi::Object)]
pub struct IrohTransport {
    client: VirtuesIrohClient,
}

#[uniffi::export(async_runtime = "tokio")]
impl IrohTransport {
    /// Dial the box. `box_id_hex` = the box's EndpointId (from the pairing
    /// ticket); `device_seed_hex` = this device's 32-byte iroh seed (generated at
    /// pairing; its EndpointId is on the box's allowlist). Reach is supplied two
    /// ways, either or both:
    /// - `relay_url` = our relay (a *claimed* box); `None`/empty for an unclaimed
    ///   box that has no relay.
    /// - `direct_addrs` = the box's direct sockets (`IP:port`), e.g. a Tailscale
    ///   `100.x:51820` or a LAN `192.168.x:51820` — dialed by NodeId with nobody
    ///   in the loop. This is what lets iOS reach an unclaimed box LAN-direct or
    ///   over Tailscale, matching the desktop helper.
    ///
    /// At least one of `relay_url` / `direct_addrs` must be usable. `background` =
    /// dialing from an iOS background task → use the shorter `DIAL_TIMEOUT_BG`
    /// budget so a cold dial bails instead of getting killed.
    #[uniffi::constructor]
    pub async fn dial(
        box_id_hex: String,
        device_seed_hex: String,
        relay_url: Option<String>,
        direct_addrs: Vec<String>,
        background: bool,
    ) -> Result<Arc<Self>, IrohError> {
        let seed = decode_32(&device_seed_hex, "device seed")?;
        let secret = SecretKey::from_bytes(&seed);
        let box_id: EndpointId = box_id_hex
            .trim()
            .parse()
            .map_err(|e| IrohError::BadHex(format!("box EndpointId: {e}")))?;

        // Relay is optional: a claimed box has one (remote reach); an unclaimed
        // box has none and is reached purely by its direct addresses.
        let relay: Option<RelayUrl> =
            match relay_url.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                Some(s) => Some(
                    s.parse()
                        .map_err(|e| IrohError::BadRelayUrl(format!("{e}")))?,
                ),
                None => None,
            };
        let direct: Vec<SocketAddr> = direct_addrs
            .iter()
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if relay.is_none() && direct.is_empty() {
            return Err(IrohError::Dial(
                "no way to reach the box — no relay and no direct addresses".into(),
            ));
        }

        let dial_timeout = if background { DIAL_TIMEOUT_BG } else { DIAL_TIMEOUT };
        // A dialing client binds an ephemeral port (only the box pins one).
        let endpoint =
            match tokio::time::timeout(dial_timeout, build_endpoint(secret, relay.clone(), None))
                .await
            {
                Ok(r) => r.map_err(|e| IrohError::Dial(format!("{e:#}")))?,
                Err(_) => return Err(IrohError::Dial("timed out binding iroh endpoint".into())),
            };
        let mut addr = EndpointAddr::new(box_id);
        for a in &direct {
            addr = addr.with_ip_addr(*a);
        }
        if let Some(r) = relay {
            addr = addr.with_relay_url(r);
        }
        let client = VirtuesIrohClient::new(endpoint, addr);
        Ok(Arc::new(Self { client }))
    }

    /// Send a raw HTTP/1 request over a fresh bi-stream; return the raw HTTP/1
    /// response bytes. Swift serializes its `URLRequest` to bytes and parses the
    /// returned bytes back into a response — the box serves each stream as a
    /// normal hyper HTTP/1 connection.
    /// `background` = called from an iOS background task → shorter `REQUEST_TIMEOUT_BG`.
    pub async fn request(&self, raw_http: Vec<u8>, background: bool) -> Result<Vec<u8>, IrohError> {
        let timeout = if background { REQUEST_TIMEOUT_BG } else { REQUEST_TIMEOUT };
        match tokio::time::timeout(timeout, self.client.request(&raw_http)).await {
            Ok(r) => r.map_err(|e| IrohError::Request(format!("{e:#}"))),
            Err(_) => Err(IrohError::Request(format!("timed out after {}s", timeout.as_secs()))),
        }
    }

    /// Graceful close — flush the QUIC close frame. Optional; dropping the last
    /// Swift reference also closes the endpoint.
    pub async fn close(&self) {
        self.client.shutdown().await;
    }
}

/// Derive a device's iroh `EndpointId` (hex) from its 32-byte seed (hex). The app
/// generates a seed at pairing, stores it, submits the derived EndpointId to the
/// box, and later dials with the same seed. Exposed so Swift doesn't need its own
/// Ed25519 implementation.
#[uniffi::export]
pub fn endpoint_id_from_seed(device_seed_hex: String) -> Result<String, IrohError> {
    let seed = decode_32(&device_seed_hex, "device seed")?;
    Ok(SecretKey::from_bytes(&seed).public().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip_and_endpoint_id() {
        let seed_hex = "00".repeat(32);
        let id = endpoint_id_from_seed(seed_hex).expect("derive id");
        // A z-base-32/hex EndpointId is non-empty and stable for a fixed seed.
        assert!(!id.is_empty());
        let id2 = endpoint_id_from_seed("00".repeat(32)).unwrap();
        assert_eq!(id, id2);
    }

    #[test]
    fn rejects_short_seed() {
        let err = endpoint_id_from_seed("00".repeat(16)).unwrap_err();
        assert!(matches!(err, IrohError::BadHex(_)));
    }

    #[test]
    fn rejects_non_hex() {
        assert!(endpoint_id_from_seed("zz".repeat(32)).is_err());
    }

    #[test]
    fn non_ascii_seed_does_not_panic() {
        // Multi-byte UTF-8 at a non-char-boundary must return an error, not panic.
        assert!(endpoint_id_from_seed("€".repeat(32)).is_err());
        assert!(hex_decode("a€").is_none());
    }
}

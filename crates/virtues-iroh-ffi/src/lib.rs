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

use std::sync::Arc;

use virtues_iroh::{build_endpoint, EndpointId, RelayUrl, SecretKey, VirtuesIrohClient};

uniffi::setup_scaffolding!();

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

/// Minimal hex decoder so the FFI crate doesn't pull the `hex` crate just for this.
fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
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
    /// Dial the box: `relay_url` = our relay, `box_id_hex` = the box's EndpointId
    /// (from the pairing ticket), `device_seed_hex` = this device's 32-byte iroh
    /// seed (generated at pairing; its EndpointId is on the box's allowlist).
    #[uniffi::constructor]
    pub async fn dial(
        relay_url: String,
        box_id_hex: String,
        device_seed_hex: String,
    ) -> Result<Arc<Self>, IrohError> {
        let seed = decode_32(&device_seed_hex, "device seed")?;
        let secret = SecretKey::from_bytes(&seed);
        let box_id: EndpointId = box_id_hex
            .trim()
            .parse()
            .map_err(|e| IrohError::BadHex(format!("box EndpointId: {e}")))?;
        let relay: RelayUrl = relay_url
            .trim()
            .parse()
            .map_err(|e| IrohError::BadRelayUrl(format!("{e}")))?;
        let endpoint = build_endpoint(secret, Some(relay.clone()))
            .await
            .map_err(|e| IrohError::Dial(format!("{e:#}")))?;
        let client = VirtuesIrohClient::from_relay(endpoint, box_id, relay);
        Ok(Arc::new(Self { client }))
    }

    /// Send a raw HTTP/1 request over a fresh bi-stream; return the raw HTTP/1
    /// response bytes. Swift serializes its `URLRequest` to bytes and parses the
    /// returned bytes back into a response — the box serves each stream as a
    /// normal hyper HTTP/1 connection.
    pub async fn request(&self, raw_http: Vec<u8>) -> Result<Vec<u8>, IrohError> {
        self.client
            .request(&raw_http)
            .await
            .map_err(|e| IrohError::Request(format!("{e:#}")))
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
}

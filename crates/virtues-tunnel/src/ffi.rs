//! uniffi FFI surface — what the iOS `VirtuesTunnel` XCFramework binds.
//!
//! Thin wrappers over the native API. Two adaptations for FFI:
//!   * objects are shared as `Arc<T>` and must be `Send + Sync`, so the stream's
//!     `&mut self` `Read`/`Write` are fronted by a `Mutex`;
//!   * the `PairingBundle` is passed as its JSON string (the exact body the box
//!     returns from `/api/pair/consume`) rather than re-modelled as a uniffi
//!     record, so the wire shape stays single-sourced in `virtues-protocol`.
//!
//! Swift usage sketch:
//! ```swift
//! let kp = generateKeypair()                       // → send kp.publicKeyB64 at pair
//! let tunnel = try VirtuesTunnel(bundleJson: json, privateKeyB64: kp.privateKeyB64)
//! let stream = try tunnel.dial(ip: bundle.internalIp, port: bundle.httpPort)
//! stream.write(data: requestBytes); let resp = stream.read(maxLen: 65536)
//! ```

use std::io::{Read, Write};
use std::net::IpAddr;
use std::sync::{Arc, Mutex};

use crate::{
    generate_keypair as native_generate_keypair, spki_fingerprint, Tunnel, TunnelError,
    TunnelStream,
};

/// FFI error — flattened to a message so Swift gets a single throwing type.
#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum TunnelFfiError {
    #[error("{0}")]
    Tunnel(String),
}

impl From<TunnelError> for TunnelFfiError {
    fn from(e: TunnelError) -> Self {
        TunnelFfiError::Tunnel(e.to_string())
    }
}

/// A freshly generated pairing keypair.
#[derive(uniffi::Record)]
pub struct PairKeypair {
    pub private_key_b64: String,
    pub public_key_b64: String,
}

/// Generate a Curve25519 keypair. Keep the private key in the keychain; send the
/// public key to the box as `wg_public_key`.
#[uniffi::export]
pub fn generate_keypair() -> PairKeypair {
    let kp = native_generate_keypair();
    PairKeypair {
        private_key_b64: kp.private_key_b64,
        public_key_b64: kp.public_key_b64,
    }
}

/// Compute the box's SPKI fingerprint (`sha256-<base64nopad>`) from its base64
/// WG public key, for out-of-band verification in Settings.
#[uniffi::export]
pub fn box_spki_fingerprint(server_public_key_b64: String) -> Result<String, TunnelFfiError> {
    let raw = crate::keys::decode_key_b64(&server_public_key_b64)?;
    Ok(spki_fingerprint(&raw).to_string())
}

/// A paired, in-app userspace WireGuard tunnel. (Named `TunnelHandle` rather
/// than `VirtuesTunnel` so the Swift type doesn't collide with the
/// `VirtuesTunnel` module name.)
#[derive(uniffi::Object)]
pub struct TunnelHandle {
    inner: Tunnel,
}

#[uniffi::export]
impl TunnelHandle {
    /// Bring up the tunnel. `bundle_json` is the raw `/api/pair/consume` body;
    /// `private_key_b64` is the device key whose public half was sent at pair.
    #[uniffi::constructor]
    pub fn new(bundle_json: String, private_key_b64: String) -> Result<Arc<Self>, TunnelFfiError> {
        let bundle: crate::PairingBundle = serde_json::from_str(&bundle_json)
            .map_err(|e| TunnelFfiError::Tunnel(format!("bundle json: {e}")))?;
        let inner = Tunnel::connect(&bundle, &private_key_b64)?;
        Ok(Arc::new(Self { inner }))
    }

    /// Open a TCP stream to `(ip, port)` inside the tunnel (the box ULA +
    /// http_port). Blocks until connected or the dial times out.
    pub fn dial(&self, ip: String, port: u16) -> Result<Arc<TunnelStreamHandle>, TunnelFfiError> {
        let addr: IpAddr = ip
            .parse()
            .map_err(|e| TunnelFfiError::Tunnel(format!("ip '{ip}': {e}")))?;
        let stream = self.inner.dial(addr, port)?;
        Ok(Arc::new(TunnelStreamHandle {
            inner: Mutex::new(stream),
        }))
    }

    /// Coarse status: "connecting" | "connected" | "failed: …" | "closed".
    pub fn status(&self) -> String {
        match self.inner.status() {
            crate::TunnelStatus::Connecting => "connecting".into(),
            crate::TunnelStatus::Connected => "connected".into(),
            crate::TunnelStatus::Failed(m) => format!("failed: {m}"),
            crate::TunnelStatus::Closed => "closed".into(),
        }
    }
}

/// One TCP byte stream inside the tunnel.
#[derive(uniffi::Object)]
pub struct TunnelStreamHandle {
    inner: Mutex<TunnelStream>,
}

#[uniffi::export]
impl TunnelStreamHandle {
    /// Read up to `max_len` bytes. Returns an empty vec on clean EOF.
    pub fn read(&self, max_len: u32) -> Result<Vec<u8>, TunnelFfiError> {
        let mut buf = vec![0u8; max_len as usize];
        let n = self
            .inner
            .lock()
            .expect("stream mutex")
            .read(&mut buf)
            .map_err(|e| TunnelFfiError::Tunnel(e.to_string()))?;
        buf.truncate(n);
        Ok(buf)
    }

    /// Write all of `data`. Returns the number of bytes accepted.
    pub fn write(&self, data: Vec<u8>) -> Result<u32, TunnelFfiError> {
        let n = self
            .inner
            .lock()
            .expect("stream mutex")
            .write(&data)
            .map_err(|e| TunnelFfiError::Tunnel(e.to_string()))?;
        Ok(n as u32)
    }
}

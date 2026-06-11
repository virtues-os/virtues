//! WireGuard remote-access support (WS-2) — server side.
//!
//! The minimal engine (kernel `wg0` via `defguard_wireguard_rs`, ULA addressing,
//! sealed box-secret storage) lives in the standalone `virtues-wg` crate so it
//! can run as a small privileged daemon while this app stays rootless — see
//! `docs/deployment.md`. The wire-protocol types (pair bundle, punch coordinator
//! shapes, constants, SPKI helpers) live in `virtues-protocol` so cross-platform
//! daemons (iOS, Mac, Android, ESP32) can decode them without dragging the box
//! binary's dep tree.
//!
//! Both are re-exported here so existing call sites under
//! `crate::wireguard::{manager, box_secrets, INTERNAL_HOST, PairingBundle, ...}`
//! keep working unchanged.
//!
//! App-side pieces that need core's deps stay here: `pairing` (assembly) and
//! `publisher` (rendezvous PUT via `BearerClient`).

pub use virtues_wg::{box_secrets, endpoint, peers, ula};

#[cfg(target_os = "linux")]
pub use virtues_wg::{manager, reconcile};

pub use virtues_protocol::{
    bundle, constants, punch, spki, INTERNAL_HOST, INTERNAL_PORT, LAN_HOST,
};

pub mod pairing;
pub mod publisher;

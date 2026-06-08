//! WireGuard remote-access support (WS-2) — server side.
//!
//! The minimal engine (kernel `wg0` via `defguard_wireguard_rs`, ULA addressing,
//! sealed box-secret storage) now lives in the standalone `virtues-wg` crate so
//! it can run as a small privileged daemon while the app stays rootless — see
//! `docs/deployment.md`. They're re-exported here so existing
//! `crate::wireguard::{manager,box_secrets,ula}` call sites are unchanged.
//!
//! App-side pieces that need core's deps stay here: `ca` (TLS cert), `bundle`
//! (the pairing-bundle model), `pairing` (assembly), `publisher` (rendezvous
//! PUT via `BearerClient`).

pub use virtues_wg::{box_secrets, endpoint, peers, ula};

#[cfg(target_os = "linux")]
pub use virtues_wg::{manager, reconcile};

pub mod bundle;
pub mod ca;
pub mod pairing;
pub mod publisher;

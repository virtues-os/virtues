//! Minimal WireGuard engine for the Virtues box.
//!
//! Built to run as a small, *privileged* standalone daemon (`virtues-wireguard`)
//! so the main app can stay rootless — see `docs/deployment.md`.
//! Deps are deliberately tiny: `defguard_wireguard_rs` + `sqlx` + crypto. No web,
//! no ML, no HTTP client, no bearer handling (the app owns rendezvous publishing).
//!
//! - `manager` — kernel `wg0` lifecycle (Linux-only; netlink).
//! - `box_secrets` — sealed singleton secrets (CA, WG keypair, rendezvous id).
//! - `ula` — the box's private `fd00:5654::/64` addressing.

pub mod box_secrets;
pub mod endpoint;
pub mod peers;
pub mod signal;
pub mod ula;

// Kernel WireGuard engine — Linux only (netlink). The macOS dev host compiles
// everything else with this cfg'd out; build + test it on Linux / OrbStack.
#[cfg(target_os = "linux")]
pub mod manager;

// Box identity + interface reconcile (the daemon core). Linux-only (uses manager).
#[cfg(target_os = "linux")]
pub mod reconcile;

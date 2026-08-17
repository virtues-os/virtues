//! Periodic in-process maintenance tasks.
//!
//! Lives inside the main `virtues` daemon — not the action runner, not a
//! separate service. Each task is a tokio interval loop that runs alongside
//! the HTTP server and shuts down with it.

pub mod ble_provision;
pub mod reset_button;
pub mod entity_resolver;
pub mod pair_rotator;
pub mod setup_ap;
pub mod sweeper;

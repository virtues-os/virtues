//! Improv Wi-Fi — the protocol both ends of Virtues onboarding speak.
//!
//! [Improv](https://improv-wifi.com) is the open BLE provisioning standard from
//! the Home Assistant / ESPHome world. The box serves it
//! (`virtues-core::maintenance::ble_provision`, BlueZ); clients drive it — the
//! desktop app through [`client`] here, iOS through its own CoreBluetooth
//! implementation, and third-party tooling through Web Bluetooth (the
//! improv-wifi.com tester can provision a box with no Virtues software at all).
//!
//! **Why this crate exists**: the framing had been written twice — once in the
//! box, once in Swift — and adding the desktop client would have made three.
//! Two of those can share, so they do: [`protocol`] is the single Rust
//! implementation, unit-tested here, used by the box and the desktop client
//! alike. Swift's copy stays duplicated because nothing can be done about that;
//! its tests live beside it.
//!
//! **The `client` feature is off by default and the box must keep it that way.**
//! The box is a GATT *server*; pulling btleplug (CoreBluetooth / WinRT / an
//! extra BlueZ path) into it would be a dependency it never uses on hardware
//! whose BLE stack it already owns.

pub mod protocol;

#[cfg(feature = "client")]
pub mod client;

pub use protocol::{
    build_result, build_rpc, chunk_for_results, parse_result, parse_rpc, service_data, Command,
    ImprovError, State, CHAR_CAPABILITIES, CHAR_CURRENT_STATE, CHAR_ERROR_STATE, CHAR_RPC_COMMAND,
    CHAR_RPC_RESULT, SERVICE_DATA_UUID_16, SERVICE_UUID,
};

#[cfg(feature = "client")]
pub use client::{FoundBox, ImprovClient, Network};

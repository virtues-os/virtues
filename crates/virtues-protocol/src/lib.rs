//! # virtues-protocol
//!
//! Wire-protocol types shared between the Virtues box (`virtues-core`), atlas
//! (`services/virtues-atlas`), and every paired-device daemon (`apps/client`,
//! `apps/ios`, `apps/mac-source`, future Android, ESP32 firmware, etc.).
//!
//! ## Why this crate exists
//!
//! The pairing-bundle types and the hole-punch coordinator types used to live
//! inside `virtues-core` (the giant box-side binary). That meant any client
//! daemon wanting to parse a pair bundle would have had to depend on
//! virtues-core's full dep tree (sqlx, axum, ML libs, etc.) — impossible to
//! build for iOS or Mac. This crate is the small, pure-data home where the
//! shared shapes live.
//!
//! Keep this crate **tiny and pure**:
//!
//! - serde + chrono + sha2 + base64 deps only
//! - no I/O, no async, no axum, no sqlx, no OS calls
//! - cross-platform-friendly (compiles on macOS, Linux, Windows, iOS, Android)
//!
//! If you find yourself wanting to add a database call or HTTP client here,
//! the right answer is "put it in the consumer crate."
//!
//! ## Module layout
//!
//! - [`bundle`] — `PairingBundle`, `WgParams`. The JSON shape the box returns
//!   from `/api/pair/consume` and the iOS/desktop daemons consume.
//! - [`spki`] — SPKI fingerprint computation (`sha256(wg_pubkey)` →
//!   `"sha256-<base64>"`) so every client can verify the box's identity in a
//!   uniform way.
//! - [`constants`] — `INTERNAL_HOST`, `LAN_HOST`, `INTERNAL_PORT`. Stable
//!   names every component references.

pub mod bundle;
pub mod constants;
pub mod spki;

// Convenience re-exports so consumers can `use virtues_protocol::{PairingBundle,
// INTERNAL_PORT, spki_fingerprint, ...}` without the inner module dance.
pub use bundle::{PairingBundle, WgParams};
pub use constants::{INTERNAL_HOST, INTERNAL_PORT, LAN_HOST};
pub use spki::{spki_fingerprint, SpkiFingerprint};

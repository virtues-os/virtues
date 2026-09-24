//! System Version Info
//!
//! Exposes the current build commit SHA for the /health endpoint
//! and settings display. There is no background auto-upgrade: a box moves
//! only when its owner runs `sudo virtues upgrade` or applies from Settings.

/// Current build commit (baked in at compile time)
pub const CURRENT_COMMIT: &str = env!("GIT_COMMIT");

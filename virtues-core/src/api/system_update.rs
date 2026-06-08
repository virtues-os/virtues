//! System Version Info
//!
//! Exposes the current build commit SHA for the /health endpoint
//! and settings display. Auto-updates are handled by Atlas via
//! scheduled maintenance windows — no polling or manual triggers needed.

/// Current build commit (baked in at compile time)
pub const CURRENT_COMMIT: &str = env!("GIT_COMMIT");

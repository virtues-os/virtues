//! Credential storage is injected, not baked in.
//!
//! The desktop sidecar stores the [`PairedBox`] in the OS keychain (+ a 0600
//! file fallback) via the `keyring` crate; the mobile plugin stores it in the
//! iOS Keychain. Rather than cfg-gate those into this crate, each host provides
//! a [`BoxStore`] and passes it into the reach functions.

use anyhow::Result;

use crate::model::PairedBox;

/// Host-provided persistence for the paired-box record.
pub trait BoxStore: Send + Sync {
    /// Load the record, or `None` if this device isn't paired.
    fn load(&self) -> Result<Option<PairedBox>>;
    /// Persist (overwrite) the record.
    fn save(&self, rec: &PairedBox) -> Result<()>;
    /// Clear all local creds — a true clean slate for revoke/reset.
    fn delete(&self) -> Result<()>;
}

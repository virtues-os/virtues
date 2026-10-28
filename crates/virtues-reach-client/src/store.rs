//! Credential storage is injected, not baked in.
//!
//! Each host provides a [`BoxStore`] and passes it into the reach functions,
//! so no platform keystore is compiled into this crate. The app's reach plugin
//! (`apps/web/plugins/reach/src/store.rs`) keeps the whole record in the
//! Keychain on iOS, the device seed in the Keychain and the rest in a 0600
//! `box.json` on macOS, and all of it in that file on every other platform. An
//! MCP agent's record is a file of its own ([`crate::mcp::AgentStore`]).

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

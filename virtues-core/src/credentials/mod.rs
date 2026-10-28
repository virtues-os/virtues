//! The credentials Vault: one table, `credentials`, for every secret on the box.
//!
//! - `kind` says what a row is: `source` (what a sync uses, keyed by source
//!   id), `box` (the box's own keys) and `env` (named keys applets declare),
//!   both through `crate::secrets`, `login` (a saved password,
//!   `api/passwords.rs`) and `connection` (`tools/connection.rs`). `key`
//!   names what the row unlocks.
//! - The secret payload is encrypted JSON whose shape the source declares.
//! - Status state machine: `pending` → `active` → (`reauth_required` |
//!   `error` | `revoked`). Only `active` credentials are usable. A revoked
//!   row keeps its id, because run history and the first-key check in
//!   `api/settings_byo.rs` point at it, but its payload is replaced with
//!   [`emptied_secrets`] in the same statement that revokes it.

use crate::error::{Error, Result};

pub mod jar;

/// The payload a revoked row keeps: `{}`, sealed under the box key.
///
/// Revoking wipes the value, so a backup taken later no longer holds a token
/// or key the owner removed. The old row version stays on disk until Postgres
/// vacuums it. The payload is sealed rather than left empty because some
/// readers decrypt by id without checking the status
/// (`vault::read_credential_secrets`), and an empty object decodes for every
/// one of them.
pub fn emptied_secrets() -> Result<String> {
    let enc = crate::crypto::TokenEncryptor::from_env()
        .map_err(|e| Error::Other(format!("vault encryptor: {e:#}")))?;
    enc.encrypt("{}").map_err(|e| Error::Other(format!("seal an emptied credential: {e:#}")))
}

/// Gives a test that seals a vault key when none is set, since CI has none:
/// thirty-two zero bytes, as the crypto crate's own tests use.
// TODO(2026-10-27): migrate the copies in api/keys.rs, api/source_auth.rs and
// secrets.rs onto this.
#[cfg(test)]
pub(crate) fn ensure_test_key() {
    if std::env::var("VIRTUES_ENCRYPTION_KEY").is_err() {
        use base64::Engine;
        std::env::set_var(
            "VIRTUES_ENCRYPTION_KEY",
            base64::engine::general_purpose::STANDARD.encode([0u8; 32]),
        );
    }
}

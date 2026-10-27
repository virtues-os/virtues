//! The credentials Vault: one table, `credentials`, for every secret on the box.
//!
//! - `kind` says what a row is: `source` (what a sync uses, keyed by source
//!   id), `box` (the box's own keys, see `crate::box_secrets`), and later
//!   `login` (agents/plan/secrets-plan.md). `key` names what the row unlocks.
//! - The secret payload is encrypted JSON whose shape the source declares.
//! - Status state machine: `pending` → `active` → (`reauth_required` |
//!   `error` | `revoked`). Only `active` credentials are usable.

pub mod jar;
pub mod types;

pub use types::{Credential, CredentialStatus, IosSecrets};

//! The credentials Vault — a single store for every secret a user holds.
//!
//! See `CREDENTIALS.md` at the repo root for the full charter. Briefly:
//!
//! - One table, `credentials`, with rows whose secret payload is encrypted
//!   JSON. The shape of the payload is declared by the connector manifest;
//!   the runtime never inspects it except by JSONPath expressions.
//! - Status state machine: `pending` → `active` → (`reauth_required` |
//!   `error` | `revoked`). Only `active` credentials are usable.
//! - O(1) bearer-token lookup via HMAC of the plaintext (column
//!   `secret_lookup_hash`), set only for `auth.kind = self_issued_bearer`
//!   connectors (iOS, Mac).
//!
//! Today, only iOS device pairing exists. The schema and Rust shape are
//! intentionally generic so future connectors (OAuth, Plaid Hosted Link,
//! API-key MCP servers) plug in without schema changes.

pub mod migrate;
pub mod types;

pub use types::{Credential, CredentialStatus, IosSecrets};

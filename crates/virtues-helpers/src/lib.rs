//! `virtues-helpers` — the single shared utility crate for Virtues.
//!
//! Used by core HTTP handlers and by every action subprocess binary.
//! Submodules organize utilities by purpose:
//!
//! ```text
//! virtues_helpers::crypto       AES-256-GCM, HMAC, OAuth state primitives
//! virtues_helpers::auth         OAuth state signing, proxy exchange, Vault writes
//! virtues_helpers::contract     ActionInput / ActionOutput wire format
//! virtues_helpers::db           DB pool from env, batch upsert helpers
//! virtues_helpers::dedup        SQL builders for batch ON CONFLICT inserts
//! virtues_helpers::ids          deterministic ID generation, prefix constants
//! virtues_helpers::ios          iOS-specific timestamp / stream constants
//! ```
//!
//! Hot-path shortcuts (used by every action binary) are re-exported at
//! the crate root: `read_input`, `output`, `connect_from_env`,
//! `ActionInput`, `ActionOutput`.
//!
//! # Anti-patterns the lints catch
//!
//! - HMAC primitives **only** in `crypto/`. CI lint: `Hmac::<Sha256>` outside
//!   `crates/virtues-helpers/src/crypto/` fails the build.
//! - Provider names (`google`, `plaid`, `notion`, ...) are banned in `auth/`.
//!   The auth helpers are catalog-driven; per-provider quirks live in proxy
//!   routes, not in helpers.

pub mod auth;
pub mod bookmarks;
pub mod contract;
pub mod crypto;
pub mod db;
pub mod dedup;
pub mod error;
pub mod handles;
pub mod ids;
pub mod input;
pub mod ios;
pub mod transport;

// Hot-path re-exports (used by every action subprocess binary).
pub use contract::{ActionInput, ActionOutput};
pub use db::connect_from_env;
pub use input::{output, output_with_records, read_input};

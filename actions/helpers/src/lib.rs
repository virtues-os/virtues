//! Virtues action helpers — shared library for action binaries.
//!
//! Every action binary links against this crate. It provides:
//!
//! - [`input`]: stdin/stdout JSON contract (`ActionInput`, `ActionOutput`, `read_input`, `output`)
//! - [`db`]: SQLite connection from `DATABASE_URL`
//! - [`ios`]: iOS-specific utilities (timestamp parsing, device validation)
//! - [`dedup`]: batch upsert patterns with `ON CONFLICT`
//! - [`oauth`]: per-provider token refresh (stub; populated as OAuth sources migrate)
//! - [`entity`]: cross-source entity resolution (stub; populated later)

pub mod db;
pub mod dedup;
pub mod entity;
pub mod ids;
pub mod input;
pub mod ios;
pub mod oauth;

pub use db::connect_from_env;
pub use input::{output, read_input, ActionInput, ActionOutput};

//! Setup config for `virtues init`.
//!
//! First boot runs [`recommended_config`], which asks no questions and reads
//! the environment, so this module is the config shape plus that loader.
//! Operators override by editing `/var/lib/virtues/virtues.env` before running
//! `virtues init`; there is no second wizard and no server-URL prompt (the
//! box's reachability is computed, not configured — see [`crate::net_check`]).

use crate::error::Result;

/// Configuration for first-boot bringup, loaded from environment defaults.
///
/// No `Debug`: in dev the URL carries the database password.
#[derive(Clone)]
pub struct SetupConfig {
    pub database_url: String,
}

/// Build a [`SetupConfig`] from environment defaults — no prompts. Used by
/// `virtues init` and `virtues doctor`.
///
/// Precedence: the process env (the installer wrote
/// `/var/lib/virtues/virtues.env`; systemd's `EnvironmentFile` + the binary's
/// env loader populate it), else the production peer-auth default.
pub fn recommended_config() -> Result<SetupConfig> {
    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres:///virtues".to_string());
    Ok(SetupConfig { database_url })
}

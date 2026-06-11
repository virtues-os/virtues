//! Setup config for `virtues init`.
//!
//! The interactive wizard was retired — first boot runs [`recommended_config`]
//! (zero prompts, env-driven), so this module is now just the config shape plus
//! the env-default loader. Operators override by editing
//! `/var/lib/virtues/virtues.env` before running `virtues init`; there is no
//! second wizard and no server-URL prompt (the box's reachability is computed,
//! not configured — see [`crate::net_check`]).

pub mod validation;

use crate::error::Result;

/// Configuration for first-boot bringup, loaded from environment defaults.
#[derive(Debug, Clone)]
pub struct SetupConfig {
    pub database_url: String,
    pub storage_path: String,
    pub encryption_key: Option<String>,
    pub run_migrations: bool,
}

/// Build a [`SetupConfig`] from environment defaults — no prompts. Used by
/// `virtues init` and any other zero-question caller.
///
/// Precedence: the process env (the installer wrote
/// `/var/lib/virtues/virtues.env`; systemd's `EnvironmentFile` + the binary's
/// env loader populate it), else the production peer-auth default.
pub fn recommended_config() -> Result<SetupConfig> {
    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres:///virtues".to_string());
    let storage_path =
        std::env::var("STORAGE_PATH").unwrap_or_else(|_| "/var/lib/virtues/lake".to_string());
    // If VIRTUES_ENCRYPTION_KEY is already in env, the binary uses it directly.
    let encryption_key = std::env::var("VIRTUES_ENCRYPTION_KEY").ok();
    Ok(SetupConfig {
        database_url,
        storage_path,
        encryption_key,
        run_migrations: true,
    })
}

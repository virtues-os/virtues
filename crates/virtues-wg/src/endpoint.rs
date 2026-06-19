//! The box's current public WG endpoint, recorded in `box_secrets`.
//!
//! This is the 1b hand-off: the (Linux, privileged) daemon detects the endpoint
//! and `write_current`s it; the (rootless) app `read_current`s it to bake into
//! the pairing bundle. The DB is the interface — the daemon never holds the
//! bearer. Cross-platform (no netlink here).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::box_secrets;

const CURRENT_ENDPOINT_KEY: &str = "wg_current_endpoint";

/// The box's current reachable WG endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    /// Current reachable global address (typically IPv6).
    pub ip: String,
    /// WG listen port.
    pub port: u16,
    /// Box WG public key (lets the phone repin if the server key rotated).
    pub wg_pub: String,
}

/// Daemon: record the box's current endpoint (on detect / change).
pub async fn write_current(db: &PgPool, ep: &Endpoint) -> Result<()> {
    let json = serde_json::to_string(ep).context("serialize endpoint")?;
    box_secrets::put(db, CURRENT_ENDPOINT_KEY, &json, &serde_json::json!({})).await
}

/// App: read the box's current endpoint (to bake into the pairing bundle).
/// `None` until the daemon has recorded one.
pub async fn read_current(db: &PgPool) -> Result<Option<Endpoint>> {
    match box_secrets::get(db, CURRENT_ENDPOINT_KEY).await? {
        Some((json, _)) => Ok(Some(
            serde_json::from_str(&json).context("parse endpoint")?,
        )),
        None => Ok(None),
    }
}

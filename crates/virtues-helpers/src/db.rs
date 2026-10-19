//! Database connection helpers for action subprocesses.
//!
//! Each action binary connects to the same Postgres instance via the
//! `DATABASE_URL` env var that the runner inherits. Applet pools are
//! intentionally small (2 connections) since most actions hit the DB
//! serially and we want to keep total backend count bounded on the
//! 8GB appliance hardware.

use anyhow::{Context, Result};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;
use std::str::FromStr;

/// Connect to Postgres using `DATABASE_URL` from the environment.
///
/// `app_name` becomes the connection's `application_name`, which is what
/// shows up in `pg_stat_activity` when you `SELECT * FROM pg_stat_activity`
/// to debug a connection-storm. Pass something specific like
/// `"virtues-action-ios-healthkit"`.
pub async fn connect_from_env(app_name: &str) -> Result<PgPool> {
    let url = std::env::var("DATABASE_URL").context("DATABASE_URL env var not set")?;

    let max_connections = std::env::var("DATABASE_MAX_CONNECTIONS")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(2);

    let opts = PgConnectOptions::from_str(&url)
        .context("invalid DATABASE_URL")?
        .application_name(app_name);

    let pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .connect_with(opts)
        .await
        .context("failed to connect to Postgres")?;
    Ok(pool)
}

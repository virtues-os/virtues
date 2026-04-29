//! Database connection helpers.
//!
//! Actions connect to SQLite via the `DATABASE_URL` env var set by the runner.

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

/// Connect to SQLite using `DATABASE_URL` from the environment.
///
/// Uses a small connection pool (5 connections max) since action subprocesses
/// are short-lived and don't need many concurrent connections.
pub async fn connect_from_env() -> Result<SqlitePool> {
    let url = std::env::var("DATABASE_URL").context("DATABASE_URL env var not set")?;
    let opts = SqliteConnectOptions::from_str(&url)
        .context("invalid DATABASE_URL")?
        .create_if_missing(false);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await
        .context("failed to connect to SQLite")?;
    Ok(pool)
}

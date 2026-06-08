//! Postgres connection + migration application for the WS-6b entitlement
//! schema.
//!
//! Optional during the transition: if `VIRTUES_API_DATABASE_URL` is unset,
//! we skip DB init and keep running with the existing RAM-budget paths.
//! Once WS-6b replaces those paths with PG-backed entitlement lookups,
//! the env var becomes required.

use anyhow::{Context, Result};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

pub async fn connect_and_migrate(database_url: &str) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(database_url)
        .await
        .context("connecting to VIRTUES_API_DATABASE_URL")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("applying virtues-api migrations")?;

    tracing::info!("virtues-api database connected; migrations applied");
    Ok(pool)
}

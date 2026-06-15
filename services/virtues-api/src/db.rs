//! Postgres connection + migration application for the entitlement schema.
//!
//! Required at boot: `VIRTUES_API_DATABASE_URL` must be set. The entitlement
//! table is the only budget store — there is no RAM-budget fallback.

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

//! Postgres connection + migration application.

use anyhow::{Context, Result};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

/// Connect to Atlas's Postgres, apply embedded migrations.
///
/// Atlas runs its OWN database, separate from virtues-api's. There is no
/// shared connection, no shared schema. The only field that bridges is
/// `activation_handle` (carried by HTTP, not by a DB join).
pub async fn connect_and_migrate(database_url: &str) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await
        .context("connecting to VIRTUES_ATLAS_DATABASE_URL")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("applying atlas migrations")?;

    tracing::info!("atlas database connected; migrations applied");
    Ok(pool)
}

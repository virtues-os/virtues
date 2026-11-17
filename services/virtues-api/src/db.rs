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

    // A rollback starts the previous image against a database that already
    // has this image's migrations. sqlx refuses to start when it finds an
    // applied migration it does not know, which would leave the service down
    // after any rollback. Migrations here are written so an older image keeps
    // working on the newer schema, so the unknown ones are safe to ignore.
    let mut migrator = sqlx::migrate!("./migrations");
    migrator.set_ignore_missing(true);
    migrator
        .run(&pool)
        .await
        .context("applying virtues-api migrations")?;

    tracing::info!("virtues-api database connected; migrations applied");
    Ok(pool)
}

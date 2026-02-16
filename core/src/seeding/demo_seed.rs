//! Demo seed - realistic sample data for development
//!
//! Embeds `seed_demo_day.sql` and executes it against the database.
//! All statements use INSERT OR IGNORE, so re-running is safe.

use crate::database::Database;
use crate::Result;
use tracing::info;

const DEMO_SQL: &str = include_str!("../../seed_demo_day.sql");

/// Seed demo data (people, places, orgs, events, messages, health, etc.)
/// Safe to call multiple times — all inserts use INSERT OR IGNORE.
pub async fn seed_demo_data(db: &Database) -> Result<()> {
    info!("🎭 Seeding demo data...");

    sqlx::raw_sql(DEMO_SQL).execute(db.pool()).await?;

    info!("✅ Demo data seeded successfully");
    Ok(())
}

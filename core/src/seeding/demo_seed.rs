//! Demo seed - realistic sample data for development
//!
//! Embeds seed SQL files and executes them against the database.
//! All statements use INSERT OR IGNORE, so re-running is safe.
//!
//! Files:
//! - seed_demo_day.sql: Feb 12-14 detailed day (primary test data)
//! - seed_baseline_w01_03.sql: Weeks 1-3 (Nov 24 - Dec 14, 2025)
//! - seed_baseline_w04_06.sql: Weeks 4-6 (Dec 15 - Jan 4, 2026)
//! - seed_baseline_w07_09.sql: Weeks 7-9 (Jan 5 - Jan 25, 2026)
//! - seed_baseline_w10_12.sql: Weeks 10-12 (Jan 26 - Feb 11, 2026)

use crate::database::Database;
use crate::Result;
use tracing::info;

const DEMO_SQL: &str = include_str!("../../seed_demo_day.sql");
const BASELINE_W01_03: &str = include_str!("../../seed_baseline_w01_03.sql");
const BASELINE_W04_06: &str = include_str!("../../seed_baseline_w04_06.sql");
const BASELINE_W07_09: &str = include_str!("../../seed_baseline_w07_09.sql");
const BASELINE_W10_12: &str = include_str!("../../seed_baseline_w10_12.sql");

/// Seed demo data (people, places, orgs, events, messages, health, etc.)
/// Safe to call multiple times — all inserts use INSERT OR IGNORE.
pub async fn seed_demo_data(db: &Database) -> Result<()> {
    info!("🎭 Seeding demo data...");

    // Primary demo day (Feb 12-14) — includes entities, places, orgs
    sqlx::raw_sql(DEMO_SQL).execute(db.pool()).await?;

    // 12-week baseline events (for novelty z-score computation)
    info!("📊 Seeding 12-week baseline (Nov 24 2025 - Feb 11 2026)...");
    sqlx::raw_sql(BASELINE_W01_03).execute(db.pool()).await?;
    sqlx::raw_sql(BASELINE_W04_06).execute(db.pool()).await?;
    sqlx::raw_sql(BASELINE_W07_09).execute(db.pool()).await?;
    sqlx::raw_sql(BASELINE_W10_12).execute(db.pool()).await?;

    info!("✅ Demo data seeded successfully");
    Ok(())
}

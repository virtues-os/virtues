//! Demo seed — realistic sample data for development.
//!
//! Embeds the two consolidated seed files and executes them against pg.
//! Every statement uses `INSERT ... ON CONFLICT DO NOTHING` so re-runs
//! are safe. See [`core/seeds/README.md`](../../seeds/README.md).

use crate::database::Database;
use crate::Result;
use tracing::info;

const DEMO_DAY_SQL: &str = include_str!("../../seeds/demo_day.sql");
const DEMO_NARRATIVE_SQL: &str = include_str!("../../seeds/demo_narrative.sql");
const DEMO_BOOKMARKS_SQL: &str = include_str!("../../seeds/demo_bookmarks.sql");

/// Seed demo data (people, places, orgs, events, messages, health, etc.).
/// Idempotent — every INSERT ends with ON CONFLICT DO NOTHING.
pub async fn seed_demo_data(db: &Database) -> Result<()> {
    info!("🎭 Seeding demo data...");

    // Primary demo day (Feb 12-14) — includes entities, places, orgs.
    sqlx::raw_sql(DEMO_DAY_SQL).execute(db.pool()).await?;

    // 12-week novelty baseline (Nov 24 2025 → Feb 11 2026).
    info!("📊 Seeding 12-week baseline...");
    sqlx::raw_sql(DEMO_NARRATIVE_SQL).execute(db.pool()).await?;

    // The designer's saves — every enrichment state the /bookmarks room has to
    // render, including the ones that are easy to forget exist (held for the
    // image pass, tombstoned-but-kept).
    info!("🔖 Seeding bookmarks...");
    sqlx::raw_sql(DEMO_BOOKMARKS_SQL).execute(db.pool()).await?;

    info!("✅ Demo data seeded successfully");
    Ok(())
}

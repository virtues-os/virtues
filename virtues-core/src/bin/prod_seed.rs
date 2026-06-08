//! Virtues Production Seed - Baseline data for new deployments
//!
//! Seeds the database with:
//! - System default models (LLM configurations)
//! - System default agents (assistant configurations)
//! - Sample axiology tags (common task categories)

use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use virtues::database::Database;
use virtues::seeding::prod_seed::seed_production_data;

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "virtues_prod_seed=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("🌱 Virtues Production Seed");
    info!("Seeding models, agents, and axiology tags...");

    // Get database URL from environment
    dotenv::dotenv().ok();
    let database_url = virtues::database::normalize_database_url()
        .expect("DATABASE_URL must be set");

    // Connect to database
    info!("Connecting to database...");
    let db = match Database::new(&database_url) {
        Ok(db) => {
            info!("✅ Database connection established");
            db
        }
        Err(e) => {
            error!("❌ Failed to connect to database: {}", e);
            std::process::exit(1);
        }
    };

    // Run production seed
    match seed_production_data(&db).await {
        Ok(_) => {
            info!("✅ Production seeding completed successfully!");
            std::process::exit(0);
        }
        Err(e) => {
            error!("❌ Production seeding failed: {}", e);
            std::process::exit(1);
        }
    }
}

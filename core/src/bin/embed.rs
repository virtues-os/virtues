//! Embedding job CLI
//!
//! Run embeddings manually: cargo run --bin embed

use sqlx::sqlite::SqlitePoolOptions;
use virtues::embeddings::EmbeddingJob;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt().with_env_filter("info").init();

    // Connect to database
    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:./data/virtues.db".to_string());

    tracing::info!("Connecting to database...");
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    tracing::info!("Creating embedding job...");

    // Create embedding job from environment
    let job = match EmbeddingJob::from_env(pool) {
        Ok(job) => job,
        Err(e) => {
            tracing::error!("Failed to create embedding job: {}", e);
            return Err(e.into());
        }
    };

    tracing::info!("Processing embeddings...");

    // Process all tables
    match job.process_all().await {
        Ok(results) => {
            tracing::info!("Embedding job completed successfully!");
            for r in &results {
                tracing::info!(
                    "  {}: {} processed, {} failed ({}ms)",
                    r.table,
                    r.records_processed,
                    r.records_failed,
                    r.duration_ms
                );
            }
        }
        Err(e) => {
            tracing::error!("Embedding job failed: {}", e);
            return Err(e.into());
        }
    }

    Ok(())
}

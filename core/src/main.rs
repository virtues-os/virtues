//! Virtues CLI - Command-line interface for the Virtues personal data platform

use virtues::cli::types::Cli;
use virtues::VirtuesBuilder;
use clap::Parser;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load environment variables from .env file
    // Try current directory first, then parent directory (for running from core/)
    if dotenv::dotenv().is_err() {
        let _ = dotenv::from_path("../.env");
    }

    // Initialize tracing
    // Use RUST_LOG env var, falling back to INFO if not set
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .init();

    // Initialize observability (metrics)
    // If OTEL_EXPORTER_OTLP_ENDPOINT is set, metrics will be exported
    if let Err(e) = virtues::observability::init(virtues::observability::ObservabilityConfig::default()) {
        tracing::warn!(error = %e, "Failed to initialize observability, continuing without metrics");
    }

    let cli = Cli::parse();

    // Handle Init command early (doesn't need Virtues client)
    if matches!(cli.command, virtues::cli::types::Commands::Init) {
        let config = virtues::setup::run_init().await?;

        // Save configuration
        virtues::setup::save_config(&config)?;

        // Run migrations if requested
        if config.run_migrations {
            println!();
            println!("📊 Running migrations...");
            let db = virtues::database::Database::new(&config.database_url)?;
            db.initialize().await?;
            println!("✅ Migrations complete");
        }

        virtues::setup::display_completion();
        return Ok(());
    }

    // Get database URL from environment
    let database_url =
        env::var("DATABASE_URL").unwrap_or_else(|_| "postgresql://localhost/virtues".to_string());

    // Initialize Virtues client with optional S3/MinIO configuration
    let mut builder = VirtuesBuilder::new().postgres(&database_url);

    // Configure S3/MinIO storage if environment variables are present
    if let Ok(bucket) = env::var("S3_BUCKET") {
        builder = builder.s3_bucket(&bucket);

        if let Ok(endpoint) = env::var("S3_ENDPOINT") {
            builder = builder.s3_endpoint(&endpoint);
        }
        if let Ok(access_key) = env::var("S3_ACCESS_KEY") {
            if let Ok(secret_key) = env::var("S3_SECRET_KEY") {
                builder = builder.s3_credentials(&access_key, &secret_key);
            }
        }
    }

    let virtues = builder.build().await?;

    // Run CLI commands
    virtues::cli::run(cli, virtues).await?;

    Ok(())
}

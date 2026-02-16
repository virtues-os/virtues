//! Virtues CLI - Command-line interface for the Virtues personal data platform

use clap::Parser;
use std::env;
use std::path::Path;
use virtues::cli::types::{Cli, Commands};
use virtues::search::Embedder;
use virtues::VirtuesBuilder;

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

    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    // Initialize observability (metrics)
    // If OTEL_EXPORTER_OTLP_ENDPOINT is set, metrics will be exported
    if let Err(e) =
        virtues::observability::init(virtues::observability::ObservabilityConfig::default())
    {
        tracing::warn!(error = %e, "Failed to initialize observability, continuing without metrics");
    }

    let cli = Cli::parse();

    // Handle WarmModels early (no database needed — just downloads ML models)
    if matches!(cli.command, Some(Commands::WarmModels)) {
        println!("Downloading embedding model (nomic-embed-text-v1.5)...");
        let embedder = virtues::search::get_embedder().await?;
        println!("Embedding model ready (dim={})", embedder.dimension());

        println!("Downloading reranker model (bge-reranker-v2-m3)...");
        let _reranker = virtues::search::get_reranker().await?;
        println!("Reranker model ready");

        return Ok(());
    }

    // Handle Init command early (doesn't need Virtues client)
    if matches!(cli.command, Some(Commands::Init)) {
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
    let mut database_url =
        env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:/data/virtues.db".to_string());

    // Auto-setup: Create data directory if it doesn't exist (for SQLite)
    if database_url.starts_with("sqlite:") {
        let db_path = database_url.trim_start_matches("sqlite:");
        // Strip query parameters to get the file path
        let file_path = db_path.split('?').next().unwrap_or(db_path);

        if let Some(parent) = Path::new(file_path).parent() {
            if !parent.exists() {
                println!("📁 Creating data directory: {}", parent.display());
                std::fs::create_dir_all(parent)?;
            }
        }

        // Ensure SQLite creates the file if it doesn't exist (mode=rwc)
        if !database_url.contains("mode=") {
            if database_url.contains('?') {
                database_url.push_str("&mode=rwc");
            } else {
                database_url.push_str("?mode=rwc");
            }
        }
    }

    // Initialize Virtues client
    // Storage path: STORAGE_PATH env var or ./data/lake default
    let mut builder = VirtuesBuilder::new().database(&database_url);

    // Configure storage path if specified
    if let Ok(storage_path) = env::var("STORAGE_PATH") {
        builder = builder.storage_path(&storage_path);
    }

    let virtues = builder.build().await?;

    // Default to server with auto-migrate if no command specified
    let cli = if cli.command.is_none() {
        println!("🚀 Starting Virtues (auto-setup mode)...");
        println!();

        // Run migrations first
        println!("📊 Running migrations...");
        virtues.database.initialize().await?;
        println!("✅ Migrations complete");
        println!();

        // Seed production defaults (models, agents, etc.)
        println!("🌱 Seeding defaults...");
        virtues::seeding::prod_seed::seed_production_data(&virtues.database).await?;
        println!("✅ Seeding complete");
        println!();

        // In production (Nomad), NOMAD_PORT_http is set to the dynamically allocated port.
        // Fall back to 8000 for local development.
        let port = env::var("NOMAD_PORT_http")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(8000);

        Cli {
            command: Some(Commands::Server {
                host: "0.0.0.0".to_string(),
                port,
            }),
        }
    } else {
        cli
    };

    // Run CLI commands
    virtues::cli::run(cli, virtues).await?;

    Ok(())
}

//! Virtues Seed - Database seeding utility
//!
//! Seeds the database with Monday in Rome reference dataset

use virtues::database::Database;
use virtues::seeding::seed_monday_in_rome_dataset;
use virtues::storage::{stream_writer::StreamWriter, Storage};
use clap::Parser;
use std::env;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(name = "virtues-seed")]
#[command(about = "Virtues Seed - Monday in Rome Reference Dataset")]
struct Args {
    /// Build timeline chunks after seeding (recommended)
    #[arg(long, default_value_t = true)]
    build_chunks: bool,

    /// Generate day view after seeding (uses stored chunks if available)
    #[arg(long, default_value_t = false)]
    day_view: bool,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "virtues_seed=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Virtues Seed - Monday in Rome Reference Dataset");

    // Get database URL from environment
    dotenv::dotenv().ok();
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    // Connect to database
    info!("Connecting to database...");
    let db = match Database::new(&database_url) {
        Ok(db) => {
            info!("Database connection established");
            db
        }
        Err(e) => {
            error!("Failed to connect to database: {}", e);
            std::process::exit(1);
        }
    };

    // Initialize storage and stream writer for pipeline seeding
    let storage = if let Ok(bucket) = env::var("S3_BUCKET") {
        let endpoint = env::var("S3_ENDPOINT").ok();
        let access_key = env::var("S3_ACCESS_KEY").ok();
        let secret_key = env::var("S3_SECRET_KEY").ok();

        match Storage::s3(bucket, endpoint, access_key, secret_key).await {
            Ok(storage) => {
                info!("S3/MinIO storage initialized");
                storage
            }
            Err(e) => {
                error!("Failed to initialize S3 storage: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        // Fall back to local storage
        let path = env::var("STORAGE_PATH").unwrap_or_else(|_| "./data".to_string());
        match Storage::local(path) {
            Ok(storage) => {
                info!("Local storage initialized");
                storage
            }
            Err(e) => {
                error!("Failed to initialize local storage: {}", e);
                std::process::exit(1);
            }
        }
    };

    let stream_writer = Arc::new(Mutex::new(StreamWriter::new()));

    info!("Seeding Monday in Rome dataset...");
    info!("This seeds: CSV -> Archive -> Transform -> Ontology tables -> Entity Resolution");

    match seed_monday_in_rome_dataset(&db, &storage, stream_writer).await {
        Ok(count) => {
            info!(
                "Monday in Rome ontology seeding completed: {} records!",
                count
            );
        }
        Err(e) => {
            error!("Monday in Rome seeding failed: {}", e);
            std::process::exit(1);
        }
    }

    // Build timeline chunks (enabled by default)
    if args.build_chunks {
        use chrono::{Duration, Utc};

        info!("Building timeline chunks...");

        // Process chunks for the seed data time range (Nov 10, 2025 Rome time)
        // Use a wide window to capture all seed data
        let end = Utc::now();
        let start = end - Duration::days(30); // Look back 30 days to capture seed data

        match virtues::timeline::chunks::process_time_window(db.pool(), start, end).await {
            Ok(count) => {
                info!("Timeline chunks built: {} chunks created", count);
            }
            Err(e) => {
                error!("Timeline chunk building failed: {}", e);
                // Don't exit - this is non-fatal
            }
        }
    }

    // Optional: Generate day view
    if args.day_view {
        use chrono::NaiveDate;

        info!("Generating day view for 2025-11-10...");

        let date = NaiveDate::from_ymd_opt(2025, 11, 10).unwrap();
        // Use stored chunks (Rome is UTC+1)
        match virtues::timeline::chunks::get_day_view(db.pool(), date, 1).await {
            Ok(day_view) => {
                info!(
                    "Day view generated: {} chunks ({} location, {} transit, {} missing)",
                    day_view.chunks.len(),
                    day_view.total_location_minutes,
                    day_view.total_transit_minutes,
                    day_view.total_missing_minutes
                );

                // Print summary of chunks
                for (i, chunk) in day_view.chunks.iter().enumerate() {
                    match chunk {
                        virtues::timeline::chunks::Chunk::Location(loc) => {
                            info!(
                                "  [{}] Location: {} ({} min) - {} messages, {} transcripts",
                                i,
                                loc.place_name.as_deref().unwrap_or("Unknown"),
                                loc.duration_minutes,
                                loc.messages.len(),
                                loc.transcripts.len()
                            );
                        }
                        virtues::timeline::chunks::Chunk::Transit(t) => {
                            info!(
                                "  [{}] Transit: {:.2} km, {:.1} km/h ({} min)",
                                i,
                                t.distance_km,
                                t.avg_speed_kmh,
                                t.duration_minutes
                            );
                        }
                        virtues::timeline::chunks::Chunk::MissingData(m) => {
                            info!(
                                "  [{}] Missing: {:?} ({} min)",
                                i,
                                m.likely_reason,
                                m.duration_minutes
                            );
                        }
                    }
                }
            }
            Err(e) => {
                error!("Day view generation failed: {}", e);
            }
        }
    }

    std::process::exit(0);
}

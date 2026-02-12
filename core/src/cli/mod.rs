//! CLI module - command-line interface for Virtues

pub mod commands;
pub mod display;
pub mod types;

use crate::storage::stream_writer::StreamWriter;
use crate::Virtues;
use std::sync::Arc;
use tokio::sync::Mutex;
use types::{Cli, Commands};

/// Run the CLI application
pub async fn run(cli: Cli, virtues: Virtues) -> Result<(), Box<dyn std::error::Error>> {
    // Initialize StreamWriter (simple in-memory buffer)
    let stream_writer = StreamWriter::new();
    let stream_writer_arc = Arc::new(Mutex::new(stream_writer));

    // Command should always be Some at this point (main.rs handles None case)
    let command = cli.command.expect("Command should be set by main.rs");

    match command {
        Commands::Init => {
            // This command is handled in main.rs before the Virtues client is created
            unreachable!("Init command should be handled in main.rs");
        }

        Commands::Migrate => {
            println!("Running database migrations...");
            virtues.database.initialize().await?;
            println!("✅ Migrations completed successfully");
        }

        Commands::Catalog { action } => {
            commands::handle_catalog_command(action)?;
        }

        Commands::Add {
            source_type,
            device_id,
            name,
        } => {
            commands::handle_add_source(virtues, &source_type, device_id, name).await?;
        }

        Commands::Source { action } => {
            commands::handle_source_command(virtues, action).await?;
        }

        Commands::Stream { action } => {
            commands::handle_stream_command(virtues, stream_writer_arc.clone(), action).await?;
        }

        Commands::Sync { source_id } => {
            println!("Syncing source: {}...", source_id);

            // Get all enabled streams for this source
            let streams = crate::list_source_streams(virtues.database.pool(), source_id.clone()).await?;
            let enabled_streams: Vec<_> = streams.iter().filter(|s| s.is_enabled).collect();

            if enabled_streams.is_empty() {
                println!("⚠️  No enabled streams for this source");
                println!(
                    "Enable streams with: virtues stream enable {} <stream_name>",
                    source_id
                );
                return Ok(());
            }

            println!("Syncing {} enabled stream(s)...\n", enabled_streams.len());

            let sync_mode = crate::SyncMode::full_refresh();
            let mut jobs_created = 0;
            let mut failed_count = 0;

            for stream in enabled_streams {
                println!("  Creating sync job for {}...", stream.stream_name);

                match crate::api::jobs::trigger_stream_sync(
                    virtues.database.pool(),
                    &*virtues.storage,
                    stream_writer_arc.clone(),
                    source_id.clone(),
                    &stream.stream_name,
                    Some(sync_mode.clone()),
                )
                .await
                {
                    Ok(response) => {
                        jobs_created += 1;
                        println!(
                            "    ✅ Job created: {} (status: {})",
                            response.job_id, response.status
                        );
                    }
                    Err(e) => {
                        failed_count += 1;
                        println!("    ❌ Error: {}", e);
                    }
                }
            }

            println!("\n📊 Sync Summary:");
            println!("  Jobs created: {}", jobs_created);
            if failed_count > 0 {
                println!("  Failed to create jobs: {}", failed_count);
            }
            println!("\nNote: Jobs are running in the background. Use 'virtues jobs list' to monitor progress.");
        }

        Commands::Server { host, port } => {
            // Run migrations and seed data
            println!("📊 Running migrations...");
            virtues.database.initialize().await?;
            println!("✅ Migrations complete");

            println!("🌱 Seeding defaults...");
            crate::seeding::prod_seed::seed_production_data(&virtues.database).await?;
            println!("✅ Seeding complete");
            println!();

            println!("Starting Virtues server on {}:{}", host, port);
            println!("API available at http://{}:{}/api", host, port);
            println!("Health check: http://{}:{}/health", host, port);
            println!();
            println!("Press Ctrl+C to stop");

            crate::server::run(virtues, &host, port).await?;
        }

        Commands::Tunnel => {
            commands::handle_tunnel_command(virtues).await?;
        }

        Commands::WarmModels => {
            unreachable!("WarmModels command should be handled in main.rs");
        }
    }

    Ok(())
}

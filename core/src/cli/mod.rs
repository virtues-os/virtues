//! CLI module - command-line interface for Virtues

pub mod commands;
pub mod types;

use crate::Virtues;
use types::{Cli, Commands};

/// Run the CLI application
pub async fn run(cli: Cli, virtues: Virtues) -> Result<(), Box<dyn std::error::Error>> {
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
            println!("Migrations completed successfully");
        }

        Commands::Server { host, port } => {
            // Run migrations and seed data
            println!("Running migrations...");
            virtues.database.initialize().await?;
            println!("Migrations complete");

            println!("Seeding defaults...");
            crate::seeding::prod_seed::seed_production_data(&virtues.database).await?;
            println!("Seeding complete");
            println!();

            println!("Starting Virtues server on {}:{}", host, port);
            println!("API available at http://{}:{}/api", host, port);
            println!("Health check: http://{}:{}/health", host, port);
            println!();
            println!("Press Ctrl+C to stop");

            crate::server::run(virtues, &host, port).await?;
        }

        Commands::Seed => {
            println!("Running migrations...");
            virtues.database.initialize().await?;
            println!("Migrations complete");
            println!();

            println!("Seeding demo data...");
            crate::seeding::seed_demo_data(&virtues.database).await?;
            println!("Demo data seeded");
        }

        Commands::Tunnel => {
            commands::handle_tunnel_command(virtues).await?;
        }

        Commands::WarmModels => {
            unreachable!("WarmModels command should be handled in main.rs");
        }

        Commands::ComputeNovelty => {
            println!("Running migrations...");
            virtues.database.initialize().await?;
            println!("Computing novelty scores for all days...");

            let pool = virtues.database.pool();

            // Get all dates that have events with summaries
            let dates: Vec<String> = sqlx::query_scalar(
                "SELECT DISTINCT d.date FROM wiki_days d \
                 JOIN wiki_events e ON e.day_id = d.id \
                 WHERE e.event_summary IS NOT NULL AND e.event_summary != '' \
                 ORDER BY d.date"
            )
            .fetch_all(pool)
            .await?;

            println!("Found {} days with events to score", dates.len());

            let mut total_scored = 0u32;
            for (i, date_str) in dates.iter().enumerate() {
                let date = date_str.parse::<chrono::NaiveDate>()
                    .map_err(|e| format!("Bad date {}: {}", date_str, e))?;

                match crate::dayline::novelty::compute_novelty_for_day(pool, date).await {
                    Ok(scored) => {
                        total_scored += scored;
                        if scored > 0 || (i + 1) % 10 == 0 {
                            println!("  {} — {} events scored ({}/{})", date_str, scored, i + 1, dates.len());
                        }
                    }
                    Err(e) => {
                        eprintln!("  {} — error: {}", date_str, e);
                    }
                }
            }

            println!("Event novelty: {} events scored across {} days.", total_scored, dates.len());

            // Topic/entity novelty
            println!("Computing topic/entity novelty...");
            let mut total_te = 0u32;
            for (i, date_str) in dates.iter().enumerate() {
                let date = date_str.parse::<chrono::NaiveDate>()
                    .map_err(|e| format!("Bad date {}: {}", date_str, e))?;

                match crate::dayline::topic_entity_novelty::compute_topic_entity_novelty(pool, date).await {
                    Ok(updated) => {
                        total_te += updated;
                        if (i + 1) % 20 == 0 {
                            println!("  topic/entity: {}/{} days", i + 1, dates.len());
                        }
                    }
                    Err(e) => {
                        eprintln!("  {} — topic/entity error: {}", date_str, e);
                    }
                }
            }

            println!("Topic/entity novelty: {} events updated.", total_te);
        }

        Commands::ComputeAutonomic => {
            println!("Running migrations...");
            virtues.database.initialize().await?;
            println!("Computing autonomic scores for all days...");

            let pool = virtues.database.pool();

            let dates: Vec<String> = sqlx::query_scalar(
                "SELECT DISTINCT d.date FROM wiki_days d \
                 JOIN wiki_events e ON e.day_id = d.id \
                 WHERE e.avg_hr IS NOT NULL \
                 ORDER BY d.date"
            )
            .fetch_all(pool)
            .await?;

            println!("Found {} days with HR data to score", dates.len());

            let mut total_scored = 0u32;
            for (i, date_str) in dates.iter().enumerate() {
                let date = date_str.parse::<chrono::NaiveDate>()
                    .map_err(|e| format!("Bad date {}: {}", date_str, e))?;

                match crate::dayline::autonomic_scoring::compute_autonomic_for_day(pool, date).await {
                    Ok(scored) => {
                        total_scored += scored;
                        if scored > 0 || (i + 1) % 10 == 0 {
                            println!("  {} — {} events scored ({}/{})", date_str, scored, i + 1, dates.len());
                        }
                    }
                    Err(e) => {
                        eprintln!("  {} — error: {}", date_str, e);
                    }
                }
            }

            println!("Autonomic scoring: {} events scored across {} days.", total_scored, dates.len());
        }
    }

    Ok(())
}

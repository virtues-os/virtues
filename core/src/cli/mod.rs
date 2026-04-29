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

        Commands::ResolveEntities { hours } => {
            use crate::entity_resolution::{self, TimeWindow};
            println!("Running entity resolution for last {hours} hours...");
            let window = TimeWindow::from_lookback_hours(hours);
            println!("  window: {} → {}", window.start, window.end);
            let stats = entity_resolution::resolve_entities(&virtues.database, window).await?;
            println!();
            println!("✅ Resolution complete");
            println!("   places_resolved: {}", stats.places_resolved);
            println!("   people_resolved: {}", stats.people_resolved);
            println!("   duration:        {}ms", stats.duration_ms);
        }

        Commands::VerifyTokens { bearer } => {
            use crate::crypto::TokenEncryptor;
            println!("Loading encryptor from VIRTUES_ENCRYPTION_KEY...");
            let encryptor = match TokenEncryptor::from_env() {
                Ok(e) => {
                    println!("  ✓ encryptor loaded");
                    e
                }
                Err(e) => {
                    println!("  ✗ FAILED: {e}");
                    return Ok(());
                }
            };
            println!();

            let rows: Vec<(String, String, Option<String>, String)> = sqlx::query_as(
                r#"SELECT id, status, secret_lookup_hash, secrets_ciphertext
                     FROM credentials WHERE source_id = 'ios'"#,
            )
            .fetch_all(virtues.database.pool())
            .await?;

            let bearer_hash = bearer
                .as_deref()
                .map(|b| encryptor.lookup_hash(b))
                .transpose()?;

            println!("Found {} iOS row(s) in credentials:", rows.len());
            for (id, status, lookup_hash, secrets_ciphertext) in &rows {
                println!();
                println!("  id={id}");
                println!("  status={status}");
                match lookup_hash {
                    Some(h) => {
                        let prefix = &h[..h.len().min(16)];
                        println!("  secret_lookup_hash: {prefix}…");
                        if let Some(ref bh) = bearer_hash {
                            if h == bh {
                                println!("  ✓ MATCHES bearer hash");
                            } else {
                                println!("  ✗ does NOT match bearer hash");
                            }
                        }
                    }
                    None => {
                        println!("  secret_lookup_hash: NULL (pending or revoked)");
                    }
                }
                match encryptor.decrypt(secrets_ciphertext) {
                    Ok(plaintext) => {
                        let preview = if plaintext.len() > 60 {
                            format!("{}…", &plaintext[..60])
                        } else {
                            plaintext
                        };
                        println!("  ✓ DECRYPT OK → {preview}");
                    }
                    Err(e) => println!("  ✗ DECRYPT FAILED: {e}"),
                }
            }
        }

        Commands::PairIos { device_id, name } => {
            // Make sure the schema is in place before we touch it
            println!("Running migrations...");
            virtues.database.initialize().await?;

            println!("Pairing iOS device {device_id} as '{name}'...");
            let pool = virtues.database.pool();
            let credential_id =
                virtues_helpers::auth::mint_pending_credential(pool, "ios", &name).await?;
            virtues_helpers::auth::finalize_self_issued_bearer(
                pool,
                &credential_id,
                &device_id,
                &serde_json::json!({}),
            )
            .await?;
            crate::action_templates::reconcile_templates(pool).await?;

            println!();
            println!("✅ Paired");
            println!("   credential_id:  {credential_id}");
            println!("   device_token:   {device_id}");
            println!();
            println!("Verify:");
            println!(
                "  sqlite3 ./data/virtues.db \"SELECT id, source_id, status FROM credentials WHERE id='{credential_id}';\""
            );
            println!(
                "  sqlite3 ./data/virtues.db \"SELECT function_name, credential_id FROM app_actions WHERE credential_id='{credential_id}';\""
            );
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

        Commands::DaySummary { date } => {
            println!("Running migrations...");
            virtues.database.initialize().await?;

            let pool = virtues.database.pool();

            // Resolve target date: explicit --date, or "today" in the user's profile tz.
            let target_date: chrono::NaiveDate = match date {
                Some(s) => s
                    .parse::<chrono::NaiveDate>()
                    .map_err(|e| format!("Invalid --date '{s}': {e}"))?,
                None => {
                    let tz_str = crate::api::profile::get_timezone(pool).await.ok().flatten();
                    if let Some(tz_name) = tz_str.as_deref() {
                        match tz_name.parse::<chrono_tz::Tz>() {
                            Ok(tz) => chrono::Utc::now().with_timezone(&tz).date_naive(),
                            Err(_) => chrono::Local::now().date_naive(),
                        }
                    } else {
                        chrono::Local::now().date_naive()
                    }
                }
            };

            // Resolve timezone for display formatting (events stored in UTC).
            let tz_for_display: Option<chrono_tz::Tz> = crate::api::profile::get_timezone(pool)
                .await
                .ok()
                .flatten()
                .and_then(|s| s.parse().ok());

            println!("Generating day summary for {target_date}...");
            let day = crate::api::day_summary::generate_day_summary(pool, target_date).await?;

            println!();
            println!("✅ Day summary written to wiki_days id={}", day.id);
            if let Some(epigraph) = &day.epigraph {
                println!();
                println!("Epigraph:");
                println!("  {epigraph}");
            }
            if let Some(autobio) = &day.autobiography {
                println!();
                println!("Autobiography:");
                for line in autobio.lines() {
                    println!("  {line}");
                }
            }
            if let Some(dq) = &day.data_quality {
                println!();
                println!("Data quality: {dq}");
            }

            // Show events that were created
            let events = crate::api::wiki::get_day_events(pool, day.id.clone()).await?;
            println!();
            println!("Events ({}):", events.len());
            for ev in &events {
                let label = ev
                    .auto_label
                    .as_deref()
                    .or(ev.user_label.as_deref())
                    .unwrap_or("(no label)");
                let loc = ev
                    .auto_location
                    .as_deref()
                    .or(ev.user_location.as_deref())
                    .map(|l| format!(" @ {l}"))
                    .unwrap_or_default();
                let (start_fmt, end_fmt) = if let Some(tz) = tz_for_display {
                    (
                        ev.start_time.with_timezone(&tz).format("%H:%M").to_string(),
                        ev.end_time.with_timezone(&tz).format("%H:%M").to_string(),
                    )
                } else {
                    (
                        ev.start_time.format("%H:%M").to_string(),
                        ev.end_time.format("%H:%M").to_string(),
                    )
                };
                println!("  {start_fmt} → {end_fmt}  {label}{loc}");
                if let Some(summary) = ev.event_summary.as_ref().filter(|s| !s.trim().is_empty()) {
                    println!("                  {summary}");
                }
            }
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

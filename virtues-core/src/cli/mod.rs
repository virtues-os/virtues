//! CLI module - command-line interface for Virtues

pub mod backup;
pub mod commands;
pub mod configure_inference;
pub mod diag;
pub mod doctor;
pub mod lake_adopt;
pub mod link;
pub mod reindex;
pub mod report_crash;
pub mod reset;
pub mod restore;
pub mod types;
pub mod ui;
pub mod uninstall;
pub mod upgrade;

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

        Commands::Pair { .. } => {
            // Handled in main.rs before the Virtues client is created — it
            // only needs the DB pool, not the full app stack.
            unreachable!("Pair command should be handled in main.rs");
        }

        Commands::Sudo { .. } => {
            // Same — handled in main.rs against a bare DB pool.
            unreachable!("Sudo command should be handled in main.rs");
        }

        Commands::Device { .. } => {
            // Same — handled in main.rs against a bare DB pool.
            unreachable!("Device command should be handled in main.rs");
        }

        Commands::Backup { .. } => {
            // Handled in main.rs — runs against a bare DB pool, doesn't
            // need the full Virtues client.
            unreachable!("Backup command should be handled in main.rs");
        }

        Commands::Restore { .. } => {
            // Same.
            unreachable!("Restore command should be handled in main.rs");
        }

        Commands::Upgrade { .. } => {
            // Same — and intentionally does NOT touch the DB; the new
            // binary's `migrate` does that after the binary swap.
            unreachable!("Upgrade command should be handled in main.rs");
        }

        Commands::Uninstall { .. } => {
            // Same — must work even when the DB pool can't be built;
            // a broken install is uninstall's primary use case.
            unreachable!("Uninstall command should be handled in main.rs");
        }

        Commands::Reset { .. } => {
            // Same — destructive schema management against a bare pool,
            // like Restore/Uninstall.
            unreachable!("Reset command should be handled in main.rs");
        }

        Commands::ConfigureInference { .. } => {
            // Same — must run before the app builds the guarded embedder, which
            // would fail on the fingerprint mismatch this command fixes.
            unreachable!("ConfigureInference command should be handled in main.rs");
        }

        Commands::Reindex { .. } => {
            // Same — runs before the normal init path, whose ensure_embedding_dims
            // would refuse the width change on the still-populated index.
            unreachable!("Reindex command should be handled in main.rs");
        }
        Commands::LakeAdopt { .. } => {
            // Needs only a pool + STORAGE_PATH, so main.rs runs it before the client
            // stack is built.
            unreachable!("LakeAdopt command should be handled in main.rs");
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

        Commands::Status { json } => {
            if json {
                commands::status_json::print(&virtues)
                    .await
                    .map_err(|e| e.to_string())?;
            } else {
                commands::deploy::handle_status(&virtues)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }

        Commands::ReportCrash => {
            // Handled in main.rs before the Virtues client is created — the
            // crash beacon only needs the env vars systemd passes; it
            // explicitly does NOT want to depend on a healthy DB pool (the
            // crash may BE the DB pool going down).
            unreachable!("ReportCrash command should be handled in main.rs");
        }

        Commands::Bringup => {
            commands::deploy::handle_bringup(&virtues)
                .await
                .map_err(|e| e.to_string())?;
        }


        Commands::Subscribe => {
            // Idempotent — sqlx::migrate skips applied steps. Without this,
            // a fresh box hits "relation \"box_secrets\" does not exist"
            // mid-subscribe because no other startup path has been run yet.
            virtues.database.initialize().await?;
            commands::deploy::handle_subscribe(&virtues)
                .await
                .map_err(|e| e.to_string())?;
        }

        Commands::AccountLogin => {
            // Pairs with `virtues init`'s [1] Log in branch; same idempotent
            // migration guard as Subscribe so a standalone retry on a fresh
            // box doesn't hit a missing-table error mid-login.
            virtues.database.initialize().await?;
            commands::deploy::handle_login(&virtues)
                .await
                .map_err(|e| e.to_string())?;
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

        Commands::WarmModels => {
            unreachable!("WarmModels command should be handled in main.rs");
        }
        Commands::Doctor => {
            unreachable!("Doctor command should be handled in main.rs");
        }

        Commands::Magnet => {
            virtues.database.initialize().await?;
            let pool = virtues.database.pool();

            use crate::magnet::{self, NOTEBOOK, STORY};
            let notebooks = magnet::run_all(pool, NOTEBOOK).await?;
            let stories = magnet::run_all(pool, STORY).await?;

            println!("magnet · notebooks attached {notebooks} · stories attached {stories}");
            return Ok(());
        }

        Commands::ComputeNovelty => {
            println!("Running migrations...");
            virtues.database.initialize().await?;
            println!("Computing novelty scores for all days...");

            let pool = virtues.database.pool();

            // `wiki_days.date` is DATE, not TEXT. Decoding it as String made
            // this backfill fail on its very first row with a ColumnDecode
            // error — so the one manual escape hatch from the cron's
            // delete-then-score bug had itself never run.
            let dates: Vec<chrono::NaiveDate> = sqlx::query_scalar(
                "SELECT DISTINCT d.date FROM wiki_days d \
                 JOIN wiki_events e ON e.day_id = d.id \
                 WHERE e.event_summary IS NOT NULL AND e.event_summary != '' \
                 ORDER BY d.date"
            )
            .fetch_all(pool)
            .await?;

            println!("Found {} days with events to score", dates.len());

            let mut total_scored = 0u32;
            for (i, date) in dates.iter().enumerate() {
                let date = *date;

                match crate::dayline::novelty::compute_novelty_for_day(pool, date).await {
                    Ok(scored) => {
                        total_scored += scored;
                        if scored > 0 || (i + 1) % 10 == 0 {
                            println!("  {} — {} events scored ({}/{})", date, scored, i + 1, dates.len());
                        }
                    }
                    Err(e) => {
                        eprintln!("  {} — error: {}", date, e);
                    }
                }
            }

            println!("Event novelty: {} events scored across {} days.", total_scored, dates.len());

            // Topic/entity novelty
            println!("Computing topic/entity novelty...");
            let mut total_te = 0u32;
            for (i, date) in dates.iter().enumerate() {
                let date = *date;

                match crate::dayline::topic_entity_novelty::compute_topic_entity_novelty(pool, date).await {
                    Ok(updated) => {
                        total_te += updated;
                        if (i + 1) % 20 == 0 {
                            println!("  topic/entity: {}/{} days", i + 1, dates.len());
                        }
                    }
                    Err(e) => {
                        eprintln!("  {} — topic/entity error: {}", date, e);
                    }
                }
            }

            println!("Topic/entity novelty: {} events updated.", total_te);
        }

        Commands::AnnotateEvents => {
            println!("Running migrations...");
            virtues.database.initialize().await?;
            println!("Annotating events (avg_hr, entities, source_ontologies)...");

            let pool = virtues.database.pool();
            let dates: Vec<chrono::NaiveDate> = sqlx::query_scalar(
                "SELECT DISTINCT d.date FROM wiki_days d \
                 JOIN wiki_events e ON e.day_id = d.id ORDER BY d.date"
            )
            .fetch_all(pool)
            .await?;

            let mut total = 0u32;
            for date in &dates {
                match crate::dayline::annotate::annotate_events_for_day(pool, *date).await {
                    Ok(n) => total += n,
                    Err(e) => eprintln!("  {} — error: {}", date, e),
                }
            }
            println!("Annotated {} events across {} days.", total, dates.len());
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

            // The two halves, in order. Segmenting is cheap and factual (Lite);
            // narrating is prose about what the day MEANT (Chat) and only happens
            // if the day earned it.
            println!("Segmenting {target_date} into events...");
            let n = crate::api::day_summary::segment_day_events(pool, target_date).await?;
            println!("  {n} events");

            println!("Narrating {target_date}...");
            let Some(day) = crate::api::day_summary::narrate_day(pool, target_date).await? else {
                println!();
                println!("· Not enough of a day to write about — no narrative, and no LLM call.");
                return Ok(());
            };

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

            let dates: Vec<chrono::NaiveDate> = sqlx::query_scalar(
                "SELECT DISTINCT d.date FROM wiki_days d \
                 JOIN wiki_events e ON e.day_id = d.id \
                 WHERE e.avg_hr IS NOT NULL \
                 ORDER BY d.date"
            )
            .fetch_all(pool)
            .await?;

            println!("Found {} days with HR data to score", dates.len());

            let mut total_scored = 0u32;
            for (i, date) in dates.iter().enumerate() {
                let date = *date;


                match crate::dayline::autonomic_scoring::compute_autonomic_for_day(pool, date).await {
                    Ok(scored) => {
                        total_scored += scored;
                        if scored > 0 || (i + 1) % 10 == 0 {
                            println!("  {} — {} events scored ({}/{})", date, scored, i + 1, dates.len());
                        }
                    }
                    Err(e) => {
                        eprintln!("  {} — error: {}", date, e);
                    }
                }
            }

            println!("Autonomic scoring: {} events scored across {} days.", total_scored, dates.len());
        }
    }

    Ok(())
}

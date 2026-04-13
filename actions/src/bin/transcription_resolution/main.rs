//! Transcription resolution cron action.
//!
//! **Device-agnostic ontology→ontology resolver.** Drains `data_audio_recording`
//! rows that don't yet have a corresponding `data_communication_transcription`
//! row, calls Gemini via Tollbooth, and writes the transcript. Runs regardless
//! of which device produced the recording (iOS today, Mac/web/imports later).
//!
//! Currently invoked every ~60s via a hand-rolled tokio interval task in
//! `core/src/server/mod.rs` — that's lunch-day duct tape. Post-lunch this
//! moves to the proper scheduler with `cron_schedule = '*/1 * * * *'` on the
//! action row, and the scheduler dispatches via `action_runner` instead of
//! match-dispatch.
//!
//! Stops early on Tollbooth 429 to preserve unprocessed recordings for the
//! next run. Failed transcriptions are retried on every cron tick — there's
//! no per-row retry cap yet (deliberately simple; we'll add it if needed).

mod transform;

use anyhow::Result;
use virtues_action_helpers::{connect_from_env, output, read_input};

/// Maximum recordings to process per cron tick. Keeps each run bounded so a
/// huge backlog doesn't hold a single Gemini connection open for minutes.
const BATCH_SIZE: i64 = 5;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    // Cron actions still receive an ActionInput on stdin (config + null payload).
    let input = read_input()?;
    let db = connect_from_env().await?;

    let (transcribed, skipped, failed) = transform::drain(&db, BATCH_SIZE).await?;
    let summary = format!(
        "transcribed: {}, skipped (silent): {}, failed: {}",
        transcribed, skipped, failed
    );

    output(&summary, &input.config)?;
    Ok(())
}

//! Transcription resolution cron action.
//!
//! **Device-agnostic ontology→ontology resolver.** Drains `data_audio_recording`
//! rows that don't yet have a corresponding `data_communication_transcription`
//! row, calls Gemini via virtues-api, and writes the transcript. Runs regardless
//! of which device produced the recording (iOS today, Mac/web/imports later).
//!
//! Currently invoked every ~60s via a hand-rolled tokio interval task in
//! `core/src/server/mod.rs` — that's lunch-day duct tape. Post-lunch this
//! moves to the proper scheduler with `cron_schedule = '*/1 * * * *'` on the
//! action row, and the scheduler dispatches via `action_runner` instead of
//! match-dispatch.
//!
//! Stops early on virtues-api 429 to preserve unprocessed recordings for the
//! next run. Failed transcriptions are retried on every cron tick — there's
//! no per-row retry cap yet (deliberately simple; we'll add it if needed).

mod transform;
mod vad;

use anyhow::Result;
use virtues_helpers::{connect_from_env, output_with_records, read_input};

/// Maximum recordings to process per cron tick. Drained sequentially, so the
/// run time is ~BATCH_SIZE × per-call latency; the action runner's 300s
/// SUBPROCESS_TIMEOUT is the ceiling. At 30 and ~2–4s/call a run finishes in
/// ~1–2 min, well inside both the timeout and the 1-min cron cadence, giving
/// ~30/min of drain (6× the old 5/2min) — enough to clear a multi-hundred
/// backlog in minutes rather than hours. A single poison row (e.g. a missing
/// audio file) now wastes 1 of 30 slots instead of 1 of 5.
const BATCH_SIZE: i64 = 30;

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    // Cron actions still receive an ActionInput on stdin (config + null payload).
    let input = read_input()?;
    let db = connect_from_env("virtues-action-transcription_resolution").await?;

    let (transcribed, skipped, failed) = transform::drain(&db, BATCH_SIZE).await?;
    let summary = format!(
        "transcribed: {}, skipped (silent): {}, failed: {}",
        transcribed, skipped, failed
    );

    // records_processed = recordings resolved this run (transcribed + silent).
    let records = (transcribed + skipped) as i64;
    output_with_records(&summary, &input.config, records)?;
    Ok(())
}

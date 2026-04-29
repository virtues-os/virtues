//! iOS Microphone push action.
//!
//! Receives audio chunks (base64-encoded) from the iPhone via `/ingest`,
//! writes the bytes to local fs, and inserts a row into `data_audio_recording`.
//! Returns success in <1s per chunk so the iOS request never times out.
//!
//! Transcription is asynchronous: a separate device-agnostic cron action
//! (`transcription_resolution`) drains untranscribed recordings from
//! `data_audio_recording` and calls Gemini via Tollbooth. See migration 048
//! for the rationale.

mod transform;

use anyhow::Result;
use virtues_helpers::{connect_from_env, output, read_input};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let input = read_input()?;
    let db = connect_from_env().await?;

    let records = input
        .payload
        .as_ref()
        .and_then(|p| p.as_array())
        .ok_or_else(|| anyhow::anyhow!("ios_microphone requires a payload array"))?;

    let (written, failed) = transform::ingest_all(&db, records).await?;
    let summary = format!("audio recordings: {} written, {} failed", written, failed);

    output(&summary, &input.config)?;
    Ok(())
}

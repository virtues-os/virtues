//! iOS EventKit action.
//!
//! Receives calendar events and reminders from the iPhone via `/ingest`.
//! Writes events to `data_calendar_event`. Reminders are skipped for now
//! (they need a different ontology table).

mod transform;

use anyhow::Result;
use virtues_action_helpers::{connect_from_env, output, read_input};

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
        .ok_or_else(|| anyhow::anyhow!("ios_eventkit requires a payload array"))?;

    let written = transform::write_events(&db, records).await?;
    let summary = format!("events: {} written", written);

    output(&summary, &input.config)?;
    Ok(())
}

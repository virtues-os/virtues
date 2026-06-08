//! iOS EventKit action.
//!
//! Receives calendar events and reminders from the iPhone via `/ingest`.
//! Writes events to `data_calendar_event`. Reminders are skipped for now
//! (they need a different ontology table).

mod transform;

use anyhow::Result;
use virtues_helpers::{connect_from_env, output, read_input};

#[tokio::main]
async fn main() -> Result<()> {
    virtues_actions::init_tracing();

    let input = read_input()?;
    let db = connect_from_env("virtues-action-ios_eventkit").await?;

    let records = input
        .payload
        .as_ref()
        .and_then(|p| {
            p.get("records")
                .and_then(|r| r.as_array())
                .or_else(|| p.as_array())
        })
        .ok_or_else(|| anyhow::anyhow!("ios_eventkit requires `records` array in payload"))?;

    let written = transform::write_events(&db, records).await?;
    let summary = format!("events: {} written", written);

    output(&summary, &input.config)?;
    Ok(())
}

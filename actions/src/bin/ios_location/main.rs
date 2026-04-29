//! iOS Location action.
//!
//! Receives location point batches from the iPhone via `/ingest` and writes
//! to `data_location_point`. Place clustering (creating `location_visit` records)
//! is handled by the separate `entity_resolution` action, not inline here.

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
        .ok_or_else(|| anyhow::anyhow!("ios_location requires a payload array"))?;

    let written = transform::write_locations(&db, records).await?;
    let summary = format!("locations: {}/{}", written, records.len());

    output(&summary, &input.config)?;
    Ok(())
}

//! iOS Contacts action.
//!
//! Receives contact batches from the iPhone via `/ingest` and resolves each
//! contact to a `wiki_people` entity. Matches existing people by email (primary)
//! or phone (fallback), creating new entities for unknowns.
//!
//! Unlike the other iOS actions, this writes to `wiki_people` (not a `data_*`
//! ontology table) because contacts ARE the canonical person records.

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
        .ok_or_else(|| anyhow::anyhow!("ios_contacts requires a payload array"))?;

    let (resolved, failed) = transform::resolve_contacts(&db, records).await?;
    let summary = format!("contacts: {} resolved, {} failed", resolved, failed);

    output(&summary, &input.config)?;
    Ok(())
}

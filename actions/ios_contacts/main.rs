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
use virtues_helpers::{connect_from_env, output, read_input};

#[tokio::main]
async fn main() -> Result<()> {
    virtues_actions::init_tracing();

    let input = read_input()?;
    let db = connect_from_env("virtues-action-ios_contacts").await?;

    let records = input
        .payload
        .as_ref()
        .and_then(|p| {
            p.get("records")
                .and_then(|r| r.as_array())
                .or_else(|| p.as_array())
        })
        .ok_or_else(|| anyhow::anyhow!("ios_contacts requires `records` array in payload"))?;

    let (resolved, failed) = transform::resolve_contacts(&db, records).await?;
    let summary = format!("contacts: {} resolved, {} failed", resolved, failed);

    output(&summary, &input.config)?;
    Ok(())
}

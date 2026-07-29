//! entity_summary: keep the wiki's entity articles current.
//!
//! Thin glue over `virtues-core` (same shape as `day_summary_eod`): connect,
//! refresh whatever entities have outgrown their article, report the count.
//! Runs hourly; the growth gate in `refresh_due_entity_summaries` is the real
//! scheduler — a tick with nothing due makes zero model calls.

use anyhow::{Context, Result};
use virtues_helpers::{output, read_input};

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    let input = read_input()?;
    let pool = virtues_helpers::connect_from_env("virtues-action-entity_summary").await?;

    let written = virtues::api::entity_summary_gen::refresh_due_entity_summaries(&pool)
        .await
        .context("entity summary refresh failed")?;

    let summary = if written == 0 {
        "no entities due for a new edition".to_string()
    } else {
        format!("wrote {written} entity summary edition(s)")
    };

    output(&summary, &input.config)
}

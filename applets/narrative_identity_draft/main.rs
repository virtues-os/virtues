//! narrative_identity_draft: generate the onboarding narrative-identity draft.
//!
//! Thin glue over `virtues-core` (same plumbing shape as `day_summary_eod` —
//! "shape" means the connect/run/report skeleton, NOT the model slot): connect,
//! run the generator, report the outcome. The generator runs on the **Chat**
//! slot (see `narrative_identity_gen`). Triggered manually from the
//! onboarding reveal (and, later, on a cron for the recurring examined-self).

use anyhow::{Context, Result};
use virtues_helpers::{output, read_input};

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    let input = read_input()?;
    let pool = virtues_helpers::connect_from_env("virtues-action-narrative_identity_draft").await?;

    let outcome = virtues::api::narrative_identity_gen::generate_narrative_identity_draft(&pool)
        .await
        .context("narrative identity draft generation failed")?;

    use virtues::api::narrative_identity_gen::DraftOutcome;
    let summary = match outcome {
        DraftOutcome::Generated => "generated narrative identity",
        DraftOutcome::Thin => "generated narrative identity (thin sketch)",
        DraftOutcome::Deferred => "deferred: insufficient data yet",
    };

    output(summary, &input.config)
}

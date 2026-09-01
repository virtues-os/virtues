//! Bookmark enrichment cron applet (agents/plan/bookmarks-plan.md §3).
//!
//! Drains `data_content_bookmark.enrichment_status = 'pending'`: fetch the
//! page, compose an extraction record, write it back. The `content_bookmark`
//! ontology then re-embeds the row through the ordinary embedding_index cron,
//! because changing the embed text changes its `doc_hash`.
//!
//! All real work lives in [`virtues::bookmark_enrichment::run_enrichment_job`];
//! this binary is the subprocess wrapper for the runner's stdin/stdout
//! contract. It has to live in core rather than here because the applet role
//! (`virtues_applet_writer`) cannot write `data_*` tables.

use anyhow::Result;
use virtues::bookmark_enrichment::run_enrichment_job;
use virtues_helpers::{connect_from_env, output, read_input};

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    let input = read_input()?;
    let pool = connect_from_env("virtues-action-bookmark_enrichment").await?;

    let summary = run_enrichment_job(&pool).await?;

    // A throttled queue and a drained queue must not read the same. Without the
    // cap line, "enriched 0 bookmarks" is what the user sees whether there is
    // nothing to do or whether their budget ran out mid-backfill.
    let mut parts = Vec::new();
    if summary.enriched > 0 {
        parts.push(format!("enriched {}", summary.enriched));
    }
    if summary.skipped > 0 {
        parts.push(format!("skipped {}", summary.skipped));
    }
    if summary.failed > 0 {
        parts.push(format!("failed {}", summary.failed));
    }
    if parts.is_empty() {
        parts.push("nothing to enrich".to_string());
    }
    if summary.remaining > 0 {
        parts.push(format!("{} awaiting enrichment", summary.remaining));
    }
    // Reported apart from `remaining` on purpose: these are held back until the
    // pixel pass exists, so folding them into the backlog would show a number
    // that cannot move and read as a stall.
    if summary.awaiting_pixels > 0 {
        parts.push(format!(
            "{} held for the image pass",
            summary.awaiting_pixels
        ));
    }
    if summary.hit_daily_cap {
        parts.push("daily budget reached".to_string());
    }

    output(&parts.join("; "), &input.config)?;
    Ok(())
}

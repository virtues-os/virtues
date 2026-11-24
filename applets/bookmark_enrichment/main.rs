//! Bookmark enrichment cron applet (agents/record/bookmarks.md).
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
use virtues::bookmark_facts::{fill_link_previews, fill_page_facts};
use virtues::bookmark_media::keep_remote_pictures;
use virtues_helpers::{connect_from_env, output, read_input};

/// Remote pictures of each kind copied into Drive per run. A copy is one image fetch, so
/// this keeps a run well inside the runner's time limit while a first sync's
/// pictures arrive over a few runs.
const PICTURES_PER_RUN: i64 = 25;

/// Pages fetched per run for facts a card shows (site name, icon, reading
/// time) and for the previews of pages posts link to. Each is one fetch and
/// no model call.
const FACTS_PER_RUN: i64 = 20;

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    let input = read_input()?;
    let pool = connect_from_env("virtues-action-bookmark_enrichment").await?;

    let summary = run_enrichment_job(&pool).await?;
    // After enrichment: read the facts of pages read before facts were kept,
    // and preview the pages posts link to. Both can name remote pictures, so
    // the copy into Drive runs last.
    let facts = fill_page_facts(&pool, FACTS_PER_RUN).await?;
    let previews = fill_link_previews(&pool, FACTS_PER_RUN).await?;
    let pictures = keep_remote_pictures(&pool, PICTURES_PER_RUN).await?;

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
    if facts.filled + previews.filled > 0 {
        parts.push(format!("read {} pages for details", facts.filled + previews.filled));
    }
    if pictures.kept > 0 {
        parts.push(format!("kept {} pictures", pictures.kept));
    }
    if pictures.failed > 0 {
        parts.push(format!("{} pictures could not be fetched", pictures.failed));
    }
    if summary.remaining > 0 {
        parts.push(format!("{} awaiting enrichment", summary.remaining));
    }
    // Reported apart from `remaining` on purpose: no pass reads these (video,
    // audio, a file that left Drive), so folding them into the backlog would
    // show a number that cannot move and read as a stall.
    if summary.unreadable_assets > 0 {
        parts.push(format!(
            "{} held: files no pass reads yet",
            summary.unreadable_assets
        ));
    }
    if summary.hit_daily_cap {
        parts.push("daily budget reached".to_string());
    }

    output(&parts.join("; "), &input.config)?;
    Ok(())
}

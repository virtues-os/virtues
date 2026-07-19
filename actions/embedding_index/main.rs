//! Embedding indexer cron action.
//!
//! Sweeps every searchable ontology (those with an `EmbeddingConfig` registered),
//! finds records lacking an entry in `search_embeddings`, embeds the configured
//! `embed_text_sql` via the local ORT embedder (nomic-embed-text-v1.5),
//! and writes results into `search_embeddings` + `search_vectors` atomically.
//!
//! All real work lives in [`virtues::search::run_embedding_job`]. This binary
//! is the subprocess wrapper that gives the runner stdin/stdout contract.
//!
//! Triggered every 15 minutes per `templates.toml`. The job drains: it loops
//! batches until the backlog is empty (or its wall-clock ceiling trips), so a
//! large onboarding backlog clears in one run instead of trickling one batch
//! per cron tick. A pg advisory lock inside the job makes an overlapping cron
//! tick no-op cleanly.

use anyhow::Result;
use virtues::search::run_embedding_job;
use virtues_helpers::{connect_from_env, output, read_input};

#[tokio::main]
async fn main() -> Result<()> {
    virtues_actions::init_tracing();

    let input = read_input()?;
    let pool = connect_from_env("virtues-action-embedding_index").await?;

    let embedded = run_embedding_job(&pool).await?;

    let summary = if embedded == 0 {
        "no new records to embed".to_string()
    } else {
        format!(
            "embedded {} record{}",
            embedded,
            if embedded == 1 { "" } else { "s" }
        )
    };

    output(&summary, &input.config)?;
    Ok(())
}

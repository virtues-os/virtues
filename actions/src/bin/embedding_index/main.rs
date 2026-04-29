//! Embedding indexer cron action.
//!
//! Sweeps every searchable ontology (those with an `EmbeddingConfig` registered),
//! finds records lacking an entry in `search_embeddings`, embeds the configured
//! `embed_text_sql` via the local fastembed model, and writes results into
//! `search_embeddings` + `vec_search` atomically.
//!
//! All real work lives in [`virtues::search::run_embedding_job`]. This binary
//! is the subprocess wrapper that gives the runner stdin/stdout contract.
//!
//! Triggered every 15 minutes per `templates.toml`.

use anyhow::Result;
use virtues::database::register_sqlite_vec_extension;
use virtues::search::run_embedding_job;
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

    // sqlite-vec must be registered before any connection so vec_search
    // (the vec0 virtual table) is available.
    register_sqlite_vec_extension();

    let input = read_input()?;
    let pool = connect_from_env().await?;

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

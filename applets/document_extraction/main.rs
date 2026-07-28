//! Document extraction cron action (researcher-plan D1).
//!
//! Drains `app_drive_files.extraction_status = 'pending'`: native-text extract
//! (pdfium / docx / text / html — no OCR), paragraph-aware chunking, upsert
//! into `extracted_document_chunks`. The `uploaded_document` ontology then
//! embeds new chunks via the embedding_index cron.
//!
//! All real work lives in [`virtues::extraction::run_extraction_job`]; this
//! binary is the subprocess wrapper for the runner's stdin/stdout contract.

use anyhow::Result;
use std::sync::Arc;
use virtues::api::DriveConfig;
use virtues::extraction::run_extraction_job;
use virtues::storage::Storage;
use virtues_helpers::{connect_from_env, output, read_input};

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    let input = read_input()?;
    let pool = connect_from_env("virtues-action-document_extraction").await?;

    // The server's resolver, called rather than re-implemented — this applet is a
    // separate process, and a second expression of the rule is how the two drift.
    let storage = Storage::file(
        virtues::storage::lake::lake_root()
            .to_string_lossy()
            .into_owned(),
    )?;
    let config = DriveConfig::new(Arc::new(storage));

    let processed = run_extraction_job(&pool, &config).await?;

    let summary = if processed == 0 {
        "no documents pending extraction".to_string()
    } else {
        format!(
            "extracted {} document{}",
            processed,
            if processed == 1 { "" } else { "s" }
        )
    };

    output(&summary, &input.config)?;
    Ok(())
}

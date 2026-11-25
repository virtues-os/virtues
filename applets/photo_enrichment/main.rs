//! Photo favorites cron applet: describe the pictures that arrived, and delete
//! the ones no photo shows any more.
//!
//! The work is [`virtues::photo_enrichment::run_photo_job`] and
//! [`virtues::api::photos::release_unused`]; this binary is the subprocess
//! wrapper for the runner's stdin/stdout contract.

use anyhow::Result;
use virtues::api::photos::release_unused;
use virtues::api::DriveConfig;
use virtues::photo_enrichment::run_photo_job;
use virtues::storage::lake::storage_from_env;
use virtues_helpers::{connect_from_env, output, read_input};

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    let input = read_input()?;
    let pool = connect_from_env("virtues-action-photo_enrichment").await?;

    // Before the model calls, so a removed photo's picture goes even on a run
    // where describing fails.
    let config = DriveConfig::new(std::sync::Arc::new(storage_from_env()?));
    let released = release_unused(&pool, &config).await?;
    let summary = run_photo_job(&pool).await?;

    let mut parts = Vec::new();
    if summary.described > 0 {
        parts.push(format!("described {}", summary.described));
    }
    if summary.skipped > 0 {
        parts.push(format!("skipped {}", summary.skipped));
    }
    if summary.failed > 0 {
        parts.push(format!("failed {}", summary.failed));
    }
    if parts.is_empty() {
        parts.push("nothing to describe".to_string());
    }
    if released > 0 {
        parts.push(format!("deleted {released} pictures no photo shows"));
    }
    if summary.remaining > 0 {
        parts.push(format!("{} waiting", summary.remaining));
    }
    if summary.stopped {
        parts.push("stopped: no model to ask".to_string());
    }
    if summary.hit_daily_cap {
        parts.push("daily limit reached".to_string());
    }

    output(&parts.join("; "), &input.config)?;
    Ok(())
}

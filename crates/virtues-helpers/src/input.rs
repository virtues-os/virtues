//! stdin/stdout I/O for action subprocess binaries.
//!
//! `AppletInput` / `AppletOutput` types live in `crate::contract` (single
//! source of truth — runner serializes from owned values, subprocess
//! deserializes here). This module wraps stdin/stdout reads and writes.

use anyhow::{Context, Result};

pub use crate::contract::{AppletInput, AppletOutput};

/// Read an `AppletInput` from stdin.
pub fn read_input() -> Result<AppletInput> {
    serde_json::from_reader(std::io::stdin())
        .context("failed to parse AppletInput from stdin")
}

/// Write the result and updated config to stdout as a single JSON object.
pub fn output(result: &str, config: &serde_json::Value) -> Result<()> {
    let out = AppletOutput::new(result, config.clone());
    serde_json::to_writer(std::io::stdout(), &out)
        .context("failed to write AppletOutput to stdout")?;
    Ok(())
}

/// Like [`output`], but also reports how many records this run processed (the
/// count lands in `app_applet_runs.records_processed` for the Telemetry tab).
pub fn output_with_records(result: &str, config: &serde_json::Value, records: i64) -> Result<()> {
    let out = AppletOutput::new(result, config.clone()).with_records(records);
    serde_json::to_writer(std::io::stdout(), &out)
        .context("failed to write AppletOutput to stdout")?;
    Ok(())
}

//! stdin/stdout I/O for action subprocess binaries.
//!
//! `ActionInput` / `ActionOutput` types live in `crate::contract` (single
//! source of truth — runner serializes from owned values, subprocess
//! deserializes here). This module wraps stdin/stdout reads and writes.

use anyhow::{Context, Result};

pub use crate::contract::{ActionInput, ActionOutput};

/// Read an `ActionInput` from stdin.
pub fn read_input() -> Result<ActionInput> {
    serde_json::from_reader(std::io::stdin())
        .context("failed to parse ActionInput from stdin")
}

/// Write the result and updated config to stdout as a single JSON object.
pub fn output(result: &str, config: &serde_json::Value) -> Result<()> {
    let out = ActionOutput::new(result, config.clone());
    serde_json::to_writer(std::io::stdout(), &out)
        .context("failed to write ActionOutput to stdout")?;
    Ok(())
}

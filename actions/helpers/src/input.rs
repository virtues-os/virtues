//! stdin/stdout JSON contract for action subprocesses.
//!
//! Every action binary reads an `ActionInput` from stdin and writes an `ActionOutput`
//! to stdout. The runner saves the returned `config` back to the action row.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Input piped to the action subprocess via stdin.
#[derive(Debug, Deserialize)]
pub struct ActionInput {
    /// Settings + code-managed state from `app_actions.config`.
    pub config: serde_json::Value,
    /// Decrypted credentials from `action_credentials`, resolved by the runner.
    /// `None` if the action has no `credential_id`.
    pub credentials: Option<serde_json::Value>,
    /// Ingest records for push actions, `None` for cron/manual triggers.
    pub payload: Option<serde_json::Value>,
}

/// Output written to stdout as a single JSON object.
/// Runner saves `config` back to the action row after a successful run.
#[derive(Debug, Serialize)]
pub struct ActionOutput<'a> {
    pub result: &'a str,
    pub config: &'a serde_json::Value,
}

/// Read an `ActionInput` from stdin.
pub fn read_input() -> Result<ActionInput> {
    let input: ActionInput = serde_json::from_reader(std::io::stdin())
        .context("failed to parse ActionInput from stdin")?;
    Ok(input)
}

/// Write the result and updated config to stdout as a single JSON object.
pub fn output(result: &str, config: &serde_json::Value) -> Result<()> {
    let out = ActionOutput { result, config };
    serde_json::to_writer(std::io::stdout(), &out)
        .context("failed to write ActionOutput to stdout")?;
    Ok(())
}

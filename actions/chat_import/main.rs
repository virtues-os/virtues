//! Chat import — one-time ingest of a Claude / ChatGPT / Gemini conversation
//! export into the AI-chat ontology.
//!
//! Manual-trigger only (no cron). The `/api/chat-import/upload` route stores the
//! uploaded export to a local path and fires this action with
//! `payload = { "file_path": "...", "provider": "claude|chatgpt|gemini" }`.
//! We parse box-side, normalize to conversation messages, and dedup-insert into
//! `data_content_conversation` keyed on `source_stream_id` — so re-importing a
//! fresher export only adds the new messages (`ON CONFLICT DO NOTHING`).

mod transform;

use anyhow::{Context, Result};
use virtues_helpers::{connect_from_env, output, read_input};

#[tokio::main]
async fn main() -> Result<()> {
    virtues_actions::init_tracing();

    let input = read_input()?;
    let payload = input.payload.clone().unwrap_or_default();
    let file_path = payload
        .get("file_path")
        .and_then(|v| v.as_str())
        .context("chat_import: payload.file_path is required")?;
    let provider = payload
        .get("provider")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let raw = std::fs::read_to_string(file_path)
        .with_context(|| format!("chat_import: failed to read {file_path}"))?;
    let json: serde_json::Value =
        serde_json::from_str(&raw).context("chat_import: export was not valid JSON")?;

    let messages = transform::parse(provider, &json);
    let parsed = messages.len();

    let pool = connect_from_env("virtues-action-chat_import").await?;
    let written = transform::write_messages(&pool, &messages).await?;

    // Best-effort cleanup of the transient upload (it's input, not a kept file).
    let _ = std::fs::remove_file(file_path);

    output(
        &format!("Imported {written} new messages ({parsed} parsed) from {provider}"),
        &input.config,
    )?;
    Ok(())
}

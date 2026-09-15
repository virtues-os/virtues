//! Reply from the record — the binary.
//!
//! Runs when the owner asks for a draft and at no other time. Two ways in,
//! one payload shape:
//! - the Mac's "draft a reply" button, which is a manual run of this applet;
//! - chat's `run_applet` tool, same payload.
//!
//! Payload: `{"thread_id": "…"}` for one thread, `{"thread_ids": […]}` for
//! several, or nothing at all, meaning the thread that most recently messaged
//! the owner.
//!
//! Everything it does lives in `virtues_applets::message_reply`; this file
//! only reads the payload and reports.

use anyhow::Result;
use virtues_applets::message_reply;
use virtues_helpers::{connect_from_env, output_with_records, read_input};

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();
    let input = read_input()?;
    let pool = connect_from_env("virtues-action-message_reply").await?;

    let payload = input.payload.clone().unwrap_or_default();
    let mut threads: Vec<String> = payload
        .get("thread_ids")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str()).map(str::to_string).collect())
        .unwrap_or_default();
    if let Some(one) = payload.get("thread_id").and_then(|v| v.as_str()) {
        if !one.is_empty() && !threads.iter().any(|t| t == one) {
            threads.push(one.to_string());
        }
    }
    if threads.is_empty() {
        let channel = payload.get("channel").and_then(|v| v.as_str()).unwrap_or("imessage");
        match message_reply::latest_inbound_thread(&pool, channel).await? {
            Some(t) => threads.push(t),
            None => {
                output_with_records("no thread has messaged you yet", &input.config, 0)?;
                return Ok(());
            }
        }
    }

    let out = message_reply::consider_threads(&pool, &threads).await?;
    let mut summary = format!(
        "{} thread(s) considered, {} drafted, {} skipped",
        out.considered, out.drafted, out.skipped
    );
    if !out.skipped_why.is_empty() {
        summary.push_str(" — ");
        summary.push_str(&out.skipped_why.join("; "));
    }
    output_with_records(&summary, &input.config, out.drafted as i64)?;
    Ok(())
}

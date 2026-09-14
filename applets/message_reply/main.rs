//! Reply from the record — the binary.
//!
//! Three ways in, one payload shape:
//! - ingest (`mac_ingest`) spawns this detached with `{"thread_ids": […],
//!   "trigger": "auto"}` after a batch that carried inbound messages;
//! - the Mac app's "take care of this" hits the manual run endpoint, with a
//!   `thread_id` or with nothing, meaning the thread that most recently
//!   messaged the owner;
//! - chat's `run_applet` tool, same payload.
//!
//! Everything it does lives in `virtues_applets::message_reply`; this file
//! only reads the payload and reports.

use anyhow::Result;
use virtues_applets::message_reply::{self, TRIGGER_AUTO, TRIGGER_MANUAL};
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
    // Only ingest says "auto". A person or the model asking is manual, which
    // skips the gate: they want a draft, not an opinion on whether one is due.
    let trigger = match payload.get("trigger").and_then(|v| v.as_str()) {
        Some(TRIGGER_AUTO) => TRIGGER_AUTO,
        _ => TRIGGER_MANUAL,
    };

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

    let out = message_reply::consider_threads(&pool, &threads, trigger).await?;
    let mut summary = format!(
        "{trigger}: {} thread(s) considered, {} drafted, {} skipped",
        out.considered, out.drafted, out.skipped
    );
    if !out.skipped_why.is_empty() {
        summary.push_str(" — ");
        summary.push_str(&out.skipped_why.join("; "));
    }
    output_with_records(&summary, &input.config, out.drafted as i64)?;
    Ok(())
}

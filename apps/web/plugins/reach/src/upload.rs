//! Upload coordinator — drains the shared outbox to the box over iroh.
//!
//! Native collectors enqueue box-shaped records into the durable outbox
//! (`virtues_reach_client::outbox`, one row per record, deterministic id). This
//! reads due rows per stream, batches them into `{stream, records}`, POSTs to
//! the box's `ios_ingest` webhook over the warm iroh client, and **deletes rows
//! only after the box durably acks** (`{status:"success"}`). At-least-once +
//! idempotent: a crash between ack and delete re-sends the same ids and the box
//! dedups on `source_stream_id`.

use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use serde_json::json;
use virtues_reach_client::{outbox, PairedBox, VirtuesIrohClient};

/// Per-chunk bounds — a few thousand records / ~2 MB keeps each ingest fast
/// (server inserts in 500-row ON CONFLICT batches) and cheap to retry.
const MAX_BYTES: usize = 2 * 1024 * 1024;
const MAX_ROWS: usize = 1000;

/// Stop starting new chunks once this much wall-clock has elapsed, so a drain
/// triggered in a ~30 s background window never gets killed mid-flight.
const TIME_BUDGET: Duration = Duration::from_secs(22);

/// Drain all due streams to the box. Returns the number of records acked.
pub async fn drain(client: &VirtuesIrohClient, rec: &PairedBox) -> Result<usize> {
    let started = Instant::now();
    let mut total = 0usize;

    for stream in outbox::due_streams()? {
        if started.elapsed() >= TIME_BUDGET {
            break;
        }
        // Resolve the concrete action id for this stream's ingest action.
        let action_key = outbox::action_key_for(&stream)?.unwrap_or_else(|| "ios_ingest".into());
        let action_id = rec
            .action_ids
            .get(&action_key)
            .or_else(|| rec.action_ids.get("ios_ingest"))
            .or_else(|| rec.action_ids.values().next())
            .ok_or_else(|| anyhow!("no ingest action id in pairing — re-pair to fix"))?
            .clone();

        // Drain this stream in chunks until dry or out of time.
        loop {
            if started.elapsed() >= TIME_BUDGET {
                break;
            }
            let batch = outbox::claim_batch(&stream, MAX_BYTES, MAX_ROWS)?;
            if batch.ids.is_empty() {
                break;
            }
            match post_batch(client, &action_id, &stream, &batch.records).await {
                Ok(true) => {
                    outbox::ack(&batch.ids)?;
                    total += batch.ids.len();
                }
                // Delivered but not durable, or transport error — release the
                // claim + back off, leave the rows for the next drain.
                Ok(false) | Err(_) => {
                    outbox::nack(&batch.ids)?;
                    break;
                }
            }
        }
    }
    Ok(total)
}

/// POST one batch; returns `true` only on a durable `{status:"success"}`.
async fn post_batch(
    client: &VirtuesIrohClient,
    action_id: &str,
    stream: &str,
    records: &[serde_json::Value],
) -> Result<bool> {
    let body = json!({ "stream": stream, "records": records }).to_string();
    let raw = format!(
        "POST /webhook/{action_id} HTTP/1.1\r\nHost: box\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let resp = client.request(raw.as_bytes()).await?;
    let text = String::from_utf8_lossy(&resp);
    Ok(body_acks(&text))
}

/// The box returns `{"status":"success"}` on a durable ingest. Anything else
/// (skipped/running/error) is retryable — leave the rows queued.
fn body_acks(resp: &str) -> bool {
    let body = resp.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or(resp);
    body.contains("\"status\":\"success\"") || body.contains("\"status\": \"success\"")
}

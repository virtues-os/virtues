//! Plaid transactions sync.
//!
//! Cron-driven, per-credential. Uses Plaid's `/transactions/sync` endpoint
//! which is incremental — we pass the cursor from our last sync, Plaid returns
//! `added`, `modified`, `removed`, and a new cursor.
//!
//! Cursor stored in `app_actions.config.plaid_cursor`. First run: empty string
//! (Plaid interprets as "give me everything").

mod transform;

use anyhow::Result;
use serde_json::{json, Value};
use virtues::storage::lake;
use virtues_helpers::{connect_from_env, output, read_input};

const MAX_CHUNKS: u32 = 20;
const ACTION: &str = "plaid_transactions_sync";

#[tokio::main]
async fn main() -> Result<()> {
    virtues_actions::init_tracing();

    let mut input = read_input()?;
    let pool = connect_from_env("virtues-action-plaid_transactions_sync").await?;

    let access_token = virtues_actions::secret(&input, "access_token")?
        .to_string();

    let mut cursor = input
        .config
        .get("plaid_cursor")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let storage = lake::storage_from_env()?;
    let mut total_written = 0usize;
    let mut total_removed = 0usize;

    for _ in 0..MAX_CHUNKS {
        // Proxied through virtues-api: the box sends only its per-user
        // access_token; the master Plaid secret stays server-side.
        let resp: Value = virtues_actions::plaid_proxy(
            &pool,
            "transactions/sync",
            &json!({
                "access_token": access_token,
                "cursor": cursor,
            }),
        )
        .await?;

        // Archive the WHOLE response before touching it. `removed` in particular is
        // read by nobody below and would otherwise be gone the moment this loop moves
        // on — and Plaid's cursor is consumed, so there is no asking for it twice.
        lake::archive_cloud(&pool, &storage, "plaid", ACTION, "transactions", &[resp.clone()])
            .await?;

        let added = resp
            .get("added")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        // `modified` is Plaid CORRECTING a transaction it already sent us — a pending
        // $50 pre-auth settling at $43.17, a merchant name resolving from
        // "SQ *XXXXX" to a real one. It shares the insert path with `added`, and that
        // path was ON CONFLICT DO NOTHING, so every correction Plaid has ever sent was
        // silently dropped: the pre-auth amount stayed on the books forever. It now
        // upserts (see transform::flush).
        let modified = resp
            .get("modified")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let mut combined = added;
        combined.extend(modified);

        let written = transform::write_transactions(&pool, &combined).await?;
        total_written += written;

        // `removed` was never read at all. Plaid tells us a transaction was reversed or
        // deleted upstream; we discarded the message, and it stayed in the user's
        // finances permanently. Tombstone rather than DELETE — `deleted_at_source`
        // exists precisely so that "the bank took this back" is itself a fact we keep,
        // instead of a hole where a transaction used to be.
        let removed = resp
            .get("removed")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        total_removed += transform::tombstone_removed(&pool, &removed).await?;

        if let Some(next) = resp.get("next_cursor").and_then(|v| v.as_str()) {
            cursor = next.to_string();
        }
        let has_more = resp.get("has_more").and_then(|v| v.as_bool()).unwrap_or(false);
        if !has_more {
            break;
        }
    }

    input.config["plaid_cursor"] = Value::String(cursor);
    let summary = format!("synced {total_written} Plaid transactions, {total_removed} removed");
    output(&summary, &input.config)
}

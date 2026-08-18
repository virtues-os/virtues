//! Plaid transactions sync.
//!
//! Cron-driven, per-credential. Uses Plaid's `/transactions/sync` endpoint
//! which is incremental — we pass the cursor from our last sync, Plaid returns
//! `added`, `modified`, `removed`, and a new cursor.
//!
//! Cursor stored in `app_applets.config.plaid_cursor`. First run: empty string
//! (Plaid interprets as "give me everything").

mod transform;

// Share the accounts writer rather than duplicate it — same package, so a path
// module is enough. `/transactions/sync` returns the full `accounts` array
// alongside the transactions, so this binary can satisfy its own foreign key
// instead of depending on when `plaid_accounts_sync` last ran (see main()).
#[path = "../plaid_accounts_sync/transform.rs"]
mod accounts_transform;

use anyhow::{Context, Result};
use serde_json::{json, Value};
use virtues::storage::lake;
use virtues_helpers::{connect_from_env, output, read_input};

const MAX_CHUNKS: u32 = 20;
const ACTION: &str = "plaid_transactions_sync";

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    let mut input = read_input()?;
    let pool = connect_from_env("virtues-action-plaid_transactions_sync").await?;

    let access_token = virtues_applets::secret(&input, "access_token")?
        .to_string();

    // Identity for any account rows this run has to create (below).
    let creds = input
        .credentials
        .as_ref()
        .context("plaid credentials missing")?;
    let item_id = creds
        .get("metadata")
        .and_then(|m| m.get("item_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let institution = creds
        .get("metadata")
        .and_then(|m| m.get("institution_name"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("Unknown")
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
        let resp: Value = virtues_applets::service_proxy(
            &pool,
        "plaid",
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

        // Write the accounts this page's transactions point at, FIRST.
        //
        // `data_financial_transaction.account_id` is a foreign key, and on a
        // freshly connected Item no account rows exist yet: `plaid_accounts_sync`
        // is on a 6-hour cron, so the first several transaction runs used to die
        // on a FK violation and nothing landed until that cron happened to fire.
        // `/transactions/sync` returns the same `accounts` array that
        // `/accounts/get` does, so the dependency is satisfiable from this very
        // response — no ordering between the two applets required. The write is
        // an idempotent upsert keyed on `plaid:account:{id}`, exactly what
        // `plaid_accounts_sync` writes, so the two agree rather than fight.
        let accounts = resp
            .get("accounts")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if !accounts.is_empty() {
            accounts_transform::write_accounts(&pool, &item_id, &institution, &accounts).await?;
        }

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

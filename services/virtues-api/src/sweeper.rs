//! Background housekeeping for virtues-api.
//!
//! Deletes dead rows:
//!   - long-expired accounts (a lapsed subscription's wallet expires at the
//!     cohort boundary; kept a short grace, then reclaimed). The `device_keys`
//!     and `ledger` rows cascade / are reclaimed with the account.
//!   - expired blocklist entries (TTL'd cooldowns; restart snapshot only).

use sqlx::PgPool;
use std::time::Duration;

const SWEEP_INTERVAL: Duration = Duration::from_secs(60 * 60); // hourly
const ACCOUNT_GRACE_DAYS: i64 = 7;

pub fn spawn(pool: PgPool) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(SWEEP_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            run_once(&pool).await;
        }
    });
}

async fn run_once(pool: &PgPool) {
    // Long-expired accounts, past grace. device_keys cascade; ledger rows go
    // with them (we keep the account row while the wallet is live, so history
    // is available to the user until the subscription lapses + grace).
    match sqlx::query(&format!(
        "DELETE FROM accounts WHERE expires_at < now() - interval '{ACCOUNT_GRACE_DAYS} days'"
    ))
    .execute(pool)
    .await
    {
        Ok(r) if r.rows_affected() > 0 => {
            tracing::info!(deleted = r.rows_affected(), "swept long-expired accounts")
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("account sweep failed: {e:#}"),
    }

    // Expired blocklist entries (blocks are TTL'd cooldowns). The in-memory
    // map is pruned separately by `Blocklist::spawn_pruner`; this clears the
    // restart snapshot so a reboot doesn't re-load stale blocks.
    match sqlx::query("DELETE FROM blocklist WHERE expires_at < now()")
        .execute(pool)
        .await
    {
        Ok(r) if r.rows_affected() > 0 => {
            tracing::info!(deleted = r.rows_affected(), "swept expired blocks")
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("blocklist sweep failed: {e:#}"),
    }
}

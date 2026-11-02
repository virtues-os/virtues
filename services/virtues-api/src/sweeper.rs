//! Background housekeeping for virtues-api.
//!
//! Deletes dead rows:
//!   - long-expired wallets (a lapsed subscription's wallet expires at the
//!     cohort boundary; kept a short grace, then reclaimed). Its `ledger` rows
//!     go with it. The box keys stay: they belong to the account, and they are
//!     what keeps the free routes, such as the OAuth token proxy, working.
//!   - expired blocklist entries (TTL'd cooldowns; restart snapshot only).

use sqlx::PgPool;
use std::time::Duration;

const SWEEP_INTERVAL: Duration = Duration::from_secs(60 * 60); // hourly
const WALLET_GRACE_DAYS: i64 = 7;

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
    // Long-expired wallets, past grace. Ledger rows cascade (the history stays
    // available to the owner until the subscription lapses + grace).
    match sqlx::query(&format!(
        "DELETE FROM wallets WHERE expires_at < now() - interval '{WALLET_GRACE_DAYS} days'"
    ))
    .execute(pool)
    .await
    {
        Ok(r) if r.rows_affected() > 0 => {
            tracing::info!(deleted = r.rows_affected(), "swept long-expired wallets")
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entitlement::{self, CreditMode};

    /// Sweeping a lapsed wallet takes its ledger and leaves the box key, so
    /// the box still authenticates, and the owner's next payment brings the
    /// wallet back under the same key without linking again.
    #[sqlx::test]
    async fn sweep_keeps_the_key_and_a_later_credit_restores_the_wallet(pool: PgPool) {
        entitlement::credit(&pool, "acct-lapsed", 1_000_000, CreditMode::Set, None)
            .await
            .unwrap();
        entitlement::register_device(&pool, b"keyhash-lapsed-0000000000000", "acct-lapsed", Some("box-a"))
            .await
            .unwrap();
        sqlx::query("UPDATE wallets SET expires_at = now() - interval '8 days' WHERE account_id = 'acct-lapsed'")
            .execute(&pool)
            .await
            .unwrap();

        run_once(&pool).await;

        assert!(entitlement::get_wallet(&pool, "acct-lapsed").await.unwrap().is_none(), "wallet swept");
        let (ledger,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM ledger WHERE account_id = 'acct-lapsed'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(ledger, 0, "the ledger goes with its wallet");
        assert_eq!(
            entitlement::key_account(&pool, b"keyhash-lapsed-0000000000000").await.unwrap().as_deref(),
            Some("acct-lapsed"),
            "the key outlives the wallet"
        );

        entitlement::credit(&pool, "acct-lapsed", 20_000_000, CreditMode::Set, None)
            .await
            .unwrap();
        let wallet = entitlement::get_wallet(&pool, "acct-lapsed").await.unwrap().expect("renewed");
        assert_eq!(wallet.balance_micros, 20_000_000);
        assert!(wallet.expires_at > chrono::Utc::now());
    }
}

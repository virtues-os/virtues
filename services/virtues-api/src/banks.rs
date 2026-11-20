//! The banks an account has linked through Plaid, and the cap on them.
//!
//! Bank data is included in the subscription, so a call to Plaid costs the
//! wallet nothing. What costs Virtues money is the number of linked banks:
//! Plaid bills a monthly fee for each until it is removed. So the limit is on
//! banks per account, checked when a newly linked bank is collected
//! (`oauth.rs` exchange), and a removed bank frees its place
//! (`routes/plaid.rs` item/remove).
//!
//! Banks linked before this table existed are not in it, so they do not
//! count until they are linked again. Removing one that is not here is a
//! no-op.

use anyhow::Result;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

/// Linked banks allowed per account. Far above ordinary use; it exists to
/// bound what one account can cost. Env: `PLAID_MAX_BANKS`.
pub const DEFAULT_MAX_BANKS: i64 = 10;

pub fn max_banks() -> i64 {
    std::env::var("PLAID_MAX_BANKS")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|n: &i64| *n > 0)
        .unwrap_or(DEFAULT_MAX_BANKS)
}

fn item_hash(item_id: &str) -> Vec<u8> {
    Sha256::digest(item_id.as_bytes()).to_vec()
}

/// What happened when a newly linked bank was offered to the account.
#[derive(Debug, PartialEq, Eq)]
pub enum Admit {
    /// Recorded, or already recorded.
    Admitted,
    /// The account already has `max` banks. Nothing was recorded.
    AtLimit { max: i64 },
}

/// Record a newly linked bank unless the account is at the limit. A bank
/// already recorded is admitted again without counting twice. The count and
/// the insert run under a per-account advisory lock so two banks linked at
/// once cannot both take the last place.
pub async fn admit(pool: &PgPool, account_id: &str, item_id: &str, max: i64) -> Result<Admit> {
    let hash = item_hash(item_id);
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('linked_banks:' || $1))")
        .bind(account_id)
        .execute(&mut *tx)
        .await?;
    let known: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM linked_banks WHERE account_id = $1 AND item_hash = $2)",
    )
    .bind(account_id)
    .bind(&hash)
    .fetch_one(&mut *tx)
    .await?;
    if !known {
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM linked_banks WHERE account_id = $1")
            .bind(account_id)
            .fetch_one(&mut *tx)
            .await?;
        if count >= max {
            return Ok(Admit::AtLimit { max });
        }
        sqlx::query("INSERT INTO linked_banks (account_id, item_hash) VALUES ($1, $2)")
            .bind(account_id)
            .bind(&hash)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Admit::Admitted)
}

/// Forget a removed bank. Removing one that is not recorded is a no-op.
pub async fn forget(pool: &PgPool, account_id: &str, item_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM linked_banks WHERE account_id = $1 AND item_hash = $2")
        .bind(account_id)
        .bind(item_hash(item_id))
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test]
    async fn admits_up_to_the_limit_then_refuses(pool: PgPool) {
        for i in 0..3 {
            assert_eq!(admit(&pool, "acct", &format!("item-{i}"), 3).await.unwrap(), Admit::Admitted);
        }
        assert_eq!(admit(&pool, "acct", "item-3", 3).await.unwrap(), Admit::AtLimit { max: 3 });
        // Another account has its own places.
        assert_eq!(admit(&pool, "other", "item-3", 3).await.unwrap(), Admit::Admitted);
    }

    #[sqlx::test]
    async fn a_bank_already_recorded_does_not_count_twice(pool: PgPool) {
        assert_eq!(admit(&pool, "acct", "item-a", 1).await.unwrap(), Admit::Admitted);
        assert_eq!(admit(&pool, "acct", "item-a", 1).await.unwrap(), Admit::Admitted);
    }

    #[sqlx::test]
    async fn forgetting_a_bank_frees_its_place_and_is_harmless_twice(pool: PgPool) {
        assert_eq!(admit(&pool, "acct", "item-a", 1).await.unwrap(), Admit::Admitted);
        assert_eq!(admit(&pool, "acct", "item-b", 1).await.unwrap(), Admit::AtLimit { max: 1 });
        forget(&pool, "acct", "item-a").await.unwrap();
        forget(&pool, "acct", "item-a").await.unwrap();
        assert_eq!(admit(&pool, "acct", "item-b", 1).await.unwrap(), Admit::Admitted);
    }
}

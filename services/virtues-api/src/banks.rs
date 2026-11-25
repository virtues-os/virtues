//! The banks an account has linked through Plaid, and the cap on them.
//!
//! Bank data is included in the subscription, so a call to Plaid costs the
//! wallet nothing. What costs Virtues money is the number of linked banks:
//! Plaid bills a monthly fee for each until it is removed. So the limit is on
//! banks per account, checked when a newly linked bank is collected
//! (`oauth.rs` exchange), and a removed bank frees its place
//! (`routes/plaid.rs` item/remove).
//!
//! The table also says whose bank an access token is. Every bank call names
//! a token, and any subscriber's key reaches the bank routes, so a call is
//! served only when the token's bank is the caller's own ([`token_owner`]).
//!
//! Banks linked before this table existed are not in it, and banks recorded
//! before `token_hash` existed have none. The first call with such a token
//! records it for the caller ([`claim`]), unless another account already has
//! that bank. Removing a bank that is not here is a no-op.

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

fn token_hash(access_token: &str) -> Vec<u8> {
    Sha256::digest(access_token.as_bytes()).to_vec()
}

/// What happened when a newly linked bank was offered to the account.
#[derive(Debug, PartialEq, Eq)]
pub enum Admit {
    /// Recorded, or already recorded.
    Admitted,
    /// The account already has `max` banks. Nothing was recorded.
    AtLimit { max: i64 },
}

/// Record a newly linked bank and its access token unless the account is at
/// the limit. A bank already recorded is admitted again without counting
/// twice, and takes the new token. The count and the insert run under a
/// per-account advisory lock so two banks linked at once cannot both take the
/// last place.
pub async fn admit(pool: &PgPool, account_id: &str, item_id: &str, access_token: &str, max: i64) -> Result<Admit> {
    let hash = item_hash(item_id);
    let token = token_hash(access_token);
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
    if known {
        sqlx::query("UPDATE linked_banks SET token_hash = $3 WHERE account_id = $1 AND item_hash = $2")
            .bind(account_id)
            .bind(&hash)
            .bind(&token)
            .execute(&mut *tx)
            .await?;
    } else {
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM linked_banks WHERE account_id = $1")
            .bind(account_id)
            .fetch_one(&mut *tx)
            .await?;
        if count >= max {
            return Ok(Admit::AtLimit { max });
        }
        sqlx::query("INSERT INTO linked_banks (account_id, item_hash, token_hash) VALUES ($1, $2, $3)")
            .bind(account_id)
            .bind(&hash)
            .bind(&token)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Admit::Admitted)
}

/// Whose bank an access token is, as far as this table knows.
#[derive(Debug, PartialEq, Eq)]
pub enum TokenOwner {
    /// One of the caller's banks.
    Caller,
    /// A bank recorded for a different account.
    Another,
    /// No recorded bank has this token. The caller asks Plaid which bank it
    /// is and offers it to [`claim`].
    Unknown,
}

pub async fn token_owner(pool: &PgPool, account_id: &str, access_token: &str) -> Result<TokenOwner> {
    let owners: Vec<String> = sqlx::query_scalar("SELECT account_id FROM linked_banks WHERE token_hash = $1")
        .bind(token_hash(access_token))
        .fetch_all(pool)
        .await?;
    Ok(if owners.iter().any(|o| o == account_id) {
        TokenOwner::Caller
    } else if owners.is_empty() {
        TokenOwner::Unknown
    } else {
        TokenOwner::Another
    })
}

/// Record `access_token` as the token of bank `item_id` for the caller, for a
/// token [`token_owner`] did not know. Returns whether the bank is now the
/// caller's: `false` means another account has it and nothing changed.
///
/// A bank no account has recorded goes to the caller, whatever the limit:
/// it was linked before this table existed, it already bills at Plaid, and
/// refusing it would stop it syncing. Until its owner's server makes that
/// first call, a copied token for such a bank would be accepted from another
/// account. Runs under a per-bank advisory lock so two accounts cannot both
/// take one bank.
pub async fn claim(pool: &PgPool, account_id: &str, item_id: &str, access_token: &str) -> Result<bool> {
    let hash = item_hash(item_id);
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('linked_bank:' || encode($1, 'hex')))")
        .bind(&hash)
        .execute(&mut *tx)
        .await?;
    let owners: Vec<String> = sqlx::query_scalar("SELECT account_id FROM linked_banks WHERE item_hash = $1")
        .bind(&hash)
        .fetch_all(&mut *tx)
        .await?;
    if !owners.is_empty() && !owners.iter().any(|o| o == account_id) {
        return Ok(false);
    }
    sqlx::query(
        "INSERT INTO linked_banks (account_id, item_hash, token_hash) VALUES ($1, $2, $3)
         ON CONFLICT (account_id, item_hash) DO UPDATE SET token_hash = EXCLUDED.token_hash",
    )
    .bind(account_id)
    .bind(&hash)
    .bind(token_hash(access_token))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(true)
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
            assert_eq!(admit(&pool, "acct", &format!("item-{i}"), &format!("tok-{i}"), 3).await.unwrap(), Admit::Admitted);
        }
        assert_eq!(admit(&pool, "acct", "item-3", "tok-item-3", 3).await.unwrap(), Admit::AtLimit { max: 3 });
        // Another account has its own places.
        assert_eq!(admit(&pool, "other", "item-3", "tok-item-3", 3).await.unwrap(), Admit::Admitted);
    }

    #[sqlx::test]
    async fn a_bank_already_recorded_does_not_count_twice(pool: PgPool) {
        assert_eq!(admit(&pool, "acct", "item-a", "tok-item-a", 1).await.unwrap(), Admit::Admitted);
        assert_eq!(admit(&pool, "acct", "item-a", "tok-item-a", 1).await.unwrap(), Admit::Admitted);
    }

    #[sqlx::test]
    async fn forgetting_a_bank_frees_its_place_and_is_harmless_twice(pool: PgPool) {
        assert_eq!(admit(&pool, "acct", "item-a", "tok-item-a", 1).await.unwrap(), Admit::Admitted);
        assert_eq!(admit(&pool, "acct", "item-b", "tok-item-b", 1).await.unwrap(), Admit::AtLimit { max: 1 });
        forget(&pool, "acct", "item-a").await.unwrap();
        forget(&pool, "acct", "item-a").await.unwrap();
        assert_eq!(admit(&pool, "acct", "item-b", "tok-item-b", 1).await.unwrap(), Admit::Admitted);
    }

    #[sqlx::test]
    async fn a_token_is_served_only_to_the_account_that_linked_its_bank(pool: PgPool) {
        assert_eq!(admit(&pool, "acct", "item-a", "tok-a", 10).await.unwrap(), Admit::Admitted);
        assert_eq!(token_owner(&pool, "acct", "tok-a").await.unwrap(), TokenOwner::Caller);
        assert_eq!(token_owner(&pool, "other", "tok-a").await.unwrap(), TokenOwner::Another);
        assert_eq!(token_owner(&pool, "acct", "tok-never-seen").await.unwrap(), TokenOwner::Unknown);
        // Removing the bank forgets its token too.
        forget(&pool, "acct", "item-a").await.unwrap();
        assert_eq!(token_owner(&pool, "acct", "tok-a").await.unwrap(), TokenOwner::Unknown);
    }

    #[sqlx::test]
    async fn a_bank_recorded_before_tokens_is_claimed_by_its_own_account_only(pool: PgPool) {
        sqlx::query("INSERT INTO linked_banks (account_id, item_hash) VALUES ('acct', $1)")
            .bind(item_hash("item-old"))
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(token_owner(&pool, "acct", "tok-old").await.unwrap(), TokenOwner::Unknown);
        assert!(!claim(&pool, "other", "item-old", "tok-old").await.unwrap());
        assert_eq!(token_owner(&pool, "other", "tok-old").await.unwrap(), TokenOwner::Unknown);
        assert!(claim(&pool, "acct", "item-old", "tok-old").await.unwrap());
        assert_eq!(token_owner(&pool, "acct", "tok-old").await.unwrap(), TokenOwner::Caller);
        assert_eq!(token_owner(&pool, "other", "tok-old").await.unwrap(), TokenOwner::Another);
    }

    #[sqlx::test]
    async fn a_bank_no_account_recorded_goes_to_the_first_caller_and_takes_a_place(pool: PgPool) {
        assert!(claim(&pool, "acct", "item-legacy", "tok-legacy").await.unwrap());
        assert!(!claim(&pool, "other", "item-legacy", "tok-legacy").await.unwrap());
        assert_eq!(token_owner(&pool, "other", "tok-legacy").await.unwrap(), TokenOwner::Another);
        // It now counts toward the limit for banks linked afterwards.
        assert_eq!(admit(&pool, "acct", "item-new", "tok-new", 1).await.unwrap(), Admit::AtLimit { max: 1 });
    }
}

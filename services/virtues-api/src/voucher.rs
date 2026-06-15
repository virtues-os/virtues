//! Voucher data layer — v3 single-amount model (locked 2026-06-05).
//!
//! Vouchers are the disposable bridge between Atlas (billing) and this
//! gate. Atlas mints a code and `register()`s it here — carrying only
//! the amount and a `is_renewal` flag, never a customer or a bearer.
//! The device later `redeem()`s it onto its bearer.
//!
//! ## Redeem semantics
//!
//! - **Renewal voucher** (`is_renewal=true`, $15): SETs wallet to the
//!   amount. Overwrite — fresh monthly allocation. Sub renewal mints these.
//! - **Top-up voucher** (`is_renewal=false`, $10–$50): ADDs to existing
//!   wallet. Manual purchases and auto-top-ups mint these.
//!
//! Either way, `expires_at` is set to the next cohort-aligned 1st of
//! month UTC. Top-ups don't extend expiry beyond the current cohort —
//! credit you bought this month expires with the wallet next renewal.
//!
//! ## Privacy hardening
//!
//! `redeemed_at` is hour-bucketed (`date_trunc('hour', now())`) — defangs
//! timing-correlation attacks against Atlas's Stripe ledger. The row is
//! deleted entirely by the sweeper 24h after redemption. See
//! `sweeper.rs` for the deletion job.

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Duration, Utc};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::entitlement::next_utc_midnight;

/// What Atlas registers when it mints a voucher. No customer, no bearer.
pub struct RegisterVoucher {
    pub voucher_code_hash: Vec<u8>,
    /// Single amount in micros USD.
    pub amount_micros: i64,
    /// `true` = sub renewal (overwrite wallet). `false` = top-up (add).
    pub is_renewal: bool,
    pub voucher_expires_at: DateTime<Utc>,
    /// The customer's daily spend ceiling, carried from Atlas. Lands on the
    /// entitlement at redeem; `charge()` enforces it per-bearer.
    pub daily_cap_micros: i64,
}

/// Register a freshly minted voucher. Called by Atlas via
/// `POST /internal/voucher`.
pub async fn register(pool: &PgPool, v: RegisterVoucher) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO vouchers (voucher_code_hash, amount_micros, is_renewal, voucher_expires_at, daily_cap_micros)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(&v.voucher_code_hash)
    .bind(v.amount_micros)
    .bind(v.is_renewal)
    .bind(v.voucher_expires_at)
    .bind(v.daily_cap_micros)
    .execute(pool)
    .await
    .context("register voucher")?;
    Ok(())
}

pub struct RedeemResult {
    pub expires_at: DateTime<Utc>,
    pub wallet_micros: i64,
}

#[derive(Debug)]
pub enum RedeemError {
    NotFound,
    AlreadyRedeemed,
    Expired,
    Db(anyhow::Error),
}

impl std::fmt::Display for RedeemError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "voucher not found"),
            Self::AlreadyRedeemed => write!(f, "voucher already redeemed"),
            Self::Expired => write!(f, "voucher expired"),
            Self::Db(e) => write!(f, "db error: {e}"),
        }
    }
}

/// Redeem a voucher onto a bearer.
///
/// 1. Atomically claim the voucher (`redeemed_at IS NULL` guard).
///    Store `redeemed_at` hour-bucketed for timing-correlation resistance.
/// 2. Upsert the bearer's entitlement:
///    - `is_renewal=true` → SET `wallet_micros = amount` (overwrite),
///      reset `today_spent`, set `expires_at` to next cohort-aligned 1st.
///    - `is_renewal=false` → ADD `amount` to existing `wallet_micros`,
///      don't touch `today_spent`, keep existing `expires_at` (top-ups
///      don't extend the cohort).
/// 3. Commit. Voucher row carries `redeemed_at` but no bearer reference.
///    Sweeper deletes it 24h later.
pub async fn redeem(
    pool: &PgPool,
    voucher_code: &str,
    bearer_hash: &[u8],
) -> Result<RedeemResult, RedeemError> {
    let code_hash = sha256(voucher_code.as_bytes());
    let now = Utc::now();

    let mut tx = pool.begin().await.map_err(|e| RedeemError::Db(e.into()))?;

    // Atomically claim. Hour-bucket the timestamp.
    let claimed: Option<(i64, bool, i64)> = sqlx::query_as(
        r#"
        UPDATE vouchers
        SET redeemed_at = date_trunc('hour', now())
        WHERE voucher_code_hash = $1
          AND redeemed_at IS NULL
          AND voucher_expires_at > now()
        RETURNING amount_micros, is_renewal, daily_cap_micros
        "#,
    )
    .bind(&code_hash)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| RedeemError::Db(e.into()))?;

    let (amount_micros, is_renewal, daily_cap_micros) = match claimed {
        Some(v) => v,
        None => {
            let existing: Option<(Option<DateTime<Utc>>, DateTime<Utc>)> = sqlx::query_as(
                "SELECT redeemed_at, voucher_expires_at FROM vouchers WHERE voucher_code_hash = $1",
            )
            .bind(&code_hash)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| RedeemError::Db(e.into()))?;
            return match existing {
                None => Err(RedeemError::NotFound),
                Some((Some(_), _)) => Err(RedeemError::AlreadyRedeemed),
                Some((None, _)) => Err(RedeemError::Expired),
            };
        }
    };

    let result_row: (DateTime<Utc>, i64) = if is_renewal {
        // Renewal: overwrite wallet, reset today_spent, set fresh cohort
        // expiry. Insert or replace.
        let new_expiry = cohort_align_after(now + Duration::days(30));
        let reset = next_utc_midnight(now);
        sqlx::query_as(
            r#"
            INSERT INTO entitlements
                (bearer_hash, wallet_micros, today_spent_micros, today_reset_at, expires_at, daily_cap_micros)
            VALUES ($1, $2, 0, $3, $4, $5)
            ON CONFLICT (bearer_hash) DO UPDATE
            SET wallet_micros = $2,
                today_spent_micros = 0,
                today_reset_at = $3,
                expires_at = $4,
                daily_cap_micros = $5
            RETURNING expires_at, wallet_micros
            "#,
        )
        .bind(bearer_hash)
        .bind(amount_micros)
        .bind(reset)
        .bind(new_expiry)
        .bind(daily_cap_micros)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| RedeemError::Db(e.into()))?
    } else {
        // Top-up: ADD to existing wallet. If no entitlement yet (shouldn't
        // happen — sub renewal mints first), insert with the top-up amount
        // and a cohort-aligned expiry.
        let new_expiry = cohort_align_after(now + Duration::days(30));
        let reset = next_utc_midnight(now);
        sqlx::query_as(
            r#"
            INSERT INTO entitlements
                (bearer_hash, wallet_micros, today_spent_micros, today_reset_at, expires_at, daily_cap_micros)
            VALUES ($1, $2, 0, $3, $4, $5)
            ON CONFLICT (bearer_hash) DO UPDATE
            SET wallet_micros = entitlements.wallet_micros + EXCLUDED.wallet_micros,
                daily_cap_micros = EXCLUDED.daily_cap_micros
            RETURNING expires_at, wallet_micros
            "#,
        )
        .bind(bearer_hash)
        .bind(amount_micros)
        .bind(reset)
        .bind(new_expiry)
        .bind(daily_cap_micros)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| RedeemError::Db(e.into()))?
    };

    tx.commit().await.map_err(|e| RedeemError::Db(e.into()))?;

    Ok(RedeemResult {
        expires_at: result_row.0,
        wallet_micros: result_row.1,
    })
}

/// Cohort alignment: first day of the month strictly after `dt`'s month,
/// 00:00 UTC. Every bearer's expiry lands on a shared monthly boundary
/// — defense-in-depth against timing fingerprinting.
fn cohort_align_after(dt: DateTime<Utc>) -> DateTime<Utc> {
    let d = dt.date_naive();
    let (y, m) = (d.year(), d.month());
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    chrono::NaiveDate::from_ymd_opt(ny, nm, 1)
        .expect("valid first-of-month")
        .and_hms_opt(0, 0, 0)
        .expect("midnight")
        .and_utc()
}

fn sha256(data: &[u8]) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().to_vec()
}

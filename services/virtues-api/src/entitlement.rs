//! Entitlement data layer — v3 single-wallet model (locked 2026-06-05).
//!
//! One row per bearer:
//!   `bearer_hash → (wallet_micros, today_spent_micros, expires_at)`
//!
//! No customer link, no tier, no pool split. Single $20/mo plan + top-ups.
//! Access gated by `expires_at`. Vouchers (see `voucher.rs`) refill
//! `wallet_micros` on redeem.
//!
//! ## Charge model
//!
//! Every `charge()`:
//!   1. Apply 20% universal markup (env: `USAGE_MARKUP_BASIS_POINTS=2000`)
//!      to the real cost from the upstream provider. The markup is how
//!      Virtues makes money on usage — same multiplier on AI, Places,
//!      Exa, Plaid, anything with a real provider cost.
//!   2. Enforce per-call cap ($5/call billed).
//!   3. Lazy daily reset of `today_spent_micros` at UTC midnight rollover.
//!   4. Enforce daily ceiling (default $20/day, user-tunable via atlas).
//!   5. Atomic decrement guarded by `expires_at > now()` AND
//!      `wallet_micros >= billed AND today_spent + billed <= daily_cap`.
//!
//! On `InsufficientBudget`, the box catches the 402 and triggers
//! auto-top-up via atlas `/credits/auto-topup` (saved-card off-session
//! charge → voucher mint → redeem → retry).
//!
//! ## `X-Virtues-Purpose` header (v3 disposition)
//!
//! In v2 this header routed charges between `os_reserve_micros` and
//! `wallet_chat_micros`. In v3 it has NO routing effect — the wallet is
//! single. Box callers may still send it; we accept it for forward
//! compatibility but ignore the value. Drop in v1.1.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::PgPool;

/// Per-call cost cap. Rejects pathologically large prompts (prompt
/// injection asking for 1M tokens, etc). Fixed defense, not user-tunable.
/// Applied AFTER markup — `billed > PER_CALL_CAP_MICROS` is what 400s.
pub const PER_CALL_CAP_MICROS: i64 = 5_000_000; // $5/call billed

/// Default daily spend ceiling (overridden per-customer via atlas
/// `customers.daily_cap_micros` once we wire that — TODO). Until the
/// per-customer override lands, every bearer uses this floor.
pub const DEFAULT_DAILY_CEILING_MICROS: i64 = 20_000_000; // $20/day

/// Universal markup applied to the real upstream cost before wallet
/// decrement. 2000 basis points = 20%.
pub const DEFAULT_MARKUP_BASIS_POINTS: i64 = 2_000;

/// Returns the configured markup basis points (env: `USAGE_MARKUP_BASIS_POINTS`).
pub fn markup_basis_points() -> i64 {
    std::env::var("USAGE_MARKUP_BASIS_POINTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_MARKUP_BASIS_POINTS)
}

/// Compute the billed amount from a real upstream cost. Single source of
/// truth for the markup formula; called inside `charge()` and exposed for
/// usage logging (we record both real and billed for margin analytics).
#[inline]
pub fn apply_markup(real_micros: i64) -> i64 {
    let bp = markup_basis_points();
    real_micros.saturating_mul(10_000 + bp) / 10_000
}

/// Full entitlement row (single-wallet shape).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Entitlement {
    pub bearer_hash: Vec<u8>,
    pub wallet_micros: i64,
    pub today_spent_micros: i64,
    pub today_reset_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// Look up an entitlement by its bearer hash. Called by bearer-auth on
/// every gated request.
pub async fn get_by_bearer_hash(
    pool: &PgPool,
    bearer_hash: &[u8],
) -> Result<Option<Entitlement>> {
    let row = sqlx::query_as::<_, Entitlement>(
        r#"
        SELECT bearer_hash, wallet_micros, today_spent_micros,
               today_reset_at, expires_at
        FROM entitlements
        WHERE bearer_hash = $1
        "#,
    )
    .bind(bearer_hash)
    .fetch_optional(pool)
    .await
    .context("select entitlement by bearer_hash")?;

    Ok(row)
}

/// Charge a bearer for an upstream call.
///
/// `real_cost_micros` is what the upstream provider charged us (Vercel AI
/// Gateway's `usage.cost`, Places' fixed $0.003, etc). The 20% markup is
/// applied here, then validated against per-call cap, daily ceiling, and
/// wallet balance, then atomically debited.
///
/// Returns `ChargeOk { wallet_micros, billed_micros }` on success.
pub async fn charge(
    pool: &PgPool,
    bearer_hash: &[u8],
    real_cost_micros: i64,
) -> Result<ChargeOk, ChargeError> {
    if real_cost_micros <= 0 {
        return Err(ChargeError::InvalidCost);
    }

    let billed = apply_markup(real_cost_micros);
    if billed > PER_CALL_CAP_MICROS {
        return Err(ChargeError::CallTooExpensive);
    }

    let now = Utc::now();
    let next_midnight = next_utc_midnight(now);
    let daily_ceiling = DEFAULT_DAILY_CEILING_MICROS;

    // Lazy daily reset: zero `today_spent` at UTC midnight rollover.
    sqlx::query(
        r#"
        UPDATE entitlements
        SET today_spent_micros = 0,
            today_reset_at = $3
        WHERE bearer_hash = $1
          AND today_reset_at <= $2
        "#,
    )
    .bind(bearer_hash)
    .bind(now)
    .bind(next_midnight)
    .execute(pool)
    .await
    .map_err(|e| ChargeError::Db(e.into()))?;

    // Atomic decrement guarded by expiry + wallet + daily-cap invariants.
    let row: Option<(i64,)> = sqlx::query_as(
        r#"
        UPDATE entitlements
        SET wallet_micros = wallet_micros - $1,
            today_spent_micros = today_spent_micros + $1
        WHERE bearer_hash = $2
          AND expires_at > now()
          AND wallet_micros >= $1
          AND today_spent_micros + $1 <= $3
        RETURNING wallet_micros
        "#,
    )
    .bind(billed)
    .bind(bearer_hash)
    .bind(daily_ceiling)
    .fetch_optional(pool)
    .await
    .map_err(|e| ChargeError::Db(e.into()))?;

    if let Some((wallet,)) = row {
        return Ok(ChargeOk {
            wallet_micros: wallet,
            billed_micros: billed,
            real_micros: real_cost_micros,
        });
    }

    classify_failure(pool, bearer_hash, billed, daily_ceiling).await
}

/// On a failed debit, disambiguate why: not found / expired / insufficient
/// wallet / daily cap reached. The box uses the error code to decide
/// whether to trigger auto-top-up (`InsufficientBudget`) or surface to
/// the user (`Expired`, `DailyCapReached`).
async fn classify_failure(
    pool: &PgPool,
    bearer_hash: &[u8],
    billed: i64,
    daily_ceiling: i64,
) -> Result<ChargeOk, ChargeError> {
    let row: Option<(DateTime<Utc>, i64, i64)> = sqlx::query_as(
        "SELECT expires_at, wallet_micros, today_spent_micros FROM entitlements WHERE bearer_hash = $1",
    )
    .bind(bearer_hash)
    .fetch_optional(pool)
    .await
    .map_err(|e| ChargeError::Db(e.into()))?;

    match row {
        None => Err(ChargeError::NotFound),
        Some((expires_at, _, _)) if expires_at <= Utc::now() => Err(ChargeError::Expired),
        Some((_, _, today_spent)) if today_spent + billed > daily_ceiling => {
            Err(ChargeError::DailyCapReached)
        }
        Some(_) => Err(ChargeError::InsufficientBudget),
    }
}

/// Refund a previously-charged amount back to the wallet. Best-effort —
/// no failure propagation on DB error. Refunds the **billed** amount
/// (post-markup), since that's what was debited; we keep the markup on
/// failed upstreams (cost of doing business).
pub async fn refund(pool: &PgPool, bearer_hash: &[u8], billed_micros: i64) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE entitlements
        SET wallet_micros = wallet_micros + $1,
            today_spent_micros = GREATEST(today_spent_micros - $1, 0)
        WHERE bearer_hash = $2
        "#,
    )
    .bind(billed_micros)
    .bind(bearer_hash)
    .execute(pool)
    .await
    .context("refund wallet")?;
    Ok(())
}

/// Outcome of a successful charge.
#[derive(Debug, Clone, Copy)]
pub struct ChargeOk {
    pub wallet_micros: i64,
    /// What the user was charged (post-markup).
    pub billed_micros: i64,
    /// What the upstream really cost us (for margin analytics).
    pub real_micros: i64,
}

#[derive(Debug)]
pub enum ChargeError {
    InsufficientBudget,
    DailyCapReached,
    Expired,
    NotFound,
    InvalidCost,
    CallTooExpensive,
    Db(anyhow::Error),
}

impl std::fmt::Display for ChargeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InsufficientBudget => write!(f, "wallet exhausted — trigger auto-top-up"),
            Self::DailyCapReached => write!(f, "daily spend ceiling reached"),
            Self::Expired => write!(f, "bearer expired — redeem fresh voucher"),
            Self::NotFound => write!(f, "entitlement not found"),
            Self::InvalidCost => write!(f, "real_cost_micros must be > 0"),
            Self::CallTooExpensive => write!(f, "single call exceeds per-call cap"),
            Self::Db(e) => write!(f, "db error: {e}"),
        }
    }
}

/// Next UTC midnight strictly after `now`.
pub fn next_utc_midnight(now: DateTime<Utc>) -> DateTime<Utc> {
    let date = now.date_naive().succ_opt().expect("date overflow");
    date.and_hms_opt(0, 0, 0)
        .expect("midnight construction")
        .and_utc()
}

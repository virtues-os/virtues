//! Wallet + ledger data layer — linked prepaid model (v1).
//!
//! One row per paid-for account in `wallets`:
//!   `account_id → (balance_micros, expires_at)`
//!
//! `account_id` is the opaque id atlas assigns per account. The box
//! authenticates with a rotatable api_key (see `device_keys`), which belongs
//! to the account, not the wallet: a key outlives a lapsed wallet, and an
//! account that never paid has a key and no wallet. Rotating or losing a key
//! never moves money.
//!
//! ## Source of truth: the ledger
//!
//! `wallets.balance_micros` is a fast **projection**. The append-only
//! `ledger` table is the truth: every grant (renewal), topup, charge, and
//! refund is one immutable row, and `balance == SUM(ledger.micros)`. The two
//! are always written in the same transaction; the projection is rebuildable.
//!
//! ## Charge model
//!
//! Only AI spends from the wallet: model calls and web search. Bank data,
//! places and maps are included in the subscription and need only a live
//! wallet (`BearerAuth`), never a debit.
//!
//! Every `settle()`:
//!   1. Usage is billed at cost by default (env `USAGE_MARKUP_BASIS_POINTS`,
//!      default 0). The margin is in the subscription price, not in usage:
//!      the plan's price is more than its monthly allotment, and top-ups
//!      are credited net of the card fee (atlas `credits.rs`).
//!   2. An unconditional debit plus a `ledger` 'charge' row, in one
//!      transaction. The route's pre-flight gate refuses the next call once
//!      the balance is spent.
//!
//! There is **no per-day spend wall** — the Cursor-style model uses a
//! user-settable MONTHLY top-up ceiling (`customers.monthly_cap_micros`,
//! enforced atlas-side on top-up) as the only spend ceiling.
//!
//! When the balance is spent the box surfaces "wallet empty" (optionally one
//! auto-topup via atlas, which credits the wallet here).

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Duration, Utc};
use sqlx::PgPool;

/// Markup applied to the real upstream cost before debit, in basis points
/// (100 = 1%). 0: usage is billed at cost.
pub const DEFAULT_MARKUP_BASIS_POINTS: i64 = 0;

/// Returns the configured markup basis points (env: `USAGE_MARKUP_BASIS_POINTS`).
pub fn markup_basis_points() -> i64 {
    std::env::var("USAGE_MARKUP_BASIS_POINTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_MARKUP_BASIS_POINTS)
}

/// Compute the billed amount from a real upstream cost. Single source of truth
/// for the markup formula.
#[inline]
pub fn apply_markup(real_micros: i64) -> i64 {
    let bp = markup_basis_points();
    real_micros.saturating_mul(10_000 + bp) / 10_000
}

/// Convert a USD float (as upstreams report it, e.g. the AI gateway's
/// `usage.cost`) into integer micros. Shared so every paid proxy resolves real
/// cost the same way before `charge`/`settle`.
#[inline]
pub fn usd_to_micros(usd: f64) -> i64 {
    (usd * 1_000_000.0).round() as i64
}

/// A live wallet, carried by bearer-auth on every paid request.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Wallet {
    pub account_id: String,
    /// Everything spendable: the monthly allotment plus top-up credit.
    pub balance_micros: i64,
    /// The part of `balance_micros` that is top-up credit (migration 0014).
    pub topup_micros: i64,
    pub expires_at: DateTime<Utc>,
}

/// The account a box key belongs to (the auth hot path). Reads only
/// `device_keys`, so it answers for an account with no wallet.
pub async fn key_account(pool: &PgPool, api_key_hash: &[u8]) -> Result<Option<String>> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT account_id FROM device_keys WHERE api_key_hash = $1")
            .bind(api_key_hash)
            .fetch_optional(pool)
            .await
            .context("resolve account by api_key hash")?;
    Ok(row.map(|(id,)| id))
}

/// An account's wallet, live or expired. None when the account has never paid
/// or its wallet was swept.
pub async fn get_wallet(pool: &PgPool, account_id: &str) -> Result<Option<Wallet>> {
    let row = sqlx::query_as::<_, Wallet>(
        r#"
        SELECT account_id, balance_micros,
               LEAST(topup_micros, GREATEST(balance_micros, 0)) AS topup_micros,
               expires_at
        FROM wallets
        WHERE account_id = $1
        "#,
    )
    .bind(account_id)
    .fetch_optional(pool)
    .await
    .context("select wallet by account id")?;
    Ok(row)
}

/// Post-paid settlement for a call whose cost is only known *after* it
/// succeeds (AI: the gateway reports `usage.cost` in the response). It debits
/// **unconditionally** — the response already went out,
/// so we must record the true spend even if it dips the balance negative by at
/// most one call. The pre-flight budget gate (see `routes/ai.rs`) then refuses
/// the *next* call once the balance is in the red. This is what makes the
/// wallet actually enforce on the chat path; a debit guarded by the balance
/// would plateau it at a few cents and leak unlimited free calls. Returns
/// the new balance.
pub async fn settle(pool: &PgPool, account_id: &str, real_cost_micros: i64) -> Result<i64> {
    if real_cost_micros <= 0 {
        return Ok(0);
    }
    let billed = apply_markup(real_cost_micros);

    let mut tx = pool.begin().await?;

    // Unconditional debit — only requires the wallet to still exist.
    let row: Option<(i64,)> = sqlx::query_as(
        "UPDATE wallets \
         SET balance_micros = balance_micros - $1, \
             topup_micros = LEAST(topup_micros, GREATEST(balance_micros - $1, 0)) \
         WHERE account_id = $2 \
         RETURNING balance_micros",
    )
    .bind(billed)
    .bind(account_id)
    .fetch_optional(&mut *tx)
    .await
    .context("settle debit")?;
    let Some((balance,)) = row else {
        // Wallet swept between the call and settlement — nothing to debit.
        tx.rollback().await?;
        return Ok(0);
    };
    sqlx::query(
        "INSERT INTO ledger (account_id, micros, kind, real_micros) \
         VALUES ($1, $2, 'charge', $3)",
    )
    .bind(account_id)
    .bind(-billed)
    .bind(real_cost_micros)
    .execute(&mut *tx)
    .await
    .context("settle ledger row")?;
    tx.commit().await?;
    Ok(balance)
}

/// Revoke one box key of an account: the owner removed that server. Returns
/// how many keys were removed (0 when virtues-api never had it). Scoped to
/// the account, so a hash can never reach another account's key.
pub async fn revoke_device(pool: &PgPool, account_id: &str, api_key_hash: &[u8]) -> Result<u64> {
    let r = sqlx::query("DELETE FROM device_keys WHERE account_id = $1 AND api_key_hash = $2")
        .bind(account_id)
        .bind(api_key_hash)
        .execute(pool)
        .await
        .context("revoke device key")?;
    Ok(r.rows_affected())
}

/// Register (or rotate) a device api key for an account. It creates no wallet:
/// a key needs only the account, and a wallet comes from a credit. The balance
/// is never touched, so re-linking re-points access without moving money.
///
/// **Scoped to a BOX when `box_id` is given.** Rotation replaces that box's own
/// key and leaves every sibling alone, so one account can hold several boxes —
/// a home and an office, a replacement mid-migration, a box bought for family.
/// Passing `None` keeps the historical behaviour (replace every key on the
/// account), which is what pre-2026-08-12 keys and any caller that does not
/// know its box get.
///
/// Until now this always replaced every key on the account, which made a second
/// box silently kill the first. Nobody hit it because linking a second box was
/// laborious; it becomes one tap as soon as the app can vouch for a link, so
/// the destruction had to go first.
pub async fn register_device(
    pool: &PgPool,
    api_key_hash: &[u8],
    account_id: &str,
    box_id: Option<&str>,
) -> Result<()> {
    let mut tx = pool.begin().await?;

    // Drop only what this registration replaces: this BOX's previous key when we
    // know which box it is, otherwise the whole account (the legacy path).
    match box_id {
        Some(id) => {
            // `OR box_id IS NULL` is load-bearing: `=` never matches NULL, so
            // a key registered before the account went per-box (box_id NULL)
            // would otherwise survive its box's re-link forever — an immortal
            // credential that still proxies AI calls and spends the wallet
            // (review finding, 2026-08-24). The NULL row is the account's old
            // shared key; the first labeled registration retires it.
            sqlx::query(
                "DELETE FROM device_keys WHERE account_id = $1 \
                 AND (box_id = $2 OR box_id IS NULL)",
            )
            .bind(account_id)
            .bind(id)
            .execute(&mut *tx)
            .await
            .context("clear this box's prior key")?;
        }
        None => {
            sqlx::query("DELETE FROM device_keys WHERE account_id = $1")
                .bind(account_id)
                .execute(&mut *tx)
                .await
                .context("clear prior device keys")?;
        }
    }
    sqlx::query(
        "INSERT INTO device_keys (api_key_hash, account_id, box_id) VALUES ($1, $2, $3)",
    )
    .bind(api_key_hash)
    .bind(account_id)
    .bind(box_id)
    .execute(&mut *tx)
    .await
    .context("insert device key")?;

    tx.commit().await?;
    Ok(())
}

/// How a credit lands on the balance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreditMode {
    /// Renewal: overwrite the balance to `amount` (fresh monthly allotment)
    /// and bump expiry to the next cohort boundary.
    Set,
    /// Top-up: add `amount` to the existing balance; expiry unchanged.
    Add,
}

/// Credit an account (subscription renewal or top-up), writing the matching
/// `ledger` row so `balance == SUM(ledger)` holds. Returns the new balance.
///
/// `Set` (renewal) resets the monthly allotment to `amount` and keeps the
/// top-up credit, so the balance becomes `amount + top-up credit`. It records
/// the **delta**, so a renewal that claws back unused allotment stays
/// journal-consistent. `Add` (top-up) adds to both the balance and the
/// top-up credit.
pub async fn credit(
    pool: &PgPool,
    account_id: &str,
    amount_micros: i64,
    mode: CreditMode,
    reference: Option<&str>,
) -> Result<i64> {
    let now = Utc::now();
    let mut tx = pool.begin().await?;
    // Serialize credits per account. `FOR UPDATE` below locks nothing when
    // the wallet row does not exist yet, so two first credits (a checkout's
    // and invoice.paid's, or a top-up racing either) would both read zero and
    // one would overwrite the other. The lock ends with the transaction.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('wallet-credit:' || $1))")
        .bind(account_id)
        .execute(&mut *tx)
        .await
        .context("lock wallet credit")?;

    let existing: Option<(i64, i64)> = sqlx::query_as(
        "SELECT balance_micros, topup_micros FROM wallets WHERE account_id = $1 FOR UPDATE",
    )
    .bind(account_id)
    .fetch_optional(&mut *tx)
    .await
    .context("lock account for credit")?;
    // absent-ok: no wallet row yet means nothing to keep; the query error was `?`-ed above.
    let (old_balance, old_topup) = existing.unwrap_or((0, 0));
    // Top-up credit never exceeds what is left; an older image that
    // ignored the column can leave it stale.
    let topup = old_topup.min(old_balance.max(0)).max(0);

    let (new_balance, new_topup, kind) = match mode {
        CreditMode::Set => (amount_micros + topup, topup, "grant"),
        CreditMode::Add => (old_balance + amount_micros, topup + amount_micros, "topup"),
    };
    let delta = new_balance - old_balance;

    match mode {
        CreditMode::Set => {
            sqlx::query(
                r#"
                INSERT INTO wallets (account_id, balance_micros, topup_micros, expires_at)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (account_id) DO UPDATE
                SET balance_micros = EXCLUDED.balance_micros,
                    topup_micros = EXCLUDED.topup_micros,
                    expires_at = EXCLUDED.expires_at
                "#,
            )
            .bind(account_id)
            .bind(new_balance)
            .bind(new_topup)
            .bind(cohort_align_after(now + Duration::days(30)))
            .execute(&mut *tx)
            .await
            .context("credit set")?;
        }
        CreditMode::Add => {
            sqlx::query(
                r#"
                INSERT INTO wallets (account_id, balance_micros, topup_micros, expires_at)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (account_id) DO UPDATE
                SET balance_micros = EXCLUDED.balance_micros,
                    topup_micros = EXCLUDED.topup_micros
                "#,
            )
            .bind(account_id)
            .bind(new_balance)
            .bind(new_topup)
            .bind(cohort_align_after(now + Duration::days(30)))
            .execute(&mut *tx)
            .await
            .context("credit add")?;
        }
    }

    sqlx::query("INSERT INTO ledger (account_id, micros, kind, ref) VALUES ($1, $2, $3, $4)")
        .bind(account_id)
        .bind(delta)
        .bind(kind)
        .bind(reference)
        .execute(&mut *tx)
        .await
        .context("credit ledger row")?;

    tx.commit().await?;
    Ok(new_balance)
}

/// Balance + recent ledger entries, for the user-facing usage surface.
pub async fn usage_summary(
    pool: &PgPool,
    account_id: &str,
    limit: i64,
) -> Result<UsageSummary> {
    let acct = get_wallet(pool, account_id).await?;
    let Some(acct) = acct else {
        return Ok(UsageSummary::default());
    };

    // Month-to-date spend (sum of charges since the 1st, UTC).
    let month_start = month_start_utc(Utc::now());
    // `::bigint`: SUM over int8 is NUMERIC in Postgres and sqlx will not
    // decode it as i64, so this read failed on every account with a charge
    // this month and /v1/usage answered `internal` — the test module below
    // already knew this about SUM and cast; the handler did not.
    let (mtd,): (Option<i64>,) = sqlx::query_as(
        "SELECT (-SUM(micros))::bigint FROM ledger \
         WHERE account_id = $1 AND kind = 'charge' AND ts >= $2",
    )
    .bind(account_id)
    .bind(month_start)
    .fetch_one(pool)
    .await
    .context("month-to-date spend")?;

    // Everything that moved the balance since the last renewal, apart from
    // top-ups. Ordered by id, not ts: credits and charges both lock the wallet
    // row before they write here, so ids follow the order they took effect,
    // while ts is when each transaction began.
    let period: Option<(DateTime<Utc>, i64)> = sqlx::query_as(
        "SELECT g.ts, (SELECT COALESCE(-SUM(l.micros), 0)::bigint FROM ledger l \
                       WHERE l.account_id = $1 AND l.id > g.id AND l.kind <> 'topup') \
         FROM (SELECT id, ts FROM ledger \
               WHERE account_id = $1 AND kind = 'grant' ORDER BY id DESC LIMIT 1) g",
    )
    .bind(account_id)
    .fetch_optional(pool)
    .await
    .context("usage since renewal")?;
    // A wallet that has only been topped up has no renewal row.
    let period_start = period.map(|(ts, _)| ts);
    let period_used_micros = period.map(|(_, used)| used);

    let entries = sqlx::query_as::<_, LedgerEntry>(
        "SELECT ts, micros, kind, real_micros FROM ledger \
         WHERE account_id = $1 ORDER BY ts DESC LIMIT $2",
    )
    .bind(account_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("recent ledger entries")?;

    Ok(UsageSummary {
        balance_micros: acct.balance_micros,
        topup_micros: acct.topup_micros,
        month_to_date_micros: mtd.unwrap_or(0),
        period_start,
        period_used_micros,
        expires_at: Some(acct.expires_at),
        entries,
    })
}

#[derive(Debug, Default, serde::Serialize)]
pub struct UsageSummary {
    pub balance_micros: i64,
    /// The part of `balance_micros` that is top-up credit, which a renewal
    /// keeps. The rest is this month's allotment.
    pub topup_micros: i64,
    pub month_to_date_micros: i64,
    /// When the monthly allotment was last set. None without a renewal.
    pub period_start: Option<DateTime<Utc>>,
    /// What has come off the balance since `period_start`. While allotment is
    /// left, `allotment = balance - topup + period_used`.
    pub period_used_micros: Option<i64>,
    /// When the wallet lapses without a renewal: the first of a month, UTC.
    /// It is not the billing date, which atlas holds (`renews_at`).
    pub expires_at: Option<DateTime<Utc>>,
    pub entries: Vec<LedgerEntry>,
}

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct LedgerEntry {
    pub ts: DateTime<Utc>,
    pub micros: i64,
    pub kind: String,
    pub real_micros: Option<i64>,
}

/// First-of-month 00:00 UTC at or after `dt` — the cohort-aligned wallet
/// expiry boundary. (Moved from the deleted voucher module.)
pub fn cohort_align_after(dt: DateTime<Utc>) -> DateTime<Utc> {
    let d = dt.date_naive();
    let (y, m) = (d.year(), d.month());
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    chrono::NaiveDate::from_ymd_opt(ny, nm, 1)
        .expect("valid first-of-month")
        .and_hms_opt(0, 0, 0)
        .expect("midnight")
        .and_utc()
}

/// Start of the current calendar month, 00:00 UTC.
fn month_start_utc(now: DateTime<Utc>) -> DateTime<Utc> {
    let d = now.date_naive();
    chrono::NaiveDate::from_ymd_opt(d.year(), d.month(), 1)
        .expect("valid first-of-month")
        .and_hms_opt(0, 0, 0)
        .expect("midnight")
        .and_utc()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `settle()` debits the balance and writes a ledger row each time,
    /// keeping `balance == SUM(ledger)`. (No daily cap — the only spend
    /// ceiling is the monthly top-up cap atlas-side.)
    #[sqlx::test]
    async fn settle_debits_and_journals(pool: PgPool) {
        // Fund $10 via a renewal credit.
        credit(&pool, "acct-low", 10_000_000, CreditMode::Set, None)
            .await
            .unwrap();

        // Billed at cost (no markup by default): 4M + 4M leaves 2M.
        assert_eq!(settle(&pool, "acct-low", 4_000_000).await.expect("1st ok"), 6_000_000);
        assert_eq!(settle(&pool, "acct-low", 4_000_000).await.expect("2nd ok"), 2_000_000);

        // balance projection == SUM(ledger).
        let acct = get_wallet(&pool, "acct-low").await.unwrap().unwrap();
        // SUM(int8) is NUMERIC in Postgres; cast so it decodes as i64.
        let (sum,): (Option<i64>,) =
            sqlx::query_as("SELECT SUM(micros)::bigint FROM ledger WHERE account_id = $1")
                .bind("acct-low")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(acct.balance_micros, sum.unwrap(), "balance must equal ledger sum");
    }

    async fn ledger_sum(pool: &PgPool, account: &str) -> i64 {
        let (sum,): (Option<i64>,) =
            sqlx::query_as("SELECT SUM(micros)::bigint FROM ledger WHERE account_id = $1")
                .bind(account)
                .fetch_one(pool)
                .await
                .unwrap();
        // absent-ok: SUM over no ledger rows is NULL, which is a zero balance.
        sum.unwrap_or(0)
    }

    /// The allotment is spent before top-up credit, and a renewal resets the
    /// allotment but keeps whatever top-up credit is left. The balance still
    /// equals the ledger after every step.
    #[sqlx::test]
    async fn renewal_keeps_topup_credit_and_allotment_is_spent_first(pool: PgPool) {
        let a = "acct-two";
        credit(&pool, a, 20_000_000, CreditMode::Set, None).await.unwrap(); // $20 allotment
        credit(&pool, a, 50_000_000, CreditMode::Add, Some("topup:pi_1")).await.unwrap(); // $50 top-up
        let w = get_wallet(&pool, a).await.unwrap().unwrap();
        assert_eq!((w.balance_micros, w.topup_micros), (70_000_000, 50_000_000));

        // $15 comes out of the allotment; top-up credit is untouched. (One
        // pre-paid call is capped at $5, so the bulk is settled post-paid.)
        settle(&pool, a, 5_000_000).await.unwrap();
        settle(&pool, a, 10_000_000).await.unwrap();
        let w = get_wallet(&pool, a).await.unwrap().unwrap();
        assert_eq!((w.balance_micros, w.topup_micros), (55_000_000, 50_000_000));

        // $10 more: $5 of allotment is left, so $5 comes out of top-up credit.
        settle(&pool, a, 10_000_000).await.unwrap();
        let w = get_wallet(&pool, a).await.unwrap().unwrap();
        assert_eq!((w.balance_micros, w.topup_micros), (45_000_000, 45_000_000));

        // Renewal: a fresh $20 allotment on top of the $45 of top-up credit.
        credit(&pool, a, 20_000_000, CreditMode::Set, Some("renewal")).await.unwrap();
        let w = get_wallet(&pool, a).await.unwrap().unwrap();
        assert_eq!((w.balance_micros, w.topup_micros), (65_000_000, 45_000_000));
        assert_eq!(w.balance_micros, ledger_sum(&pool, a).await);
    }

    /// Concurrent first credits on a new account all land: none
    /// overwrites another, and the balance matches the ledger.
    #[sqlx::test]
    async fn concurrent_first_credits_all_land(pool: PgPool) {
        let a = "acct-race";
        let tasks: Vec<_> = (0..6)
            .map(|_| {
                let pool = pool.clone();
                tokio::spawn(async move {
                    credit(&pool, a, 1_000_000, CreditMode::Add, None).await.unwrap();
                })
            })
            .collect();
        for t in tasks {
            t.await.unwrap();
        }
        let w = get_wallet(&pool, a).await.unwrap().unwrap();
        assert_eq!((w.balance_micros, w.topup_micros), (6_000_000, 6_000_000));
        assert_eq!(w.balance_micros, ledger_sum(&pool, a).await);
    }

    /// Revoking removes exactly that key on that account, and never a key
    /// on another account even with the same hash bytes.
    #[sqlx::test]
    async fn revoke_removes_only_that_key(pool: PgPool) {
        register_device(&pool, b"hash-kitchen-000000000000000000", "acct-a", Some("ep-kitchen")).await.unwrap();
        register_device(&pool, b"hash-office-0000000000000000000", "acct-a", Some("ep-office")).await.unwrap();
        assert_eq!(revoke_device(&pool, "acct-b", b"hash-office-0000000000000000000").await.unwrap(), 0);
        assert_eq!(revoke_device(&pool, "acct-a", b"hash-office-0000000000000000000").await.unwrap(), 1);
        assert!(key_account(&pool, b"hash-office-0000000000000000000").await.unwrap().is_none());
        assert_eq!(key_account(&pool, b"hash-kitchen-000000000000000000").await.unwrap().as_deref(), Some("acct-a"));
    }

    /// Unused allotment does not carry over: a renewal resets it.
    #[sqlx::test]
    async fn unused_allotment_resets_at_renewal(pool: PgPool) {
        let a = "acct-idle";
        credit(&pool, a, 20_000_000, CreditMode::Set, None).await.unwrap();
        settle(&pool, a, 2_000_000).await.unwrap();
        credit(&pool, a, 20_000_000, CreditMode::Set, None).await.unwrap();
        let w = get_wallet(&pool, a).await.unwrap().unwrap();
        assert_eq!((w.balance_micros, w.topup_micros), (20_000_000, 0));
        assert_eq!(w.balance_micros, ledger_sum(&pool, a).await);
    }

    /// `/v1/usage` decodes for an account that has spent this month. The
    /// month-to-date figure is a SUM over int8, which is NUMERIC — without
    /// the cast this read failed on exactly the accounts with something to
    /// show, and the Billing page's wallet panel said "Couldn't load".
    #[sqlx::test]
    async fn usage_summary_decodes_month_to_date(pool: PgPool) {
        let fresh = usage_summary(&pool, "acct-u", 10).await.unwrap();
        assert_eq!(fresh.month_to_date_micros, 0, "no account, no spend");

        credit(&pool, "acct-u", 10_000_000, CreditMode::Set, None)
            .await
            .unwrap();
        let empty = usage_summary(&pool, "acct-u", 10).await.unwrap();
        assert_eq!(empty.month_to_date_micros, 0, "funded but unspent");

        settle(&pool, "acct-u", 1_000_000).await.expect("settle");
        settle(&pool, "acct-u", 2_000_000).await.expect("settle");
        let spent = usage_summary(&pool, "acct-u", 10).await.unwrap();
        let acct = get_wallet(&pool, "acct-u").await.unwrap().unwrap();
        assert!(spent.month_to_date_micros > 0);
        assert_eq!(
            spent.month_to_date_micros,
            10_000_000 - acct.balance_micros,
            "month-to-date is what the charges took off the credit"
        );
    }

    /// The Billing page's breakdown: usage since the renewal, plus what is
    /// left, is the allotment the renewal set. Usage before the renewal and
    /// top-ups after it stay out of it.
    #[sqlx::test]
    async fn usage_since_renewal_explains_the_balance(pool: PgPool) {
        let a = "acct-period";
        credit(&pool, a, 5_000_000, CreditMode::Add, None).await.unwrap();
        let no_renewal = usage_summary(&pool, a, 10).await.unwrap();
        assert_eq!((no_renewal.period_start, no_renewal.period_used_micros), (None, None));

        credit(&pool, a, 20_000_000, CreditMode::Set, None).await.unwrap();
        let unused = usage_summary(&pool, a, 10).await.unwrap();
        assert!(unused.period_start.is_some());
        assert_eq!(unused.period_used_micros, Some(0));

        settle(&pool, a, 3_000_000).await.expect("settle");
        credit(&pool, a, 20_000_000, CreditMode::Set, None).await.unwrap();
        settle(&pool, a, 2_000_000).await.expect("settle");
        credit(&pool, a, 10_000_000, CreditMode::Add, None).await.unwrap();

        let u = usage_summary(&pool, a, 10).await.unwrap();
        assert_eq!(u.period_used_micros, Some(2_000_000), "only since the last renewal");
        assert_eq!(u.month_to_date_micros, 5_000_000, "the calendar month counts both");
        assert_eq!(u.topup_micros, 15_000_000);
        assert_eq!(u.balance_micros - u.topup_micros + u.period_used_micros.unwrap(), 20_000_000);
    }

    /// Rotating the device key (recovery) preserves the balance: the same
    /// funds are chargeable under the new key.
    #[sqlx::test]
    async fn key_rotation_preserves_balance(pool: PgPool) {
        credit(&pool, "acct-r", 50_000_000, CreditMode::Set, None)
            .await
            .unwrap();
        register_device(&pool, b"keyhash-old-0000000000000000", "acct-r", None)
            .await
            .unwrap();
        settle(&pool, "acct-r", 1_000_000).await.expect("settle under old key path");

        let before = get_wallet(&pool, "acct-r").await.unwrap().unwrap().balance_micros;
        // Rotate to a new key for the SAME account.
        register_device(&pool, b"keyhash-new-0000000000000000", "acct-r", None)
            .await
            .unwrap();
        let resolved = key_account(&pool, b"keyhash-new-0000000000000000")
            .await
            .unwrap()
            .expect("new key resolves the account");
        let wallet = get_wallet(&pool, &resolved).await.unwrap().unwrap();
        assert_eq!(wallet.balance_micros, before, "balance preserved across rotation");
        // Old key no longer resolves.
        assert!(key_account(&pool, b"keyhash-old-0000000000000000")
            .await
            .unwrap()
            .is_none());
    }

    /// Linking a box creates no wallet. An owner who never paid has a key that
    /// resolves to their account, and nothing for a paid route to spend.
    #[sqlx::test]
    async fn register_device_creates_no_wallet(pool: PgPool) {
        register_device(&pool, b"keyhash-free-000000000000000", "acct-free", Some("box-a"))
            .await
            .unwrap();
        assert_eq!(
            key_account(&pool, b"keyhash-free-000000000000000").await.unwrap().as_deref(),
            Some("acct-free")
        );
        assert!(get_wallet(&pool, "acct-free").await.unwrap().is_none());
    }

    /// The image this deploy replaces still reads and writes `accounts`.
    /// Every statement it runs must keep working through the view that
    /// migration 0011 leaves, or the service breaks while the new image is
    /// smoke tested and again after a rollback.
    #[sqlx::test]
    async fn the_previous_image_sql_runs_through_the_accounts_view(pool: PgPool) {
        let q = |sql: &'static str| sqlx::query(sql);
        // register_device's wallet insert, both credit upserts.
        q("INSERT INTO accounts (account_id, balance_micros, expires_at) \
           VALUES ('acct-old', 0, now() + interval '30 days') ON CONFLICT (account_id) DO NOTHING")
            .execute(&pool).await.expect("insert do nothing");
        q("INSERT INTO accounts (account_id, balance_micros, expires_at) \
           VALUES ('acct-old', 9, now() + interval '30 days') ON CONFLICT (account_id) DO UPDATE \
           SET balance_micros = EXCLUDED.balance_micros, expires_at = EXCLUDED.expires_at")
            .execute(&pool).await.expect("credit set");
        q("INSERT INTO accounts (account_id, balance_micros, expires_at) \
           VALUES ('acct-old', 1, now()) ON CONFLICT (account_id) DO UPDATE \
           SET balance_micros = accounts.balance_micros + 1")
            .execute(&pool).await.expect("credit add");
        q("SELECT balance_micros FROM accounts WHERE account_id = 'acct-old' FOR UPDATE")
            .execute(&pool).await.expect("lock for credit");
        q("UPDATE accounts SET balance_micros = balance_micros - 1 \
           WHERE account_id = 'acct-old' AND expires_at > now() AND balance_micros >= 1 RETURNING balance_micros")
            .execute(&pool).await.expect("guarded charge");
        q("INSERT INTO device_keys (api_key_hash, account_id, box_id) VALUES ('\\x01', 'acct-old', NULL)")
            .execute(&pool).await.expect("register key");
        let (id,): (String,) = sqlx::query_as(
            "SELECT a.account_id FROM device_keys k JOIN accounts a ON a.account_id = k.account_id \
             WHERE k.api_key_hash = '\\x01'",
        )
        .fetch_one(&pool)
        .await
        .expect("resolve key through the view");
        assert_eq!(id, "acct-old");

        let wallet = get_wallet(&pool, "acct-old").await.unwrap().expect("written to wallets");
        assert_eq!(wallet.balance_micros, 9, "set to 9, plus 1, minus 1");
        let (created,): (Option<chrono::DateTime<Utc>>,) =
            sqlx::query_as("SELECT created_at FROM wallets WHERE account_id = 'acct-old'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(created.is_some(), "an insert through the view takes the table's defaults");

        q("DELETE FROM accounts WHERE expires_at < now() - interval '7 days'")
            .execute(&pool).await.expect("old sweep");
    }
}

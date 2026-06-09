//! Dev-only entitlement seed.
//!
//! Funds a known bearer so a local *standalone* virtues-api accepts calls
//! without the Atlas voucher → redeem path. Pairs with the virtues-core
//! client override (`VIRTUES_API_BEARER`): the client presents the raw bearer,
//! we store an entitlement keyed by its SHA-256 here, and the hashes line up.
//!
//! The caller gates this to `ENVIRONMENT=dev` + no-Atlas, so it can never run
//! in production. The metering path it unlocks is otherwise fully real — the
//! `$20/day` ceiling and `$5/call` cap in [`crate::entitlement::charge`] still
//! apply (guardrails on the real upstream spend a local box still incurs).

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

/// Default bearer when `VIRTUES_API_BEARER` is unset. Kept in sync with the
/// virtues-core client override and the Makefile's `dev-api`/`dev-core` env.
const DEFAULT_DEV_BEARER: &str = "dev-local-bearer";

/// Wallet to fund the dev entitlement with ($1000 in micros). The per-day and
/// per-call caps in `entitlement::charge` still bound actual spend — this just
/// keeps the wallet itself from running dry mid-iteration.
const DEV_WALLET_MICROS: i64 = 1_000_000_000;

/// Insert a funded entitlement for the dev bearer. Idempotent via
/// `ON CONFLICT DO NOTHING` so restarts never reset a wallet you've been
/// spending against.
pub async fn seed_dev_entitlement(pool: &PgPool) -> Result<()> {
    let bearer = std::env::var("VIRTUES_API_BEARER")
        .ok()
        .filter(|b| !b.is_empty())
        .unwrap_or_else(|| DEFAULT_DEV_BEARER.to_string());

    let bearer_hash = Sha256::digest(bearer.as_bytes()).to_vec();
    let now = chrono::Utc::now();
    // Far-future expiry: dev wallets never lapse. Renewal/sweeper only touch a
    // row when a real voucher is redeemed, which never happens in standalone.
    let expires_at = now + chrono::Duration::days(3650);

    let inserted = sqlx::query(
        "INSERT INTO entitlements \
            (bearer_hash, wallet_micros, today_spent_micros, today_reset_at, expires_at) \
         VALUES ($1, $2, 0, $3, $4) \
         ON CONFLICT (bearer_hash) DO NOTHING",
    )
    .bind(&bearer_hash)
    .bind(DEV_WALLET_MICROS)
    .bind(now)
    .bind(expires_at)
    .execute(pool)
    .await
    .context("seeding dev entitlement")?;

    if inserted.rows_affected() > 0 {
        tracing::warn!(
            "DEV SEED: funded entitlement for bearer '{}' with ${} wallet \
             (ENVIRONMENT=dev, standalone — never runs in production)",
            bearer,
            DEV_WALLET_MICROS / 1_000_000
        );
    } else {
        tracing::info!("DEV SEED: entitlement already present, wallet left untouched");
    }
    Ok(())
}

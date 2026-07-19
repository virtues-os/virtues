//! Dev-only account seed.
//!
//! Funds a known account + device key so a local *standalone* virtues-api
//! accepts calls without atlas. Pairs with the virtues-core client override
//! (`VIRTUES_API_KEY`): the client presents the raw api_key, we register a
//! device key keyed by its SHA-256 here against a funded account, and the
//! hashes line up.
//!
//! Gated to `ENVIRONMENT=dev` by the caller, so it can never run in
//! production. The metering path it unlocks is otherwise fully real — the
//! `$20/day` ceiling and `$5/call` cap in [`crate::entitlement::charge`] still
//! apply.

use anyhow::Result;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::entitlement::{self, CreditMode};

/// Default api_key when `VIRTUES_API_KEY` is unset. Kept in sync with the
/// virtues-core client override and the Makefile's `dev-api`/`dev-core` env.
const DEFAULT_DEV_KEY: &str = "dev-local-key";

/// Opaque account id for the dev account.
const DEV_ACCOUNT_ID: &str = "dev-local-account";

/// Balance to fund the dev account with ($10 in micros). Kept small so a
/// runaway dev loop can't burn much; re-set to this on every boot, so a
/// restart tops it back up. The per-call cap still bounds actual spend.
const DEV_BALANCE_MICROS: i64 = 10_000_000;

/// Seed a funded account + device key for the dev api_key. Idempotent enough:
/// re-credits the account to the dev balance on each boot (dev convenience).
pub async fn seed_dev_account(pool: &PgPool) -> Result<()> {
    let api_key = std::env::var("VIRTUES_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .unwrap_or_else(|| DEFAULT_DEV_KEY.to_string());

    let key_hash = Sha256::digest(api_key.as_bytes()).to_vec();

    // Fund the account (set), then register the device key against it.
    entitlement::credit(
        pool,
        DEV_ACCOUNT_ID,
        DEV_BALANCE_MICROS,
        CreditMode::Set,
        Some("dev-seed"),
    )
    .await?;
    entitlement::register_device(pool, &key_hash, DEV_ACCOUNT_ID).await?;

    tracing::warn!(
        "DEV SEED: funded account '{}' with ${} for api_key '{}' \
         (ENVIRONMENT=dev, standalone — never runs in production)",
        DEV_ACCOUNT_ID,
        DEV_BALANCE_MICROS / 1_000_000,
        api_key
    );
    Ok(())
}

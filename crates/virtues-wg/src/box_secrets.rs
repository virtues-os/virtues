//! Sealed singleton secrets for this box (the per-server CA, the WG server
//! keypair, the rendezvous identity). Thin helper over the `box_secrets` table
//! (migration 0009): the secret material is sealed with the vault master key;
//! non-secret public parts live in `metadata` in the clear.
//!
//! Cross-platform (DB + crypto only) — no WireGuard/netlink here.

use anyhow::{Context, Result};
use sqlx::PgPool;
use virtues_helpers::crypto::TokenEncryptor;

/// Fetch a box secret: `(decrypted secret, public metadata)` if present.
pub async fn get(db: &PgPool, key: &str) -> Result<Option<(String, serde_json::Value)>> {
    let row: Option<(String, serde_json::Value)> =
        sqlx::query_as("SELECT secret_ciphertext, metadata FROM box_secrets WHERE key = $1")
            .bind(key)
            .fetch_optional(db)
            .await
            .context("load box secret")?;
    let Some((ciphertext, metadata)) = row else {
        return Ok(None);
    };
    let enc = TokenEncryptor::from_env().context("vault encryptor")?;
    let secret = enc.decrypt(&ciphertext).context("decrypt box secret")?;
    Ok(Some((secret, metadata)))
}

/// Upsert a box secret (sealed) plus its public metadata.
pub async fn put(
    db: &PgPool,
    key: &str,
    secret: &str,
    metadata: &serde_json::Value,
) -> Result<()> {
    let enc = TokenEncryptor::from_env().context("vault encryptor")?;
    let sealed = enc.encrypt(secret).context("seal box secret")?;
    sqlx::query(
        "INSERT INTO box_secrets (key, secret_ciphertext, metadata, updated_at)
         VALUES ($1, $2, $3, now())
         ON CONFLICT (key) DO UPDATE
           SET secret_ciphertext = EXCLUDED.secret_ciphertext,
               metadata = EXCLUDED.metadata,
               updated_at = now()",
    )
    .bind(key)
    .bind(&sealed)
    .bind(metadata)
    .execute(db)
    .await
    .context("put box secret")?;
    Ok(())
}

/// Insert a box secret only if absent (`ON CONFLICT DO NOTHING`). Used for
/// mint-once singletons that two processes might race to create (e.g. the WG
/// server keypair, minted by both the app and the daemon): the first writer
/// wins, and callers re-read to converge on it. Returns `true` if this call
/// inserted.
pub async fn put_if_absent(
    db: &PgPool,
    key: &str,
    secret: &str,
    metadata: &serde_json::Value,
) -> Result<bool> {
    let enc = TokenEncryptor::from_env().context("vault encryptor")?;
    let sealed = enc.encrypt(secret).context("seal box secret")?;
    let res = sqlx::query(
        "INSERT INTO box_secrets (key, secret_ciphertext, metadata, updated_at)
         VALUES ($1, $2, $3, now())
         ON CONFLICT (key) DO NOTHING",
    )
    .bind(key)
    .bind(&sealed)
    .bind(metadata)
    .execute(db)
    .await
    .context("put-if-absent box secret")?;
    Ok(res.rows_affected() > 0)
}

//! Sealed singleton secrets for this box. Thin helper over the `box_secrets`
//! table (migration 0009): secret material is sealed with the vault master key;
//! non-secret public parts live in `metadata` in the clear.
//!
//! Pure DB + crypto (cross-platform). Used for the box's device-link code, and
//! any other mint-once box secret. (Formerly lived in `virtues-wg`; moved here
//! when WireGuard was removed — it was never WG-specific.)

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
pub async fn put(db: &PgPool, key: &str, secret: &str, metadata: &serde_json::Value) -> Result<()> {
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

/// Insert a box secret only if absent (`ON CONFLICT DO NOTHING`). Returns `true`
/// if this call inserted (mint-once singletons two processes might race to set).
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

/// Remove a box secret. Absent-is-fine by design: callers use this to clear
/// an override (e.g. the relay off switch), and clearing what isn't set is a
/// no-op, not an error.
pub async fn delete(db: &PgPool, key: &str) -> Result<()> {
    sqlx::query("DELETE FROM box_secrets WHERE key = $1")
        .bind(key)
        .execute(db)
        .await
        .context("delete box secret")?;
    Ok(())
}

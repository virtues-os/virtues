//! Sealed singleton secrets for this box: the `kind = 'box'` rows of the
//! `credentials` table. The secret is sealed with the box key; non-secret
//! public parts live in `metadata` in the clear.
//!
//! Pure DB + crypto (cross-platform). Used for the box's iroh secret, the
//! device-link code, the door key and any other mint-once box secret. A box
//! row's id is `box:<key>`, its name is the key, and it is always `active`.

use anyhow::{Context, Result};
use sqlx::PgPool;
use virtues_helpers::crypto::TokenEncryptor;

/// Fetch a box secret: `(decrypted secret, public metadata)` if present.
pub async fn get(db: &PgPool, key: &str) -> Result<Option<(String, serde_json::Value)>> {
    let row: Option<(String, serde_json::Value)> = sqlx::query_as(
        "SELECT secrets_ciphertext, metadata FROM credentials WHERE kind = 'box' AND key = $1",
    )
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
        "INSERT INTO credentials (id, kind, key, name, status, secrets_ciphertext, metadata)
         VALUES ('box:' || $1, 'box', $1, $1, 'active', $2, $3)
         ON CONFLICT (key) WHERE kind = 'box' DO UPDATE
           SET secrets_ciphertext = EXCLUDED.secrets_ciphertext,
               metadata = EXCLUDED.metadata",
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
        "INSERT INTO credentials (id, kind, key, name, status, secrets_ciphertext, metadata)
         VALUES ('box:' || $1, 'box', $1, $1, 'active', $2, $3)
         ON CONFLICT (key) WHERE kind = 'box' DO NOTHING",
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
    sqlx::query("DELETE FROM credentials WHERE kind = 'box' AND key = $1")
        .bind(key)
        .execute(db)
        .await
        .context("delete box secret")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrations = "./migrations")]
    async fn box_rows_round_trip_and_stay_out_of_the_source_list(pool: PgPool) {
        // Sealing needs a vault key; CI has none. Thirty-two zero bytes, as
        // the crypto crate's own tests use, and only when nothing is set.
        if std::env::var("VIRTUES_ENCRYPTION_KEY").is_err() {
            use base64::Engine;
            std::env::set_var(
                "VIRTUES_ENCRYPTION_KEY",
                base64::engine::general_purpose::STANDARD.encode([0u8; 32]),
            );
        }
        let meta = serde_json::json!({ "note": "public" });

        assert!(put_if_absent(&pool, "test_key", "first", &meta).await.unwrap());
        assert!(!put_if_absent(&pool, "test_key", "second", &meta).await.unwrap());
        assert_eq!(get(&pool, "test_key").await.unwrap(), Some(("first".to_string(), meta.clone())));

        put(&pool, "test_key", "replaced", &serde_json::json!({})).await.unwrap();
        let (secret, _) = get(&pool, "test_key").await.unwrap().unwrap();
        assert_eq!(secret, "replaced");

        // The owner's credential list is source rows only: a box key must
        // never show up in Settings, attention or the model's context.
        let listed = crate::api::credentials::list_credentials(&pool).await.unwrap();
        assert!(listed.iter().all(|c| c.id != "box:test_key"), "box row leaked into the source list");

        delete(&pool, "test_key").await.unwrap();
        assert_eq!(get(&pool, "test_key").await.unwrap(), None);
        delete(&pool, "test_key").await.unwrap();
    }
}

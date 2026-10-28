//! Named secrets on the box: one value per name, sealed with the box key, in
//! the `credentials` table (agents/plan/secrets-plan.md).
//!
//! Two kinds are key/value:
//! - [`Kind::Box`]: the box's own keys (the iroh secret, the door key, the
//!   link code, the relay config). Core reads them; nothing else does.
//! - [`Kind::Env`]: a named key an applet declares it needs, such as
//!   `OPENAI_API_KEY`, set by the owner.
//!
//! `source` rows (a sync's session or tokens) and `login` rows (saved
//! passwords) can have several per key, one per account, so they have their
//! own paths: `virtues_helpers::auth::vault` for sources.
//!
//! A row's id is `<kind>:<key>`, its name is the key, and it is always
//! `active`. The secret is stored as the bare string; non-secret public parts
//! live in `metadata` in the clear.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use virtues_helpers::crypto::TokenEncryptor;

/// Which named secrets: the box's own, or the owner's for applets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Box,
    Env,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Box => "box",
            Kind::Env => "env",
        }
    }
}

/// One named secret without its value, for listing.
#[derive(Debug, Clone, PartialEq)]
pub struct Named {
    pub key: String,
    pub metadata: serde_json::Value,
    pub updated_at: DateTime<Utc>,
}

/// Fetch a secret: `(decrypted value, public metadata)` if present.
pub async fn get(db: &PgPool, kind: Kind, key: &str) -> Result<Option<(String, serde_json::Value)>> {
    let row: Option<(String, serde_json::Value)> = sqlx::query_as(
        "SELECT secrets_ciphertext, metadata FROM credentials WHERE kind = $1 AND key = $2",
    )
    .bind(kind.as_str())
    .bind(key)
    .fetch_optional(db)
    .await
    .with_context(|| format!("load {} secret", kind.as_str()))?;
    let Some((ciphertext, metadata)) = row else {
        return Ok(None);
    };
    let enc = TokenEncryptor::from_env().context("vault encryptor")?;
    let secret = enc.decrypt(&ciphertext).with_context(|| format!("decrypt {} secret", kind.as_str()))?;
    Ok(Some((secret, metadata)))
}

/// Upsert a secret (sealed) plus its public metadata.
pub async fn put(db: &PgPool, kind: Kind, key: &str, secret: &str, metadata: &serde_json::Value) -> Result<()> {
    let enc = TokenEncryptor::from_env().context("vault encryptor")?;
    let sealed = enc.encrypt(secret).context("seal secret")?;
    sqlx::query(
        "INSERT INTO credentials (id, kind, key, name, status, secrets_ciphertext, metadata)
         VALUES ($1 || ':' || $2, $1, $2, $2, 'active', $3, $4)
         ON CONFLICT (kind, key) WHERE kind IN ('box', 'env') DO UPDATE
           SET secrets_ciphertext = EXCLUDED.secrets_ciphertext,
               metadata = EXCLUDED.metadata",
    )
    .bind(kind.as_str())
    .bind(key)
    .bind(&sealed)
    .bind(metadata)
    .execute(db)
    .await
    .with_context(|| format!("put {} secret", kind.as_str()))?;
    Ok(())
}

/// Insert a secret only if absent (`ON CONFLICT DO NOTHING`). Returns `true`
/// if this call inserted (mint-once singletons two processes might race to set).
pub async fn put_if_absent(
    db: &PgPool,
    kind: Kind,
    key: &str,
    secret: &str,
    metadata: &serde_json::Value,
) -> Result<bool> {
    let enc = TokenEncryptor::from_env().context("vault encryptor")?;
    let sealed = enc.encrypt(secret).context("seal secret")?;
    let res = sqlx::query(
        "INSERT INTO credentials (id, kind, key, name, status, secrets_ciphertext, metadata)
         VALUES ($1 || ':' || $2, $1, $2, $2, 'active', $3, $4)
         ON CONFLICT (kind, key) WHERE kind IN ('box', 'env') DO NOTHING",
    )
    .bind(kind.as_str())
    .bind(key)
    .bind(&sealed)
    .bind(metadata)
    .execute(db)
    .await
    .with_context(|| format!("put-if-absent {} secret", kind.as_str()))?;
    Ok(res.rows_affected() > 0)
}

/// Remove a secret. Absent-is-fine by design: callers use this to clear an
/// override (e.g. the relay off switch), and clearing what isn't set is a
/// no-op, not an error.
pub async fn delete(db: &PgPool, kind: Kind, key: &str) -> Result<()> {
    sqlx::query("DELETE FROM credentials WHERE kind = $1 AND key = $2")
        .bind(kind.as_str())
        .bind(key)
        .execute(db)
        .await
        .with_context(|| format!("delete {} secret", kind.as_str()))?;
    Ok(())
}

/// Every secret of a kind, by name, without values. What Settings lists and
/// what a chat may know exists.
pub async fn list(db: &PgPool, kind: Kind) -> Result<Vec<Named>> {
    let rows: Vec<(String, serde_json::Value, DateTime<Utc>)> = sqlx::query_as(
        "SELECT key, metadata, updated_at FROM credentials WHERE kind = $1 ORDER BY key",
    )
    .bind(kind.as_str())
    .fetch_all(db)
    .await
    .with_context(|| format!("list {} secrets", kind.as_str()))?;
    Ok(rows
        .into_iter()
        .map(|(key, metadata, updated_at)| Named { key, metadata, updated_at })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ensure_test_key() {
        // Sealing needs a vault key; CI has none. Thirty-two zero bytes, as
        // the crypto crate's own tests use, and only when nothing is set.
        if std::env::var("VIRTUES_ENCRYPTION_KEY").is_err() {
            use base64::Engine;
            std::env::set_var(
                "VIRTUES_ENCRYPTION_KEY",
                base64::engine::general_purpose::STANDARD.encode([0u8; 32]),
            );
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn box_rows_round_trip_and_stay_out_of_the_source_list(pool: PgPool) {
        ensure_test_key();
        let meta = serde_json::json!({ "note": "public" });

        assert!(put_if_absent(&pool, Kind::Box, "test_key", "first", &meta).await.unwrap());
        assert!(!put_if_absent(&pool, Kind::Box, "test_key", "second", &meta).await.unwrap());
        assert_eq!(get(&pool, Kind::Box, "test_key").await.unwrap(), Some(("first".to_string(), meta.clone())));

        put(&pool, Kind::Box, "test_key", "replaced", &serde_json::json!({})).await.unwrap();
        let (secret, _) = get(&pool, Kind::Box, "test_key").await.unwrap().unwrap();
        assert_eq!(secret, "replaced");

        // The owner's credential list is source rows only: a box key must
        // never show up in Settings, attention or the model's context.
        let listed = crate::api::credentials::list_credentials(&pool).await.unwrap();
        assert!(listed.iter().all(|c| c.id != "box:test_key"), "box row leaked into the source list");

        delete(&pool, Kind::Box, "test_key").await.unwrap();
        assert_eq!(get(&pool, Kind::Box, "test_key").await.unwrap(), None);
        delete(&pool, Kind::Box, "test_key").await.unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn env_and_box_keys_with_one_name_are_separate(pool: PgPool) {
        ensure_test_key();
        put(&pool, Kind::Env, "SHARED_NAME", "for applets", &serde_json::json!({})).await.unwrap();
        put(&pool, Kind::Box, "SHARED_NAME", "for core", &serde_json::json!({})).await.unwrap();
        put(&pool, Kind::Env, "OPENAI_API_KEY", "sk-test", &serde_json::json!({ "hint": "test" })).await.unwrap();

        assert_eq!(get(&pool, Kind::Env, "SHARED_NAME").await.unwrap().unwrap().0, "for applets");
        assert_eq!(get(&pool, Kind::Box, "SHARED_NAME").await.unwrap().unwrap().0, "for core");

        let names: Vec<String> = list(&pool, Kind::Env).await.unwrap().into_iter().map(|n| n.key).collect();
        assert_eq!(names, ["OPENAI_API_KEY", "SHARED_NAME"]);

        delete(&pool, Kind::Env, "SHARED_NAME").await.unwrap();
        assert_eq!(get(&pool, Kind::Env, "SHARED_NAME").await.unwrap(), None);
        assert!(get(&pool, Kind::Box, "SHARED_NAME").await.unwrap().is_some(), "deleting the env key left the box key");
    }
}

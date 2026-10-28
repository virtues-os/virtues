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

/// What a key did with a set of sealed rows: how many it tried, and how many
/// it opened.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyCheck {
    pub tried: usize,
    pub opened: usize,
}

impl KeyCheck {
    /// The key opened none of the rows it tried. One row that does not open
    /// is damage, or a row sealed before a key change; every row failing
    /// means the key is not the one the table was sealed with.
    pub fn opens_nothing(&self) -> bool {
        self.tried > 0 && self.opened == 0
    }

    pub fn failed(&self) -> usize {
        self.tried - self.opened
    }
}

/// Try a key on sealed values. `None` is a box with no usable key at all,
/// which opens nothing. Every kind of row is sealed with the same box key, so
/// any row is a fair test of it.
pub fn try_key<'a>(enc: Option<&TokenEncryptor>, sealed: impl IntoIterator<Item = &'a str>) -> KeyCheck {
    let mut check = KeyCheck::default();
    for value in sealed {
        check.tried += 1;
        if enc.is_some_and(|enc| enc.decrypt(value).is_ok()) {
            check.opened += 1;
        }
    }
    check
}

/// Up to `limit` sealed values from `credentials`, chosen at random.
///
/// Reads only `secrets_ciphertext`, which every schema since the first has,
/// so it also works on a database restored from an older release before its
/// migrations run. An empty value is not sealed (`encrypt("")` writes one).
/// A database with no `credentials` table yet, a box on its first start, has
/// nothing sealed.
pub async fn sample_sealed(db: &PgPool, limit: i64) -> Result<Vec<String>> {
    let has_table: bool = sqlx::query_scalar("SELECT to_regclass('credentials') IS NOT NULL")
        .fetch_one(db)
        .await
        .context("look for the credentials table")?;
    if !has_table {
        return Ok(Vec::new());
    }
    sqlx::query_scalar(
        "SELECT secrets_ciphertext FROM credentials WHERE secrets_ciphertext <> '' \
         ORDER BY random() LIMIT $1",
    )
    .bind(limit)
    .fetch_all(db)
    .await
    .context("sample sealed credentials")
}

/// How many sealed rows the start-up check tries. Enough that one damaged row
/// cannot make a good key look wrong, few enough to cost nothing.
const START_CHECK_ROWS: i64 = 5;

static KEY_AT_START: std::sync::OnceLock<KeyCheck> = std::sync::OnceLock::new();

/// Check the server's key against the table once, when the server starts.
///
/// A key that opens none of the sealed rows is the state a restore with the
/// wrong `virtues.env` leaves. Each feature then fails on its own (reach,
/// Passwords, applets that need a key) with an error that never names the
/// key, and every secret saved afterwards is sealed with the wrong one. So
/// this logs one error that names the cause, and attention shows it
/// ([`key_at_start`]). It never stops the server: everything that needs no
/// secret still works, and the owner needs a running server to read the
/// message.
pub async fn check_key_at_start(db: &PgPool) {
    let sealed = match sample_sealed(db, START_CHECK_ROWS).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("key check skipped: {e:#}");
            return;
        }
    };
    let enc = TokenEncryptor::from_env().ok();
    let check = try_key(enc.as_ref(), sealed.iter().map(String::as_str));
    if check.opens_nothing() {
        tracing::error!(
            tried = check.tried,
            env_file = %crate::box_env::path().display(),
            "the encryption key this server started with opens none of the saved secrets it tried, \
             so every saved password, sign-in and key is unreadable. Put back the virtues.env they \
             were saved with, then restart the server"
        );
    }
    let _ = KEY_AT_START.set(check);
}

/// What [`check_key_at_start`] found, or `None` before it has run (and in
/// any process that is not the server).
pub fn key_at_start() -> Option<KeyCheck> {
    KEY_AT_START.get().copied()
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

    fn encryptor(byte: u8) -> TokenEncryptor {
        use base64::Engine;
        TokenEncryptor::from_base64_key(&base64::engine::general_purpose::STANDARD.encode([byte; 32])).unwrap()
    }

    #[test]
    fn a_key_check_tells_the_wrong_key_from_one_damaged_row() {
        let right = encryptor(1);
        let wrong = encryptor(2);
        let sealed: Vec<String> = ["a", "b", "c"].iter().map(|s| right.encrypt(s).unwrap()).collect();
        let rows = || sealed.iter().map(String::as_str);

        assert_eq!(try_key(Some(&right), rows()), KeyCheck { tried: 3, opened: 3 });
        let wrong_key = try_key(Some(&wrong), rows());
        assert!(wrong_key.opens_nothing(), "{wrong_key:?}");
        assert!(try_key(None, rows()).opens_nothing(), "no key opens nothing");

        // A damaged row beside good ones is not a wrong key.
        let mixed = try_key(Some(&right), rows().chain(["not sealed"]));
        assert_eq!((mixed.opened, mixed.failed()), (3, 1));
        assert!(!mixed.opens_nothing());

        // Nothing sealed is nothing to be wrong about.
        assert!(!try_key(Some(&wrong), std::iter::empty()).opens_nothing());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn the_sample_holds_only_sealed_values(pool: PgPool) {
        let enc = encryptor(3);
        sqlx::query(
            "INSERT INTO credentials (id, key, name, status, secrets_ciphertext) VALUES \
             ('cred_sealed', 'ios', 'Phone', 'active', $1), \
             ('cred_empty', 'mac', 'Mac', 'pending', '')",
        )
        .bind(enc.encrypt("{\"token\":\"invented\"}").unwrap())
        .execute(&pool)
        .await
        .unwrap();
        let sample = sample_sealed(&pool, 10).await.unwrap();
        assert_eq!(sample.len(), 1, "an empty value is not sealed: {sample:?}");
        assert_eq!(try_key(Some(&enc), sample.iter().map(String::as_str)).opened, 1);
        assert!(try_key(Some(&encryptor(4)), sample.iter().map(String::as_str)).opens_nothing());
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

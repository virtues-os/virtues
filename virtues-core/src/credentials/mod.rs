//! The credentials Vault: one table, `credentials`, for every secret on the box.
//!
//! - `kind` says what a row is: `source` (what a sync uses, keyed by source
//!   id), `box` (the box's own keys) and `env` (named keys applets declare),
//!   both through `crate::secrets`, `login` (a saved password,
//!   `api/passwords.rs`) and `connection` (`tools/connection.rs`). `key`
//!   names what the row unlocks.
//! - The secret payload is encrypted JSON whose shape the source declares.
//! - Status state machine: `pending` → `active` → (`reauth_required` |
//!   `error` | `revoked`). Only `active` credentials are usable. A revoked
//!   row keeps its id, because run history points at it, but its payload is
//!   replaced with [`emptied_secrets`] in the same statement that revokes it.
//!   Rows revoked before that are emptied at server start
//!   ([`empty_revoked_at_start`]).

use sqlx::PgPool;

use crate::crypto::TokenEncryptor;
use crate::error::{Error, Result};

pub mod jar;

/// The payload a revoked row keeps: `{}`, sealed under the box key.
///
/// Revoking wipes the value, so a backup taken later no longer holds a token
/// or key the owner removed. The old row version stays on disk until Postgres
/// vacuums it. The payload is sealed rather than left empty because some
/// readers decrypt by id without checking the status
/// (`vault::read_credential_secrets`), and an empty object decodes for every
/// one of them.
pub fn emptied_secrets() -> Result<String> {
    let enc = TokenEncryptor::from_env().map_err(|e| Error::Other(format!("vault encryptor: {e:#}")))?;
    enc.encrypt("{}").map_err(|e| Error::Other(format!("seal an emptied credential: {e:#}")))
}

/// Empty the payload of every revoked source row that still holds one, and
/// clear its refresh schedule. Before revoking also emptied the payload, a
/// revoke set only the status, so those rows still hold the token or key the
/// owner removed, and every backup carries it.
///
/// The box's own `virtues_api` row is left alone, because
/// `virtues_api::renew` reads its key whatever its status, and an older
/// release could revoke it while it was still the key in use.
///
/// It needs no marker to run once. A row whose payload already opens to `{}`
/// with no expiry is skipped, so a second run writes nothing. A payload that
/// does not open is emptied as well: the row is revoked, so nothing reads it.
/// Each write is conditional on the row still being revoked with the payload
/// that was read. Returns how many rows it emptied.
pub async fn empty_revoked_sources(db: &PgPool) -> Result<u64> {
    let rows: Vec<(String, String, bool)> = sqlx::query_as(
        "SELECT id, secrets_ciphertext, expires_at IS NULL AND next_refresh_at IS NULL \
           FROM credentials \
          WHERE kind = 'source' AND status = 'revoked' AND key <> $1",
    )
    .bind(crate::virtues_api::renew::SOURCE_ID)
    .fetch_all(db)
    .await?;
    if rows.is_empty() {
        return Ok(0);
    }

    let enc = TokenEncryptor::from_env().map_err(|e| Error::Other(format!("vault encryptor: {e:#}")))?;
    let emptied = emptied_secrets()?;
    let mut count = 0;
    for (id, sealed, unscheduled) in rows {
        if unscheduled && opens_to_empty(&enc, &sealed) {
            continue;
        }
        count += sqlx::query(
            "UPDATE credentials \
                SET secrets_ciphertext = $1, expires_at = NULL, next_refresh_at = NULL \
              WHERE id = $2 AND kind = 'source' AND status = 'revoked' AND secrets_ciphertext = $3",
        )
        .bind(&emptied)
        .bind(&id)
        .bind(&sealed)
        .execute(db)
        .await?
        .rows_affected();
    }
    Ok(count)
}

/// True when `sealed` opens to an empty JSON object.
fn opens_to_empty(enc: &TokenEncryptor, sealed: &str) -> bool {
    enc.decrypt(sealed)
        .ok()
        .and_then(|plaintext| serde_json::from_str::<serde_json::Value>(&plaintext).ok())
        .is_some_and(|v| v.as_object().is_some_and(|o| o.is_empty()))
}

/// How long the start-up pass waits for the server's migrations.
const MIGRATIONS_WAIT: std::time::Duration = std::time::Duration::from_secs(600);

/// Run [`empty_revoked_sources`] once the server has started. It waits for
/// the migrations, which run beside it and rename the columns it reads on a
/// box upgrading from an older schema. It runs only with a key that opens the
/// saved secrets: under the wrong key every revoked row would look unreadable
/// and be sealed again under that key, which would also hide the wrong key
/// from `secrets::check_key_at_start` on the next start. It logs what it did
/// and never stops the server.
pub async fn empty_revoked_at_start(database: &crate::database::Database) {
    // A database managed elsewhere (`make dev-real`) is already at its schema
    // and is read, never repaired.
    if std::env::var("VIRTUES_SKIP_MIGRATIONS").as_deref() == Ok("1") {
        return;
    }
    let deadline = tokio::time::Instant::now() + MIGRATIONS_WAIT;
    loop {
        // An error here is usually Postgres still starting up, so it waits
        // like a pending migration does.
        let waiting = match database.migration_check().await {
            Ok(check) if check.pending.is_empty() => break,
            Ok(check) => format!("migrations {:?} are still pending", check.pending),
            Err(e) => e.to_string(),
        };
        if tokio::time::Instant::now() >= deadline {
            tracing::warn!("did not empty revoked credentials: {waiting}");
            return;
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    let db = database.pool();
    let sealed = match crate::secrets::sample_sealed(db, 5).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("did not empty revoked credentials: {e:#}");
            return;
        }
    };
    let enc = TokenEncryptor::from_env().ok();
    if crate::secrets::try_key(enc.as_ref(), sealed.iter().map(String::as_str)).opens_nothing() {
        tracing::warn!(
            "did not empty revoked credentials, because the server's key opens none of the saved secrets"
        );
        return;
    }
    match empty_revoked_sources(db).await {
        Ok(0) => {}
        Ok(emptied) => tracing::info!(emptied, "emptied credentials that an earlier release revoked"),
        Err(e) => tracing::warn!("could not empty revoked credentials: {e}"),
    }
}

/// Gives a test that seals a vault key when none is set, since CI has none:
/// thirty-two zero bytes, as the crypto crate's own tests use.
// TODO(2026-10-27): migrate the copies in api/keys.rs, api/source_auth.rs and
// secrets.rs onto this.
#[cfg(test)]
pub(crate) fn ensure_test_key() {
    if std::env::var("VIRTUES_ENCRYPTION_KEY").is_err() {
        use base64::Engine;
        std::env::set_var(
            "VIRTUES_ENCRYPTION_KEY",
            base64::engine::general_purpose::STANDARD.encode([0u8; 32]),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn insert(pool: &PgPool, id: &str, kind: &str, key: &str, status: &str, sealed: &str) {
        sqlx::query(
            "INSERT INTO credentials \
                 (id, kind, key, name, status, secrets_ciphertext, expires_at, next_refresh_at) \
             VALUES ($1, $2, $3, 'nick@example.com', $4, $5, \
                     now() + interval '1 hour', now() + interval '59 minutes')",
        )
        .bind(id)
        .bind(kind)
        .bind(key)
        .bind(status)
        .bind(sealed)
        .execute(pool)
        .await
        .unwrap();
    }

    /// The row's sealed payload, and whether it still has a refresh schedule.
    async fn payload(pool: &PgPool, id: &str) -> (String, bool) {
        sqlx::query_as(
            "SELECT secrets_ciphertext, expires_at IS NOT NULL OR next_refresh_at IS NOT NULL \
               FROM credentials WHERE id = $1",
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn rows_an_older_release_revoked_are_emptied_once(pool: PgPool) {
        ensure_test_key();
        let enc = TokenEncryptor::from_env().unwrap();
        let token = enc.encrypt(r#"{"refresh_token":"rt-example"}"#).unwrap();
        let api_key = enc.encrypt(r#"{"api_key":"vk-example"}"#).unwrap();

        // Revoked by an older release: the status changed, the token stayed.
        insert(&pool, "cred_old", "source", "example_source", "revoked", &token).await;
        // A payload that does not open, as one sealed under another key.
        insert(&pool, "cred_unreadable", "source", "example_source", "revoked", "not-sealed").await;
        // Revoked by this release, which empties in the same statement.
        let empty = emptied_secrets().unwrap();
        insert(&pool, "cred_new", "source", "example_source", "revoked", &empty).await;
        sqlx::query("UPDATE credentials SET expires_at = NULL, next_refresh_at = NULL WHERE id = 'cred_new'")
            .execute(&pool)
            .await
            .unwrap();
        // Left alone: in use, the box's own key, and rows of other kinds.
        insert(&pool, "cred_active", "source", "example_source", "active", &token).await;
        let box_key = crate::virtues_api::renew::SOURCE_ID;
        insert(&pool, "cred_box_key", "source", box_key, "revoked", &api_key).await;
        insert(&pool, "login_example", "login", "example.com", "revoked", &token).await;

        assert_eq!(empty_revoked_sources(&pool).await.unwrap(), 2);

        for id in ["cred_old", "cred_unreadable"] {
            let (sealed, scheduled) = payload(&pool, id).await;
            assert!(opens_to_empty(&enc, &sealed), "{id} still holds a value");
            assert!(!scheduled, "{id} still has a refresh schedule");
        }
        assert_eq!(payload(&pool, "cred_new").await, (empty, false), "already empty, so not rewritten");
        assert_eq!(payload(&pool, "cred_active").await, (token.clone(), true));
        assert_eq!(payload(&pool, "cred_box_key").await, (api_key, true));
        assert_eq!(payload(&pool, "login_example").await, (token, true));
        let status: String = sqlx::query_scalar("SELECT status FROM credentials WHERE id = 'cred_old'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "revoked");

        assert_eq!(empty_revoked_sources(&pool).await.unwrap(), 0, "a second run writes nothing");
    }
}

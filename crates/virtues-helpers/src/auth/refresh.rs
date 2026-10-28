//! Just-in-time credential refresh.
//!
//! `ensure_fresh` is called from `applet_runner::load_credentials` immediately
//! before dispatching a subprocess, and by the `credential_refresh` applet's
//! sweep. If the credential's access token has expired (or is within the 60s
//! safety margin), it refreshes inline via the OAuth proxy and writes the new
//! secrets back. The subprocess always sees a valid token; extractors never
//! need to handle 401 / refresh themselves.
//!
//! ## Concurrency
//!
//! One refresh of a credential runs at a time on the box. Core's applet runner
//! and the `credential_refresh` applet are separate processes, both refresh,
//! and both find a token due at the same moment. A Postgres advisory lock keyed
//! on the credential id (`CredentialLock`) makes the second one wait. A
//! per-credential async mutex in front of it keeps the tasks of one process
//! from each holding a database connection while they wait. Whoever gets the
//! lock second re-reads the row and finds the token already fresh.
//!
//! The write after a refresh is conditional on the row still being the one
//! the refresh read: active, with the same ciphertext. Disconnecting a source
//! does not take the lock, so a source the owner disconnects during the proxy
//! call stays disconnected, and the tokens the proxy returned are dropped.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use sqlx::PgPool;
use tokio::sync::Mutex;

use crate::auth::error::{AuthError, Result};
use crate::auth::proxy::{is_refused_grant, proxy_refresh, PROXY_TIMEOUT};
use crate::auth::vault::CredentialStatus;
use crate::crypto::TokenEncryptor;

/// Safety margin: refresh when access token has this much (or less) life left.
const REFRESH_MARGIN_SECS: i64 = 60;

/// The first half of every refresh advisory lock key: ASCII "cred". The
/// two-int key space (`pg_advisory_lock(int4, int4)`) is apart from the
/// one-bigint space the indexer and `sqlx migrate` lock in, and the class
/// keeps it apart from core's day-write locks ("dayw").
const REFRESH_LOCK_CLASS: i32 = 0x6372_6564;

/// How long a refresh waits for another process's refresh of the same
/// credential: that one's proxy call, plus time for its write.
const LOCK_WAIT: std::time::Duration = std::time::Duration::from_secs(PROXY_TIMEOUT.as_secs() + 30);

fn refresh_locks() -> &'static Mutex<HashMap<String, Arc<Mutex<()>>>> {
    static LOCKS: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();
    LOCKS.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn lock_for(credential_id: &str) -> Arc<Mutex<()>> {
    let mut map = refresh_locks().lock().await;
    map.entry(credential_id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

/// The cross-process half of the refresh lock: a session advisory lock keyed
/// on the credential id, on a connection taken out of the pool.
///
/// A refresh holds it across one proxy call of up to [`PROXY_TIMEOUT`], which
/// is too long to keep a transaction open, and a session lock on a connection
/// that went back to the pool would stay with whoever got that connection
/// next. Dropping this closes the connection, which releases the lock on every
/// exit path, a cancelled dispatch or a killed applet included (core's
/// `day_summary::DayWriteLock` does the same).
struct CredentialLock(sqlx::PgConnection);

impl CredentialLock {
    /// Wait up to `wait` for the credential. A wait that runs out is a
    /// `Conflict`: another refresh of it is still going.
    async fn acquire(db: &PgPool, credential_id: &str, wait: std::time::Duration) -> Result<Self> {
        let mut conn = db.acquire().await?.detach();
        // The connection belongs to this lock alone and closes with it, so a
        // session setting cannot reach any other query.
        sqlx::query("SELECT set_config('lock_timeout', $1, false)")
            .bind(format!("{}ms", wait.as_millis()))
            .execute(&mut conn)
            .await?;
        let locked = sqlx::query("SELECT pg_advisory_lock($1::int4, hashtext($2))")
            .bind(REFRESH_LOCK_CLASS)
            .bind(credential_id)
            .execute(&mut conn)
            .await;
        match locked {
            Ok(_) => Ok(Self(conn)),
            Err(e) if is_lock_timeout(&e) => Err(AuthError::Conflict(format!(
                "credential {credential_id}: another refresh did not finish within {}s",
                wait.as_secs()
            ))),
            Err(e) => Err(e.into()),
        }
    }

    /// Let the credential go now and close the connection cleanly. Dropping
    /// the lock releases it too, by closing the connection without a goodbye.
    async fn release(mut self) {
        if let Err(e) = sqlx::query("SELECT pg_advisory_unlock_all()")
            .execute(&mut self.0)
            .await
        {
            tracing::debug!(error = %e, "could not unlock the credential; closing the connection releases it");
        }
        if let Err(e) = sqlx::Connection::close(self.0).await {
            tracing::debug!(error = %e, "the refresh lock's connection did not close cleanly");
        }
    }
}

/// Postgres's `lock_not_available`, which a wait past `lock_timeout` raises.
fn is_lock_timeout(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.code())
        .is_some_and(|code| code == "55P03")
}

#[derive(Debug, sqlx::FromRow)]
struct CredRow {
    key: String,
    status: String,
    expires_at: Option<DateTime<Utc>>,
    secrets_ciphertext: String,
}

#[derive(Debug, Deserialize)]
struct OauthSecrets {
    refresh_token: Option<String>,
}

/// Outcome of a single `ensure_fresh` call. Mostly for tests / logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshOutcome {
    Fresh,
    Refreshed,
    NoRefreshable,
}

/// Ensure the credential's access token is valid for the next dispatch.
pub async fn ensure_fresh(db: &PgPool, credential_id: &str) -> Result<RefreshOutcome> {
    let row = read_active(db, credential_id).await?;
    if !needs_refresh(row.expires_at) {
        return Ok(if row.expires_at.is_some() {
            RefreshOutcome::Fresh
        } else {
            RefreshOutcome::NoRefreshable
        });
    }

    let lock = lock_for(credential_id).await;
    let _guard = lock.lock().await;
    let held = CredentialLock::acquire(db, credential_id, LOCK_WAIT).await?;
    let outcome = refresh_locked(db, credential_id).await;
    held.release().await;
    outcome
}

/// The refresh itself, for a caller holding both locks.
async fn refresh_locked(db: &PgPool, credential_id: &str) -> Result<RefreshOutcome> {
    // Re-read under the lock: another task or process may have refreshed the
    // token while this one waited, or the owner may have disconnected the
    // source.
    let row = read_active(db, credential_id).await?;
    if !needs_refresh(row.expires_at) {
        return Ok(RefreshOutcome::Fresh);
    }

    let encryptor = TokenEncryptor::from_env()?;
    let plaintext = encryptor.decrypt(&row.secrets_ciphertext)?;
    let secrets: serde_json::Value = serde_json::from_str(&plaintext)?;
    let refresh_token = match serde_json::from_value::<OauthSecrets>(secrets) {
        Ok(s) => match s.refresh_token {
            Some(t) if !t.is_empty() => t,
            _ => return Ok(RefreshOutcome::NoRefreshable),
        },
        Err(_) => return Ok(RefreshOutcome::NoRefreshable),
    };

    // Identify this box to the proxy (see proxy_refresh). Unlinked is fine;
    // an unreadable vault is not, and is logged rather than passed off as
    // "no key".
    let api_key = match super::vault::read_box_api_key(db).await {
        Ok(k) => k,
        Err(e) => {
            tracing::warn!(credential_id, error = %e, "could not read box api_key for proxy refresh");
            None
        }
    };
    match proxy_refresh(&row.key, &refresh_token, api_key.as_deref()).await {
        Ok(resp) => {
            let ciphertext = encryptor.encrypt(&serde_json::to_string(&resp.secrets)?)?;
            let stored = store_refreshed(
                db,
                credential_id,
                &row.secrets_ciphertext,
                &ciphertext,
                resp.expires_in,
            )
            .await?;
            if !stored {
                return Err(AuthError::Conflict(format!(
                    "credential {credential_id} changed during its refresh; the new tokens were dropped"
                )));
            }
            tracing::info!(
                credential_id,
                source_id = %row.key,
                expires_in = resp.expires_in,
                "credential refreshed just-in-time"
            );
            Ok(RefreshOutcome::Refreshed)
        }
        Err(e) if is_refused_grant(&e) => {
            match mark_refused(db, credential_id, &row.secrets_ciphertext).await {
                Ok(true) => tracing::warn!(
                    credential_id,
                    source_id = %row.key,
                    err = %e,
                    "refresh rejected by provider; marked reauth_required"
                ),
                Ok(false) => tracing::info!(
                    credential_id,
                    source_id = %row.key,
                    "refresh rejected by provider, but the credential changed meanwhile; left as it is"
                ),
                Err(db_err) => tracing::warn!(
                    credential_id,
                    source_id = %row.key,
                    error = %db_err,
                    "refresh rejected by provider; could not mark reauth_required"
                ),
            }
            Err(e)
        }
        Err(e) => Err(e),
    }
}

/// The credential's row, if it exists and is active.
async fn read_active(db: &PgPool, credential_id: &str) -> Result<CredRow> {
    let row: Option<CredRow> = sqlx::query_as(
        r#"SELECT key, status, expires_at, secrets_ciphertext
             FROM credentials WHERE id = $1 AND kind = 'source'"#,
    )
    .bind(credential_id)
    .fetch_optional(db)
    .await?;

    let Some(row) = row else {
        return Err(AuthError::NotFound(credential_id.to_string()));
    };
    if row.status != "active" {
        return Err(AuthError::Conflict(format!(
            "credential {credential_id} not active (status={})",
            row.status
        )));
    }
    Ok(row)
}

// TODO(2026-10-27): migrate this write and `mark_refused` into vault.rs, next
// to `update_credential_secrets` and `mark_credential_status`, which write the
// same columns without the ciphertext condition, and share one expires_at /
// next_refresh_at calculation with them.
/// Write refreshed secrets, but only while the row is active and still has
/// `read_ciphertext`, the secrets the refresh started from. False when it does
/// not, and then nothing was written. A stored refresh counts as a use of the
/// credential, so it sets `last_seen_at`.
async fn store_refreshed(
    db: &PgPool,
    credential_id: &str,
    read_ciphertext: &str,
    new_ciphertext: &str,
    expires_in: Option<i64>,
) -> Result<bool> {
    let expires_at = expires_in.map(|secs| Utc::now() + Duration::seconds(secs));
    let next_refresh_at = expires_at.map(|exp| exp - Duration::seconds(REFRESH_MARGIN_SECS));
    let result = sqlx::query(
        r#"UPDATE credentials
              SET secrets_ciphertext = $1,
                  expires_at = $2,
                  next_refresh_at = $3,
                  status_reason = NULL,
                  last_seen_at = now(),
                  updated_at = now()
            WHERE id = $4 AND kind = 'source'
              AND status = 'active' AND secrets_ciphertext = $5"#,
    )
    .bind(new_ciphertext)
    .bind(expires_at)
    .bind(next_refresh_at)
    .bind(credential_id)
    .bind(read_ciphertext)
    .execute(db)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Mark the credential `reauth_required` after a refused grant, but only while
/// the row is active and still has `read_ciphertext`. A refusal of tokens
/// that were replaced since, or of a source the owner disconnected, says
/// nothing about the row as it is now. False when nothing was marked.
async fn mark_refused(db: &PgPool, credential_id: &str, read_ciphertext: &str) -> Result<bool> {
    let result = sqlx::query(
        r#"UPDATE credentials
              SET status = $1,
                  status_reason = 'token_rejected_by_provider',
                  updated_at = now()
            WHERE id = $2 AND kind = 'source'
              AND status = 'active' AND secrets_ciphertext = $3"#,
    )
    .bind(CredentialStatus::ReauthRequired.as_str())
    .bind(credential_id)
    .bind(read_ciphertext)
    .execute(db)
    .await?;
    Ok(result.rows_affected() == 1)
}

fn needs_refresh(expires_at: Option<DateTime<Utc>>) -> bool {
    let Some(dt) = expires_at else { return false };
    dt < Utc::now() + Duration::seconds(REFRESH_MARGIN_SECS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_refresh_none() {
        assert!(!needs_refresh(None));
    }

    #[test]
    fn needs_refresh_future() {
        assert!(!needs_refresh(Some(Utc::now() + Duration::hours(1))));
    }

    #[test]
    fn needs_refresh_within_margin() {
        assert!(needs_refresh(Some(Utc::now() + Duration::seconds(30))));
    }

    #[test]
    fn needs_refresh_past() {
        assert!(needs_refresh(Some(Utc::now() - Duration::hours(1))));
    }

    const SHORT_WAIT: std::time::Duration = std::time::Duration::from_millis(300);

    async fn insert_credential(pool: &PgPool, id: &str, status: &str, expires_at: DateTime<Utc>) {
        sqlx::query(
            r#"INSERT INTO credentials (id, kind, key, name, status, secrets_ciphertext, expires_at)
               VALUES ($1, 'source', 'strava', 'nick@example.com', $2, 'ct-read', $3)"#,
        )
        .bind(id)
        .bind(status)
        .bind(expires_at)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn status_and_ciphertext(pool: &PgPool, id: &str) -> (String, String) {
        sqlx::query_as("SELECT status, secrets_ciphertext FROM credentials WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    /// Wait until a session of this test's database is queued on an advisory
    /// lock, so the test knows the refresh it started has passed its first
    /// read and is waiting.
    async fn wait_for_a_lock_waiter(pool: &PgPool) {
        for _ in 0..200 {
            let waiting: i64 = sqlx::query_scalar(
                r#"SELECT count(*) FROM pg_locks
                    WHERE locktype = 'advisory' AND NOT granted
                      AND database = (SELECT oid FROM pg_database WHERE datname = current_database())"#,
            )
            .fetch_one(pool)
            .await
            .unwrap();
            if waiting > 0 {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        panic!("the refresh never queued on the advisory lock");
    }

    #[sqlx::test(migrations = "../../virtues-core/migrations")]
    async fn a_second_connection_waits_for_the_credential(pool: PgPool) {
        let held = CredentialLock::acquire(&pool, "cred_a", SHORT_WAIT)
            .await
            .unwrap();

        match CredentialLock::acquire(&pool, "cred_a", SHORT_WAIT).await {
            Err(AuthError::Conflict(_)) => {}
            Err(e) => panic!("expected a Conflict, got {e:?}"),
            Ok(_) => panic!("a second connection took a held credential"),
        }

        // Another credential is not held up.
        CredentialLock::acquire(&pool, "cred_b", SHORT_WAIT)
            .await
            .unwrap()
            .release()
            .await;

        held.release().await;
        CredentialLock::acquire(&pool, "cred_a", SHORT_WAIT)
            .await
            .unwrap()
            .release()
            .await;
    }

    #[sqlx::test(migrations = "../../virtues-core/migrations")]
    async fn dropping_the_lock_releases_it(pool: PgPool) {
        // A cancelled dispatch drops the lock without calling release.
        drop(
            CredentialLock::acquire(&pool, "cred_a", SHORT_WAIT)
                .await
                .unwrap(),
        );
        CredentialLock::acquire(&pool, "cred_a", std::time::Duration::from_secs(10))
            .await
            .unwrap()
            .release()
            .await;
    }

    #[sqlx::test(migrations = "../../virtues-core/migrations")]
    async fn the_refresh_that_waited_rereads_and_finds_it_fresh(pool: PgPool) {
        insert_credential(
            &pool,
            "cred_reread",
            "active",
            Utc::now() - Duration::minutes(5),
        )
        .await;

        // The other process is mid-refresh.
        let other = CredentialLock::acquire(&pool, "cred_reread", SHORT_WAIT)
            .await
            .unwrap();
        let waiting = tokio::spawn({
            let pool = pool.clone();
            async move { ensure_fresh(&pool, "cred_reread").await }
        });
        wait_for_a_lock_waiter(&pool).await;

        // It writes its tokens and lets go.
        sqlx::query(
            "UPDATE credentials SET secrets_ciphertext = 'ct-other', expires_at = now() + interval '1 hour' WHERE id = 'cred_reread'",
        )
        .execute(&pool)
        .await
        .unwrap();
        other.release().await;

        // No proxy call and no decrypt: this would fail without a proxy or a
        // key if it tried to refresh again.
        assert_eq!(waiting.await.unwrap().unwrap(), RefreshOutcome::Fresh);
        assert_eq!(
            status_and_ciphertext(&pool, "cred_reread").await,
            ("active".into(), "ct-other".into())
        );
    }

    #[sqlx::test(migrations = "../../virtues-core/migrations")]
    async fn a_source_disconnected_during_the_wait_is_not_refreshed(pool: PgPool) {
        insert_credential(
            &pool,
            "cred_disconnected",
            "active",
            Utc::now() - Duration::minutes(5),
        )
        .await;

        let other = CredentialLock::acquire(&pool, "cred_disconnected", SHORT_WAIT)
            .await
            .unwrap();
        let waiting = tokio::spawn({
            let pool = pool.clone();
            async move { ensure_fresh(&pool, "cred_disconnected").await }
        });
        wait_for_a_lock_waiter(&pool).await;

        sqlx::query("UPDATE credentials SET status = 'revoked' WHERE id = 'cred_disconnected'")
            .execute(&pool)
            .await
            .unwrap();
        other.release().await;

        let outcome = waiting.await.unwrap();
        assert!(
            matches!(outcome, Err(AuthError::Conflict(_))),
            "{outcome:?}"
        );
        assert_eq!(
            status_and_ciphertext(&pool, "cred_disconnected").await.0,
            "revoked"
        );
    }

    #[sqlx::test(migrations = "../../virtues-core/migrations")]
    async fn refreshed_tokens_are_written_only_over_what_was_read(pool: PgPool) {
        insert_credential(&pool, "cred_a", "active", Utc::now() - Duration::minutes(5)).await;

        // Someone else wrote after this refresh read the row.
        assert!(
            !store_refreshed(&pool, "cred_a", "ct-stale", "ct-new", Some(3600))
                .await
                .unwrap()
        );
        assert_eq!(status_and_ciphertext(&pool, "cred_a").await.1, "ct-read");

        assert!(
            store_refreshed(&pool, "cred_a", "ct-read", "ct-new", Some(3600))
                .await
                .unwrap()
        );
        assert_eq!(
            status_and_ciphertext(&pool, "cred_a").await,
            ("active".into(), "ct-new".into())
        );
        let (expires_at, next_refresh_at, used_just_now): (DateTime<Utc>, DateTime<Utc>, bool) =
            sqlx::query_as(
                "SELECT expires_at, next_refresh_at, last_seen_at > now() - interval '1 minute' \
                   FROM credentials WHERE id = 'cred_a'",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(!needs_refresh(Some(expires_at)));
        assert_eq!(
            expires_at - next_refresh_at,
            Duration::seconds(REFRESH_MARGIN_SECS)
        );
        assert!(used_just_now, "a stored refresh counts as a use");
    }

    #[sqlx::test(migrations = "../../virtues-core/migrations")]
    async fn a_revoked_row_stays_revoked(pool: PgPool) {
        insert_credential(
            &pool,
            "cred_a",
            "revoked",
            Utc::now() - Duration::minutes(5),
        )
        .await;

        assert!(
            !store_refreshed(&pool, "cred_a", "ct-read", "ct-new", Some(3600))
                .await
                .unwrap()
        );
        assert!(!mark_refused(&pool, "cred_a", "ct-read").await.unwrap());
        assert_eq!(
            status_and_ciphertext(&pool, "cred_a").await,
            ("revoked".into(), "ct-read".into())
        );
    }

    #[sqlx::test(migrations = "../../virtues-core/migrations")]
    async fn a_refused_grant_marks_only_the_tokens_it_refused(pool: PgPool) {
        insert_credential(&pool, "cred_a", "active", Utc::now() - Duration::minutes(5)).await;

        // The refusal was of tokens another refresh has since replaced.
        assert!(!mark_refused(&pool, "cred_a", "ct-stale").await.unwrap());
        assert_eq!(status_and_ciphertext(&pool, "cred_a").await.0, "active");

        assert!(mark_refused(&pool, "cred_a", "ct-read").await.unwrap());
        let (status, reason): (String, Option<String>) =
            sqlx::query_as("SELECT status, status_reason FROM credentials WHERE id = 'cred_a'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status, "reauth_required");
        assert_eq!(reason.as_deref(), Some("token_rejected_by_provider"));
    }
}

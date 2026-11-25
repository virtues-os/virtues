//! Grants the box has let go of and the OAuth proxy has not released yet.
//!
//! Some providers bill for a connection while it exists (the catalog's
//! `release_on_lapse`; Plaid's linked banks today). Ending one needs its
//! token, and disconnecting empties the credential that holds it. So the
//! token moves here, sealed as a box secret, until the proxy confirms the
//! provider released it. Nothing reads it but the release.
//!
//! The proxy can be unreachable, and it refuses a bank recorded for another
//! Virtues account: a server that moved to a new account releases its old
//! banks only after it is linked to that account again. The daily job
//! (`credentials::lapse`) tries every one again.
//!
//! Other sources' tokens are not kept. Disconnecting one erases its token at
//! once, because the proxy holds nothing to release for them.

use std::future::Future;

use chrono::Utc;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::error::{Error, Result};
use crate::secrets::{self, Kind};

const PREFIX: &str = "pending_release:";

/// Whether a source's grant has to be released at the proxy.
pub fn needs_release(source_id: &str) -> bool {
    crate::applet_templates::lookup_source(source_id).is_some_and(|s| s.release_on_lapse)
}

/// Keep a grant until the proxy releases it. Called before the credential is
/// emptied, so a stop in between loses nothing. Returns the name it is kept
/// under.
///
/// The name carries a hash of the grant: one credential id can hold a second
/// bank after a reconnect, and both must be released.
pub async fn keep(db: &PgPool, credential_id: &str, source_id: &str, grant: &Value) -> Result<String> {
    let value = grant.to_string();
    let hash = hex::encode(&Sha256::digest(value.as_bytes())[..6]);
    let key = format!("{PREFIX}{credential_id}:{hash}");
    let meta = json!({ "source_id": source_id, "credential_id": credential_id, "kept_at": Utc::now().to_rfc3339() });
    secrets::put(db, Kind::Box, &key, &value, &meta).await.map_err(other)?;
    Ok(key)
}

/// Forget a grant that does not have to be released after all.
pub async fn forget(db: &PgPool, key: &str) -> Result<()> {
    secrets::delete(db, Kind::Box, key).await.map_err(other)
}

/// Ask the proxy to release one kept grant, and forget it once the proxy
/// confirms.
pub async fn release(db: &PgPool, key: &str) -> Result<()> {
    release_with(db, key, through_proxy).await
}

/// Try every kept grant again. One that fails is kept for the next day.
pub async fn release_all(db: &PgPool) -> Result<()> {
    release_all_with(db, through_proxy).await
}

async fn through_proxy(source_id: String, grant: Value, api_key: String) -> std::result::Result<(), String> {
    virtues_helpers::auth::proxy_revoke(&source_id, &grant, Some(&api_key))
        .await
        .map_err(|e| e.to_string())
}

async fn release_all_with<F, Fut>(db: &PgPool, ask: F) -> Result<()>
where
    F: Fn(String, Value, String) -> Fut,
    Fut: Future<Output = std::result::Result<(), String>>,
{
    let kept = secrets::list(db, Kind::Box).await.map_err(other)?;
    for named in kept.into_iter().filter(|n| n.key.starts_with(PREFIX)) {
        if let Err(e) = release_with(db, &named.key, &ask).await {
            tracing::warn!(key = %named.key, error = %e, "a kept grant could not be released");
        }
    }
    Ok(())
}

async fn release_with<F, Fut>(db: &PgPool, key: &str, ask: F) -> Result<()>
where
    F: Fn(String, Value, String) -> Fut,
    Fut: Future<Output = std::result::Result<(), String>>,
{
    let Some((value, meta)) = secrets::get(db, Kind::Box, key).await.map_err(other)? else {
        return Ok(());
    };
    let grant: Value = serde_json::from_str(&value).map_err(|e| Error::Other(format!("read a kept grant: {e}")))?;
    let (Some(source_id), Some(credential_id)) = (meta["source_id"].as_str(), meta["credential_id"].as_str()) else {
        return Err(Error::Other("a kept grant names no source".into()));
    };
    // The source is still connected with this grant: a disconnect is between
    // keeping it and emptying the credential, or stopped there. It is not
    // released, and it stays kept, since forgetting it now would lose the
    // token once the disconnect finishes. The next try checks again.
    if in_use(db, credential_id, &grant).await? {
        return Ok(());
    }
    // The proxy releases only for a box key it issued. An unlinked box keeps
    // the grant until it is linked.
    let Some(api_key) = virtues_helpers::auth::read_box_api_key(db)
        .await
        .map_err(|e| Error::Other(format!("read the box key: {e}")))?
    else {
        return Ok(());
    };
    match ask(source_id.to_string(), grant, api_key).await {
        Ok(()) => {
            tracing::info!(source_id, credential_id, "the OAuth proxy released a grant the box let go of");
            forget(db, key).await
        }
        Err(e) => {
            tracing::warn!(source_id, credential_id, error = %e, "the OAuth proxy did not release a grant; kept for another try");
            Ok(())
        }
    }
}

/// Whether an active credential still holds this grant.
async fn in_use(db: &PgPool, credential_id: &str, grant: &Value) -> Result<bool> {
    let sealed: Option<String> = sqlx::query_scalar(
        "SELECT secrets_ciphertext FROM credentials WHERE id = $1 AND kind = 'source' AND status = 'active'",
    )
    .bind(credential_id)
    .fetch_optional(db)
    .await?;
    let Some(sealed) = sealed.filter(|s| !s.is_empty()) else {
        return Ok(false);
    };
    let held = crate::crypto::TokenEncryptor::from_env()
        .and_then(|enc| enc.decrypt(&sealed))
        .map_err(|e| Error::Other(format!("read an active credential: {e:#}")))?;
    Ok(serde_json::from_str::<Value>(&held).is_ok_and(|h| h == *grant))
}

fn other(e: anyhow::Error) -> Error {
    Error::Other(format!("kept grant: {e:#}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::ensure_test_key;
    use std::sync::{Arc, Mutex};

    type Asked = Arc<Mutex<Vec<(String, Value, String)>>>;

    /// A stand-in proxy that answers `answer` and notes what it was asked.
    fn proxy(answer: std::result::Result<(), String>) -> (Asked, impl Fn(String, Value, String) -> std::future::Ready<std::result::Result<(), String>>) {
        let asked: Asked = Arc::default();
        let seen = asked.clone();
        (asked, move |source, grant, key| {
            seen.lock().unwrap().push((source, grant, key));
            std::future::ready(answer.clone())
        })
    }

    fn bank(token: &str) -> Value {
        json!({ "access_token": token })
    }

    async fn kept(db: &PgPool) -> usize {
        secrets::list(db, Kind::Box).await.unwrap().iter().filter(|n| n.key.starts_with(PREFIX)).count()
    }

    async fn connect(db: &PgPool, id: &str, grant: &Value) {
        let sealed = crate::crypto::TokenEncryptor::from_env().unwrap().encrypt(&grant.to_string()).unwrap();
        sqlx::query(
            "INSERT INTO credentials (id, key, name, status, secrets_ciphertext) VALUES ($1, 'plaid', 'Example Bank', 'active', $2) \
             ON CONFLICT (id) DO UPDATE SET status = 'active', secrets_ciphertext = EXCLUDED.secrets_ciphertext",
        )
        .bind(id)
        .bind(&sealed)
        .execute(db)
        .await
        .unwrap();
    }

    async fn link_box(db: &PgPool) {
        crate::virtues_api::renew::store_api_key(db, "vk-example").await.unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_released_grant_is_forgotten(pool: PgPool) {
        ensure_test_key();
        link_box(&pool).await;
        keep(&pool, "cred_bank", "plaid", &bank("at-example")).await.unwrap();
        let (asked, ask) = proxy(Ok(()));
        release_all_with(&pool, ask).await.unwrap();
        assert_eq!(*asked.lock().unwrap(), [("plaid".to_string(), bank("at-example"), "vk-example".to_string())]);
        assert_eq!(kept(&pool).await, 0);
    }

    /// Refused or unreachable, the grant is kept and asked for again.
    #[sqlx::test(migrations = "./migrations")]
    async fn a_refused_grant_is_kept_and_tried_again(pool: PgPool) {
        ensure_test_key();
        link_box(&pool).await;
        let key = keep(&pool, "cred_bank", "plaid", &bank("at-example")).await.unwrap();
        let (_, refuse) = proxy(Err("upstream 403: bank_not_yours".into()));
        release_with(&pool, &key, &refuse).await.unwrap();
        release_all_with(&pool, &refuse).await.unwrap();
        assert_eq!(kept(&pool).await, 1);
        let (_, accept) = proxy(Ok(()));
        release_all_with(&pool, accept).await.unwrap();
        assert_eq!(kept(&pool).await, 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn an_unlinked_box_keeps_the_grant_and_asks_nobody(pool: PgPool) {
        ensure_test_key();
        keep(&pool, "cred_bank", "plaid", &bank("at-example")).await.unwrap();
        let (asked, ask) = proxy(Ok(()));
        release_all_with(&pool, ask).await.unwrap();
        assert!(asked.lock().unwrap().is_empty());
        assert_eq!(kept(&pool).await, 1);
    }

    /// A daily try that lands between keeping the grant and emptying the
    /// credential neither ends the still-connected bank nor drops the kept
    /// token, which the disconnect then releases.
    #[sqlx::test(migrations = "./migrations")]
    async fn a_grant_still_in_use_is_kept_and_not_released(pool: PgPool) {
        ensure_test_key();
        link_box(&pool).await;
        connect(&pool, "cred_bank", &bank("at-example")).await;
        let key = keep(&pool, "cred_bank", "plaid", &bank("at-example")).await.unwrap();
        let (asked, ask) = proxy(Ok(()));
        release_all_with(&pool, &ask).await.unwrap();
        assert!(asked.lock().unwrap().is_empty());
        assert_eq!(kept(&pool).await, 1);
        // The disconnect finishes: the credential is emptied, then released.
        sqlx::query("UPDATE credentials SET status = 'revoked' WHERE id = 'cred_bank'").execute(&pool).await.unwrap();
        release_with(&pool, &key, &ask).await.unwrap();
        assert_eq!(asked.lock().unwrap().len(), 1);
        assert_eq!(kept(&pool).await, 0);
    }

    /// Disconnect, connect another bank under the same credential, disconnect
    /// again: both old banks are released, and a connected one is not.
    #[sqlx::test(migrations = "./migrations")]
    async fn each_bank_under_one_credential_is_released(pool: PgPool) {
        ensure_test_key();
        link_box(&pool).await;
        keep(&pool, "cred_bank", "plaid", &bank("at-first")).await.unwrap();
        keep(&pool, "cred_bank", "plaid", &bank("at-second")).await.unwrap();
        connect(&pool, "cred_bank", &bank("at-third")).await;
        let (asked, ask) = proxy(Ok(()));
        release_all_with(&pool, ask).await.unwrap();
        let mut tokens: Vec<String> =
            asked.lock().unwrap().iter().map(|a| a.1["access_token"].as_str().unwrap().to_string()).collect();
        tokens.sort();
        assert_eq!(tokens, ["at-first", "at-second"]);
        assert_eq!(kept(&pool).await, 0);
    }

    #[test]
    fn only_billed_sources_are_kept() {
        assert!(needs_release("plaid"));
        assert!(!needs_release("google") && !needs_release("notion") && !needs_release("no-such-source"));
    }
}

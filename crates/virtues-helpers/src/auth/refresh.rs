//! Just-in-time credential refresh.
//!
//! `ensure_fresh` is called from `applet_runner::load_credentials` immediately
//! before dispatching a subprocess. If the credential's access token has
//! expired (or is within the 60s safety margin), it refreshes inline via the
//! OAuth proxy and writes the new secrets back. The subprocess always sees a
//! valid token; extractors never need to handle 401 / refresh themselves.
//!
//! ## Concurrency
//!
//! Multiple actions can dispatch for the same credential simultaneously. A
//! per-credential async mutex serializes refresh attempts so we never hit the
//! provider's token endpoint twice in parallel for the same credential.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use sqlx::PgPool;
use tokio::sync::Mutex;

use crate::auth::error::{AuthError, Result};
use crate::auth::proxy::proxy_refresh;
use crate::auth::vault::{mark_credential_status, update_credential_secrets, CredentialStatus};
use crate::crypto::TokenEncryptor;

/// Safety margin: refresh when access token has this much (or less) life left.
const REFRESH_MARGIN_SECS: i64 = 60;

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

#[derive(Debug, sqlx::FromRow)]
struct CredRow {
    source_id: String,
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
    let row: Option<CredRow> = sqlx::query_as(
        r#"SELECT source_id, status, expires_at, secrets_ciphertext
             FROM credentials WHERE id = $1"#,
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

    if !needs_refresh(row.expires_at) {
        return Ok(if row.expires_at.is_some() {
            RefreshOutcome::Fresh
        } else {
            RefreshOutcome::NoRefreshable
        });
    }

    let lock = lock_for(credential_id).await;
    let _guard = lock.lock().await;

    // Re-read under the lock — another task may have already refreshed.
    let row: CredRow = sqlx::query_as(
        r#"SELECT source_id, status, expires_at, secrets_ciphertext
             FROM credentials WHERE id = $1"#,
    )
    .bind(credential_id)
    .fetch_one(db)
    .await?;

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
    match proxy_refresh(&row.source_id, &refresh_token, api_key.as_deref()).await {
        Ok(resp) => {
            update_credential_secrets(db, credential_id, &resp.secrets, resp.expires_in).await?;
            tracing::info!(
                credential_id,
                source_id = %row.source_id,
                expires_in = resp.expires_in,
                "credential refreshed just-in-time"
            );
            Ok(RefreshOutcome::Refreshed)
        }
        Err(AuthError::Proxy(msg)) if msg.contains("upstream 4") => {
            tracing::warn!(
                credential_id,
                source_id = %row.source_id,
                err = %msg,
                "refresh rejected by provider; marking reauth_required"
            );
            let _ = mark_credential_status(
                db,
                credential_id,
                CredentialStatus::ReauthRequired,
                Some("token_rejected_by_provider"),
            )
            .await;
            Err(AuthError::Proxy(msg))
        }
        Err(e) => Err(e),
    }
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
}

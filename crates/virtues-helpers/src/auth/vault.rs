//! Vault writes — credential row mint, finalize, status transitions, fan-out lookup.
//!
//! Encryption flows through `crate::crypto`.
//!
//! All writes target the `kind = 'source'` rows of the `credentials` table,
//! one per connected source, with the source id in `key`. The box's own keys
//! are the `kind = 'box'` rows, handled by core's `secrets` module.
//! `secrets_ciphertext` holds AES-256-GCM JSON; `metadata` is JSONB non-secret
//! context.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::error::{AuthError, Result};
use crate::crypto::TokenEncryptor;

/// Credential lifecycle state. Mirrors `credentials.status` in the schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialStatus {
    Pending,
    Active,
    Revoked,
    ReauthRequired,
    Error,
}

impl CredentialStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Revoked => "revoked",
            Self::ReauthRequired => "reauth_required",
            Self::Error => "error",
        }
    }
}

/// Mint a pending credential row. The row is `status='pending'`, with empty
/// encrypted secrets (`{}`). Caller flips to `active` via finalize_*.
pub async fn mint_pending_credential(
    db: &PgPool,
    source_id: &str,
    name: &str,
) -> Result<String> {
    if name.trim().is_empty() {
        return Err(AuthError::InvalidInput("name cannot be empty".into()));
    }

    let id = format!("cred_{}", Uuid::new_v4());
    let encryptor = TokenEncryptor::from_env()?;
    let empty_secrets = encryptor.encrypt("{}")?;

    sqlx::query(
        r#"INSERT INTO credentials
              (id, key, name, status, secrets_ciphertext, metadata)
           VALUES ($1, $2, $3, 'pending', $4, '{}'::jsonb)"#,
    )
    .bind(&id)
    .bind(source_id)
    .bind(name)
    .bind(&empty_secrets)
    .execute(db)
    .await?;

    Ok(id)
}

/// `expires_at` and `next_refresh_at` for a grant that lasts `expires_in`
/// seconds. The refresh is due a minute before the grant ends.
fn expiry_from(expires_in: Option<i64>) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
    match expires_in {
        Some(secs) => {
            let exp = Utc::now() + Duration::seconds(secs);
            (Some(exp), Some(exp - Duration::seconds(60)))
        }
        None => (None, None),
    }
}

/// Finalize a fresh `via_proxy` credential: encrypt secrets, store metadata
/// and expiry, and move it from `pending` to `active`. A second callback for
/// the same row is a no-op once it is active. A reconnect goes through
/// [`reauth_credential`] instead, because this no-op would drop its new
/// tokens.
pub async fn finalize_credential(
    db: &PgPool,
    credential_id: &str,
    secrets: &serde_json::Value,
    metadata: &serde_json::Value,
    expires_in: Option<i64>,
) -> Result<()> {
    let encryptor = TokenEncryptor::from_env()?;
    let secrets_str = serde_json::to_string(secrets)?;
    let secrets_ct = encryptor.encrypt(&secrets_str)?;
    let metadata_json = metadata.clone();

    let (expires_at, next_refresh_at) = expiry_from(expires_in);

    let result = sqlx::query(
        r#"UPDATE credentials
              SET status = 'active',
                  status_reason = NULL,
                  secrets_ciphertext = $1,
                  metadata = $2,
                  expires_at = $3,
                  next_refresh_at = $4,
                  last_seen_at = now()
            WHERE id = $5 AND kind = 'source' AND status = 'pending'"#,
    )
    .bind(&secrets_ct)
    .bind(&metadata_json)
    .bind(expires_at)
    .bind(next_refresh_at)
    .bind(credential_id)
    .execute(db)
    .await?;

    if result.rows_affected() == 0 {
        let current: Option<(String,)> =
            sqlx::query_as("SELECT status FROM credentials WHERE id = $1 AND kind = 'source'")
                .bind(credential_id)
                .fetch_optional(db)
                .await?;
        match current {
            None => return Err(AuthError::NotFound(credential_id.to_string())),
            Some((status,)) if status == "active" => {
                tracing::info!(
                    credential_id,
                    "finalize_credential: row already active (double-callback dedup)"
                );
                return Ok(());
            }
            Some((status,)) => {
                return Err(AuthError::Conflict(format!(
                    "cannot finalize credential in status '{status}'"
                )))
            }
        }
    }

    Ok(())
}

/// Repair a source credential in place when the owner reconnects it, through
/// OAuth or by pasting new fields: seal the new secrets, reset its expiry, and
/// set it `active` again. `Some(metadata)` replaces the row's metadata, as a
/// new OAuth grant does; `None` keeps it. The row keeps its id, name and
/// applets, so its syncs carry on from their cursors.
///
/// Returns `false` when there is nothing to repair: the row is gone, belongs
/// to another source, or was disconnected. A disconnected (`revoked`) row
/// stays revoked, and connecting the source again adds a new sign-in.
pub async fn reauth_credential(
    db: &PgPool,
    credential_id: &str,
    source_id: &str,
    secrets: &serde_json::Value,
    metadata: Option<&serde_json::Value>,
    expires_in: Option<i64>,
) -> Result<bool> {
    let encryptor = TokenEncryptor::from_env()?;
    let secrets_ct = encryptor.encrypt(&serde_json::to_string(secrets)?)?;
    let (expires_at, next_refresh_at) = expiry_from(expires_in);

    let result = sqlx::query(
        r#"UPDATE credentials
              SET status = 'active',
                  status_reason = NULL,
                  secrets_ciphertext = $1,
                  metadata = COALESCE($2, metadata),
                  expires_at = $3,
                  next_refresh_at = $4,
                  last_seen_at = now(),
                  updated_at = now()
            WHERE id = $5 AND kind = 'source' AND key = $6 AND status <> 'revoked'"#,
    )
    .bind(&secrets_ct)
    .bind(metadata)
    .bind(expires_at)
    .bind(next_refresh_at)
    .bind(credential_id)
    .bind(source_id)
    .execute(db)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Mint + finalize in one call for `api_key` flows. The user pasted a token;
/// nothing to dedupe via pending state. Connecting counts as a use, as it
/// does for an OAuth grant, so the row starts with `last_seen_at` set.
pub async fn finalize_apikey_credential(
    db: &PgPool,
    source_id: &str,
    name: &str,
    fields: &serde_json::Value,
) -> Result<String> {
    if name.trim().is_empty() {
        return Err(AuthError::InvalidInput("name cannot be empty".into()));
    }

    let id = format!("cred_{}", Uuid::new_v4());
    let encryptor = TokenEncryptor::from_env()?;
    let secrets_str = serde_json::to_string(fields)?;
    let secrets_ct = encryptor.encrypt(&secrets_str)?;

    sqlx::query(
        r#"INSERT INTO credentials
              (id, key, name, status, secrets_ciphertext, metadata, last_seen_at)
           VALUES ($1, $2, $3, 'active', $4, '{}'::jsonb, now())"#,
    )
    .bind(&id)
    .bind(source_id)
    .bind(name)
    .bind(&secrets_ct)
    .execute(db)
    .await?;

    Ok(id)
}

/// Replace a credential's secrets and set it `active`, whatever its status.
/// Relinking the box's own key (`virtues_api::renew`) uses this. The owner
/// cannot disconnect that key, but an older release let a request revoke its
/// row, and a relink must still work on such a box. A source sign-in goes
/// through [`reauth_credential`], which leaves a disconnected row alone.
/// Writing new secrets counts as a use, so it sets `last_seen_at`.
pub async fn update_credential_secrets(
    db: &PgPool,
    credential_id: &str,
    secrets: &serde_json::Value,
    expires_in: Option<i64>,
) -> Result<()> {
    let encryptor = TokenEncryptor::from_env()?;
    let secrets_str = serde_json::to_string(secrets)?;
    let secrets_ct = encryptor.encrypt(&secrets_str)?;

    let (expires_at, next_refresh_at) = expiry_from(expires_in);

    let result = sqlx::query(
        r#"UPDATE credentials
              SET secrets_ciphertext = $1,
                  expires_at = $2,
                  next_refresh_at = $3,
                  status = 'active',
                  status_reason = NULL,
                  last_seen_at = now(),
                  updated_at = now()
            WHERE id = $4 AND kind = 'source'"#,
    )
    .bind(&secrets_ct)
    .bind(expires_at)
    .bind(next_refresh_at)
    .bind(credential_id)
    .execute(db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AuthError::NotFound(credential_id.to_string()));
    }
    Ok(())
}

/// Record when a session credential stops working, from its login cookies.
///
/// Leaves `next_refresh_at` alone: nothing can renew a browser session, so
/// the refresh sweep must not pick it up. `None` clears a date a previous
/// sign-in set. Attention reads `expires_at` to warn the owner beforehand.
pub async fn set_credential_expiry(
    db: &PgPool,
    credential_id: &str,
    expires_at: Option<DateTime<Utc>>,
) -> Result<()> {
    let result = sqlx::query(
        "UPDATE credentials SET expires_at = $1 WHERE id = $2 AND kind = 'source'",
    )
    .bind(expires_at)
    .bind(credential_id)
    .execute(db)
    .await?;
    if result.rows_affected() == 0 {
        return Err(AuthError::NotFound(credential_id.to_string()));
    }
    Ok(())
}

/// Record that a credential was just used. `last_seen_at` means "last used":
/// activating, reconnecting or refreshing a credential sets it, and so does
/// every read through [`read_credential_secrets`]. Code that decrypts
/// `secrets_ciphertext` on its own skips the bump unless it calls this.
/// Throttled to once a minute (the `app_device` pattern) and best effort,
/// because a failed bump must never fail the use; it is logged instead.
pub async fn touch_last_seen(db: &PgPool, credential_id: &str) {
    if let Err(e) = sqlx::query(
        "UPDATE credentials SET last_seen_at = now() \
         WHERE id = $1 AND kind = 'source' AND (last_seen_at IS NULL \
            OR last_seen_at < now() - interval '60 seconds')",
    )
    .bind(credential_id)
    .execute(db)
    .await
    {
        tracing::warn!(credential_id, error = %e, "could not record when a credential was used");
    }
}

/// Decrypt the secrets payload of a credential row.
pub async fn read_credential_secrets(
    db: &PgPool,
    credential_id: &str,
) -> Result<serde_json::Value> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT secrets_ciphertext FROM credentials WHERE id = $1 AND kind = 'source'")
            .bind(credential_id)
            .fetch_optional(db)
            .await?;
    let (ciphertext,) = row.ok_or_else(|| AuthError::NotFound(credential_id.to_string()))?;

    touch_last_seen(db, credential_id).await;

    let encryptor = TokenEncryptor::from_env()?;
    let plaintext = encryptor.decrypt(&ciphertext)?;
    Ok(serde_json::from_str(&plaintext)?)
}

/// The box's own Virtues api_key, if linked — the credential stored under
/// `key = 'virtues_api'` by core's `virtues_api::renew`. Lives here
/// rather than in core because the credential-refresh cron (this crate) needs
/// it to identify itself to the OAuth proxy, and helpers cannot depend on
/// core. It reads the row whatever its status, as `renew::read_api_key` does,
/// so both send the same key. `Ok(None)` is an unlinked box, an ordinary
/// state; `Err` is a vault that could not be read, which is not.
pub async fn read_box_api_key(db: &PgPool) -> Result<Option<String>> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT id FROM credentials WHERE kind = 'source' AND key = 'virtues_api' LIMIT 1")
            .fetch_optional(db)
            .await?;
    let Some((id,)) = row else { return Ok(None) };
    let secrets = read_credential_secrets(db, &id).await?;
    Ok(secrets["api_key"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(String::from))
}

/// Mark a credential's status without touching its secret payload. A
/// `revoked` row is refused with `Conflict`, so a run that fails after the
/// owner disconnected a source cannot put it back under "Needs you".
pub async fn mark_credential_status(
    db: &PgPool,
    credential_id: &str,
    status: CredentialStatus,
    reason: Option<&str>,
) -> Result<()> {
    let result = sqlx::query(
        r#"UPDATE credentials
              SET status = $1,
                  status_reason = $2,
                  updated_at = now()
            WHERE id = $3 AND kind = 'source' AND status <> 'revoked'"#,
    )
    .bind(status.as_str())
    .bind(reason)
    .bind(credential_id)
    .execute(db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(missing_or_revoked(db, credential_id).await);
    }
    Ok(())
}

/// The error for a write that matched no row: `NotFound` when the row does
/// not exist, `Conflict` when the owner disconnected it.
async fn missing_or_revoked(db: &PgPool, credential_id: &str) -> AuthError {
    let status: std::result::Result<Option<String>, sqlx::Error> =
        sqlx::query_scalar("SELECT status FROM credentials WHERE id = $1 AND kind = 'source'")
            .bind(credential_id)
            .fetch_optional(db)
            .await;
    match status {
        Ok(Some(_)) => AuthError::Conflict(format!("credential {credential_id} was disconnected")),
        Ok(None) => AuthError::NotFound(credential_id.to_string()),
        Err(e) => e.into(),
    }
}

/// Return the per-device fan-out map: `command-name → app_applets.id` for every
/// ingest action row anchored to this device. The key is `command[0]` (the
/// action's program name, e.g. `ios_ingest`), which the device uses to route a
/// flush to `POST /webhook/{applet_id}`. All iOS streams share the single
/// `ios_ingest` action and disambiguate via the `stream` field in the body.
pub async fn fanout_applet_ids(
    db: &PgPool,
    device_id: &str,
) -> Result<HashMap<String, String>> {
    let rows: Vec<(Option<String>, String)> = sqlx::query_as(
        r#"SELECT command::jsonb->>0, id FROM app_applets
           WHERE device_id = $1 AND command IS NOT NULL"#,
    )
    .bind(device_id)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|(key, id)| key.map(|k| (k, id)))
        .collect())
}

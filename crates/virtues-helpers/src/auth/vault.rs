//! Vault writes — credential row mint, finalize, status transitions, fan-out lookup.
//!
//! Encryption flows through `crate::crypto`. Schema details live in
//! `core/migrations/055_credentials_create.sql` and `ACTIONS.md` § Schema.
//!
//! All writes target the `credentials` table. The `secrets_ciphertext` column
//! holds AES-256-GCM JSON; `secret_lookup_hash` holds an HMAC of the
//! plaintext (for self-issued-bearer flows only); `metadata` is plaintext
//! non-secret context.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use sqlx::SqlitePool;
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

/// Mint a pending credential row. Used by:
/// - `pair_initiate` core handler (iOS-style flows)
/// - `oauth_start` core handler when `existing_credential_id` is `None`
///
/// The row is `status='pending'`, with empty encrypted secrets (`{}`). The
/// caller is expected to flip it to `active` via `finalize_credential` or
/// `finalize_self_issued_bearer` after the user completes the flow.
pub async fn mint_pending_credential(
    db: &SqlitePool,
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
              (id, source_id, name, status, secrets_ciphertext, metadata)
           VALUES (?, ?, ?, 'pending', ?, '{}')"#,
    )
    .bind(&id)
    .bind(source_id)
    .bind(name)
    .bind(&empty_secrets)
    .execute(db)
    .await?;

    Ok(id)
}

/// Finalize a `via_proxy` credential — encrypts the secrets payload, stores
/// metadata + scopes + expiry, transitions `pending → active`.
///
/// **Idempotency**: only updates rows currently `status = 'pending'`. A
/// second callback for the same `credential_id` no-ops, which dedups
/// double-callbacks (the proxy may retry on network flake).
///
/// `expires_in` is seconds from now; `next_refresh_at` is computed as
/// `now + expires_in - 60s` (safety margin). The `credential_refresh` cron
/// sweeps `WHERE next_refresh_at < now()`.
pub async fn finalize_credential(
    db: &SqlitePool,
    credential_id: &str,
    secrets: &serde_json::Value,
    metadata: &serde_json::Value,
    expires_in: Option<i64>,
    scopes: Option<&[String]>,
) -> Result<()> {
    let encryptor = TokenEncryptor::from_env()?;
    let secrets_str = serde_json::to_string(secrets)?;
    let secrets_ct = encryptor.encrypt(&secrets_str)?;
    let metadata_str = serde_json::to_string(metadata)?;
    let scopes_json = scopes
        .map(|s| serde_json::to_string(s))
        .transpose()
        .map_err(AuthError::Serde)?;

    let (expires_at, next_refresh_at): (Option<String>, Option<String>) = match expires_in {
        Some(secs) => {
            let exp: DateTime<Utc> = Utc::now() + Duration::seconds(secs);
            let refresh: DateTime<Utc> = exp - Duration::seconds(60);
            (Some(exp.to_rfc3339()), Some(refresh.to_rfc3339()))
        }
        None => (None, None),
    };

    let result = sqlx::query(
        r#"UPDATE credentials
              SET status = 'active',
                  status_reason = NULL,
                  secrets_ciphertext = ?,
                  metadata = ?,
                  scopes = ?,
                  expires_at = ?,
                  next_refresh_at = ?,
                  last_seen_at = datetime('now')
            WHERE id = ? AND status = 'pending'"#,
    )
    .bind(&secrets_ct)
    .bind(&metadata_str)
    .bind(&scopes_json)
    .bind(&expires_at)
    .bind(&next_refresh_at)
    .bind(credential_id)
    .execute(db)
    .await?;

    if result.rows_affected() == 0 {
        let current: Option<(String,)> =
            sqlx::query_as("SELECT status FROM credentials WHERE id = ?")
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

/// Atomic finalize for `self_issued_bearer` flows: encrypts the bearer token
/// as `{"token": "..."}`, computes the HMAC lookup hash for O(1) webhook
/// authentication, stores metadata, and flips status to `active`.
///
/// Same idempotency semantics as `finalize_credential`.
pub async fn finalize_self_issued_bearer(
    db: &SqlitePool,
    credential_id: &str,
    plaintext_token: &str,
    metadata: &serde_json::Value,
) -> Result<()> {
    if plaintext_token.trim().is_empty() {
        return Err(AuthError::InvalidInput("token cannot be empty".into()));
    }

    let encryptor = TokenEncryptor::from_env()?;
    let secrets_payload = json!({ "token": plaintext_token }).to_string();
    let secrets_ct = encryptor.encrypt(&secrets_payload)?;
    let lookup_hash = encryptor.lookup_hash(plaintext_token)?;
    let metadata_str = serde_json::to_string(metadata)?;

    let result = sqlx::query(
        r#"UPDATE credentials
              SET status = 'active',
                  status_reason = NULL,
                  secrets_ciphertext = ?,
                  secret_lookup_hash = ?,
                  metadata = ?,
                  last_seen_at = datetime('now')
            WHERE id = ? AND status = 'pending'"#,
    )
    .bind(&secrets_ct)
    .bind(&lookup_hash)
    .bind(&metadata_str)
    .bind(credential_id)
    .execute(db)
    .await?;

    if result.rows_affected() == 0 {
        let current: Option<(String,)> =
            sqlx::query_as("SELECT status FROM credentials WHERE id = ?")
                .bind(credential_id)
                .fetch_optional(db)
                .await?;
        match current {
            None => return Err(AuthError::NotFound(credential_id.to_string())),
            Some((status,)) if status == "active" => {
                tracing::info!(credential_id, "finalize_self_issued_bearer: already active");
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

/// Mint + finalize in one call for `api_key` flows. The user pasted a token;
/// nothing to dedupe via pending state.
///
/// `fields` is the JSON object the form collected (`{"token": "..."}` for
/// single-field connectors, `{"key1": "...", "key2": "..."}` for multi).
/// Stored as the encrypted secrets payload verbatim.
pub async fn finalize_apikey_credential(
    db: &SqlitePool,
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
              (id, source_id, name, status, secrets_ciphertext, metadata)
           VALUES (?, ?, ?, 'active', ?, '{}')"#,
    )
    .bind(&id)
    .bind(source_id)
    .bind(name)
    .bind(&secrets_ct)
    .execute(db)
    .await?;

    Ok(id)
}

/// Update an active credential's secrets after a successful refresh. Used by
/// the `credential_refresh` cron action. Re-encrypts the new secrets payload
/// and recomputes `next_refresh_at` from `expires_in` (60s safety margin).
///
/// Unlike `finalize_credential`, this targets rows already in `status='active'` —
/// it's the post-handshake refresh path, not the initial connect path.
/// `metadata` and `scopes` are preserved unless the proxy returns new values.
pub async fn update_credential_secrets(
    db: &SqlitePool,
    credential_id: &str,
    secrets: &serde_json::Value,
    expires_in: Option<i64>,
) -> Result<()> {
    let encryptor = TokenEncryptor::from_env()?;
    let secrets_str = serde_json::to_string(secrets)?;
    let secrets_ct = encryptor.encrypt(&secrets_str)?;

    let (expires_at, next_refresh_at): (Option<String>, Option<String>) = match expires_in {
        Some(secs) => {
            let exp: DateTime<Utc> = Utc::now() + Duration::seconds(secs);
            let refresh: DateTime<Utc> = exp - Duration::seconds(60);
            (Some(exp.to_rfc3339()), Some(refresh.to_rfc3339()))
        }
        None => (None, None),
    };

    let result = sqlx::query(
        r#"UPDATE credentials
              SET secrets_ciphertext = ?,
                  expires_at = ?,
                  next_refresh_at = ?,
                  status = 'active',
                  status_reason = NULL,
                  updated_at = datetime('now')
            WHERE id = ?"#,
    )
    .bind(&secrets_ct)
    .bind(&expires_at)
    .bind(&next_refresh_at)
    .bind(credential_id)
    .execute(db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AuthError::NotFound(credential_id.to_string()));
    }
    Ok(())
}

/// Decrypt the secrets payload of a credential row. Used by `credential_refresh`
/// to extract the `refresh_token` before calling `proxy_refresh`. Returns the
/// decoded JSON value.
pub async fn read_credential_secrets(
    db: &SqlitePool,
    credential_id: &str,
) -> Result<serde_json::Value> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT secrets_ciphertext FROM credentials WHERE id = ?")
            .bind(credential_id)
            .fetch_optional(db)
            .await?;
    let (ciphertext,) = row.ok_or_else(|| AuthError::NotFound(credential_id.to_string()))?;
    let encryptor = TokenEncryptor::from_env()?;
    let plaintext = encryptor.decrypt(&ciphertext)?;
    Ok(serde_json::from_str(&plaintext)?)
}

/// Mark a credential's status without touching its secret payload. Used for
/// `revoked` (user clicked Reconnect / Disconnect), `reauth_required`
/// (proxy webhook signaled provider-side invalidation), and `error`
/// (transient provider failures).
pub async fn mark_credential_status(
    db: &SqlitePool,
    credential_id: &str,
    status: CredentialStatus,
    reason: Option<&str>,
) -> Result<()> {
    let result = sqlx::query(
        r#"UPDATE credentials
              SET status = ?,
                  status_reason = ?,
                  updated_at = datetime('now')
            WHERE id = ?"#,
    )
    .bind(status.as_str())
    .bind(reason)
    .bind(credential_id)
    .execute(db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AuthError::NotFound(credential_id.to_string()));
    }
    Ok(())
}

/// Return the per-credential fan-out map: `function_name → app_actions.id`
/// for every action row keyed to this credential.
///
/// Used by `pair_complete` to send the iOS app its routing table:
/// `{"ios_healthkit": "<action_id>", "ios_location": "<action_id>", ...}`.
/// The device stores this alongside its `device_token` and routes each
/// stream flush to `POST /webhook/{action_id}`.
///
/// Generic over source kind — drops the legacy `LIKE 'ios_%'` filter.
/// Any per-credential fan-out (custom IoT, future Mac, etc.) returns its
/// full action set here.
pub async fn fanout_action_ids(
    db: &SqlitePool,
    credential_id: &str,
) -> Result<HashMap<String, String>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        r#"SELECT function_name, id FROM app_actions
           WHERE credential_id = ? AND function_name IS NOT NULL"#,
    )
    .bind(credential_id)
    .fetch_all(db)
    .await?;

    Ok(rows.into_iter().collect())
}

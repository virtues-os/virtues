//! Vault writes — credential row mint, finalize, status transitions, fan-out lookup.
//!
//! Encryption flows through `crate::crypto`. Schema details live in
//! `virtues-core/migrations/0004_credentials_and_actions.sql`.
//!
//! All writes target the `credentials` table. `secrets_ciphertext` holds
//! AES-256-GCM JSON; `secret_lookup_hash` holds an HMAC of the plaintext
//! (self-issued-bearer flows only); `metadata` is JSONB non-secret context.

use std::collections::HashMap;

use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::error::{AuthError, Result};
use crate::crypto::TokenEncryptor;

/// Mint a fresh random 32-byte bearer, base64url-encoded (no padding). The
/// *server* issues the device's bearer at pairing so no stable device
/// identifier (e.g. a UUID) is ever used as a credential — the no-stable-bearer
/// rule. RNG failure is catastrophic and treated as infallible, matching
/// `crypto::OauthStateClaims::new`.
pub fn generate_bearer() -> String {
    let mut bytes = [0u8; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .expect("SystemRandom should always produce bytes");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

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
              (id, source_id, name, status, secrets_ciphertext, metadata)
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

/// Finalize a `via_proxy` credential — encrypts secrets, stores metadata +
/// scopes + expiry, transitions `pending → active`. Idempotent on second
/// callback (no-ops if already active).
pub async fn finalize_credential(
    db: &PgPool,
    credential_id: &str,
    secrets: &serde_json::Value,
    metadata: &serde_json::Value,
    expires_in: Option<i64>,
    scopes: Option<&[String]>,
) -> Result<()> {
    let encryptor = TokenEncryptor::from_env()?;
    let secrets_str = serde_json::to_string(secrets)?;
    let secrets_ct = encryptor.encrypt(&secrets_str)?;
    let metadata_json = metadata.clone();
    let scopes_json = scopes.map(|s| serde_json::to_value(s)).transpose()?;

    let (expires_at, next_refresh_at): (Option<DateTime<Utc>>, Option<DateTime<Utc>>) =
        match expires_in {
            Some(secs) => {
                let exp = Utc::now() + Duration::seconds(secs);
                let refresh = exp - Duration::seconds(60);
                (Some(exp), Some(refresh))
            }
            None => (None, None),
        };

    let result = sqlx::query(
        r#"UPDATE credentials
              SET status = 'active',
                  status_reason = NULL,
                  secrets_ciphertext = $1,
                  metadata = $2,
                  scopes = $3,
                  expires_at = $4,
                  next_refresh_at = $5,
                  last_seen_at = now()
            WHERE id = $6 AND status = 'pending'"#,
    )
    .bind(&secrets_ct)
    .bind(&metadata_json)
    .bind(&scopes_json)
    .bind(expires_at)
    .bind(next_refresh_at)
    .bind(credential_id)
    .execute(db)
    .await?;

    if result.rows_affected() == 0 {
        let current: Option<(String,)> =
            sqlx::query_as("SELECT status FROM credentials WHERE id = $1")
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

/// Atomic finalize for `self_issued_bearer` flows: encrypts the bearer as
/// `{"token": "..."}`, computes HMAC lookup hash for O(1) webhook auth,
/// flips status to `active`. Same idempotency as `finalize_credential`.
pub async fn finalize_self_issued_bearer(
    db: &PgPool,
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
    let metadata_json = metadata.clone();

    let result = sqlx::query(
        r#"UPDATE credentials
              SET status = 'active',
                  status_reason = NULL,
                  secrets_ciphertext = $1,
                  secret_lookup_hash = $2,
                  metadata = $3,
                  last_seen_at = now()
            WHERE id = $4 AND status = 'pending'"#,
    )
    .bind(&secrets_ct)
    .bind(&lookup_hash)
    .bind(&metadata_json)
    .bind(credential_id)
    .execute(db)
    .await?;

    if result.rows_affected() == 0 {
        let current: Option<(String,)> =
            sqlx::query_as("SELECT status FROM credentials WHERE id = $1")
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
              (id, source_id, name, status, secrets_ciphertext, metadata)
           VALUES ($1, $2, $3, 'active', $4, '{}'::jsonb)"#,
    )
    .bind(&id)
    .bind(source_id)
    .bind(name)
    .bind(&secrets_ct)
    .execute(db)
    .await?;

    Ok(id)
}

/// Update an active credential's secrets after a successful refresh.
pub async fn update_credential_secrets(
    db: &PgPool,
    credential_id: &str,
    secrets: &serde_json::Value,
    expires_in: Option<i64>,
) -> Result<()> {
    let encryptor = TokenEncryptor::from_env()?;
    let secrets_str = serde_json::to_string(secrets)?;
    let secrets_ct = encryptor.encrypt(&secrets_str)?;

    let (expires_at, next_refresh_at): (Option<DateTime<Utc>>, Option<DateTime<Utc>>) =
        match expires_in {
            Some(secs) => {
                let exp = Utc::now() + Duration::seconds(secs);
                let refresh = exp - Duration::seconds(60);
                (Some(exp), Some(refresh))
            }
            None => (None, None),
        };

    let result = sqlx::query(
        r#"UPDATE credentials
              SET secrets_ciphertext = $1,
                  expires_at = $2,
                  next_refresh_at = $3,
                  status = 'active',
                  status_reason = NULL,
                  updated_at = now()
            WHERE id = $4"#,
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

/// Decrypt the secrets payload of a credential row.
pub async fn read_credential_secrets(
    db: &PgPool,
    credential_id: &str,
) -> Result<serde_json::Value> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT secrets_ciphertext FROM credentials WHERE id = $1")
            .bind(credential_id)
            .fetch_optional(db)
            .await?;
    let (ciphertext,) = row.ok_or_else(|| AuthError::NotFound(credential_id.to_string()))?;
    let encryptor = TokenEncryptor::from_env()?;
    let plaintext = encryptor.decrypt(&ciphertext)?;
    Ok(serde_json::from_str(&plaintext)?)
}

/// Mark a credential's status without touching its secret payload.
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
            WHERE id = $3"#,
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

/// Return the per-credential fan-out map: `command-name → app_actions.id` for
/// every action row keyed to this credential. The key is `command[0]` (the
/// action's program name, e.g. `ios_ingest`), which the device uses to route a
/// flush to `POST /webhook/{action_id}`. All iOS streams share the single
/// `ios_ingest` action and disambiguate via the `stream` field in the body.
pub async fn fanout_action_ids(
    db: &PgPool,
    credential_id: &str,
) -> Result<HashMap<String, String>> {
    let rows: Vec<(Option<String>, String)> = sqlx::query_as(
        r#"SELECT command::jsonb->>0, id FROM app_actions
           WHERE credential_id = $1 AND command IS NOT NULL"#,
    )
    .bind(credential_id)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|(key, id)| key.map(|k| (k, id)))
        .collect())
}

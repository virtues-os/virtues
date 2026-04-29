//! Post-migration hooks for the credentials Vault.
//!
//! Runs after `sqlx::migrate!` in `database::initialize()`. SQLite migrations
//! are pure SQL and cannot perform crypto operations (decrypt + re-encrypt +
//! HMAC); the work that needs the master key happens here.
//!
//! The `055_credentials_create.sql` migration copies iOS rows from
//! `action_credentials` into `credentials` with the placeholder
//! `secrets_ciphertext = '__PENDING_REENCRYPT__'`. This hook finds those rows,
//! reads the matching plaintext from `action_credentials`, re-encrypts into
//! the new `{"token": ...}` JSON shape, computes the HMAC lookup hash, and
//! merges `device_info` into `metadata`.
//!
//! The hook is idempotent: it only touches rows still bearing the placeholder.
//! Re-running after a successful run is a no-op. If it crashes mid-loop,
//! the next startup picks up where it left off.

use serde_json::json;
use sqlx::SqlitePool;

use crate::crypto::TokenEncryptor;
use crate::error::{Error, Result};

const PLACEHOLDER: &str = "__PENDING_REENCRYPT__";

/// Run all post-migration hooks. Called once during `database::initialize()`
/// after `sqlx::migrate!` completes.
pub async fn run(db: &SqlitePool) -> Result<()> {
    reencrypt_pending_credentials(db).await?;
    log_legacy_table_baseline(db).await;
    Ok(())
}

/// Phase 7 observability gate. Logs the row count of the legacy
/// `action_credentials` table at every boot. Phase 8 (drop in migration 056)
/// is gated on:
///   1. `arch_lint.sh` passing (no new code references the table)
///   2. The re-encryption hook above finding zero pending rows (steady state)
///   3. This baseline being stable for ≥1 week
///
/// If the table doesn't exist (migration 056 has run), this no-ops.
async fn log_legacy_table_baseline(db: &SqlitePool) {
    let exists: Option<(String,)> = sqlx::query_as(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'action_credentials'",
    )
    .fetch_optional(db)
    .await
    .ok()
    .flatten();

    if exists.is_none() {
        // Table is gone — migration 056 has run. Nothing to monitor.
        return;
    }

    let row: Option<(i64,)> =
        sqlx::query_as("SELECT COUNT(*) FROM action_credentials")
            .fetch_optional(db)
            .await
            .ok()
            .flatten();

    if let Some((count,)) = row {
        tracing::info!(
            count,
            "legacy action_credentials baseline (drop scheduled in migration 056; phase-7 observability gate)"
        );
    }
}

/// Re-encrypt iOS device tokens copied by migration 055.
///
/// For each placeholder row in `credentials`:
///   1. Read the legacy `action_credentials` row by id.
///   2. If it has a `device_token`, decrypt it and re-encrypt as
///      `{"token": "<plaintext>"}`. Compute `secret_lookup_hash` for
///      `status = 'active'` rows so webhook auth works.
///   3. If it has no token (pure pending row), encrypt the empty payload
///      `{}` so the column is non-NULL and well-formed.
///   4. Merge `device_info` JSON + the legacy `metadata` JSON +
///      `{device_id: ...}` into the new `credentials.metadata`.
async fn reencrypt_pending_credentials(db: &SqlitePool) -> Result<()> {
    let pending: Vec<(String, String)> = sqlx::query_as(
        r#"SELECT id, status FROM credentials
           WHERE secrets_ciphertext = ?"#,
    )
    .bind(PLACEHOLDER)
    .fetch_all(db)
    .await
    .map_err(|e| Error::Database(format!("failed to scan pending credentials: {e}")))?;

    if pending.is_empty() {
        return Ok(());
    }

    tracing::info!(
        count = pending.len(),
        "re-encrypting iOS credentials into the new Vault shape"
    );

    let encryptor = TokenEncryptor::from_env()?;

    for (cred_id, status) in pending {
        if let Err(e) = reencrypt_one(db, &encryptor, &cred_id, &status).await {
            // Don't abort the whole startup over a single corrupt row — log it
            // and move on. The placeholder remains, so the row is unusable
            // (status check + non-decryptable secrets), but every other
            // credential continues to work.
            tracing::error!(
                credential_id = %cred_id,
                error = %e,
                "failed to re-encrypt credential; leaving placeholder in place",
            );
        }
    }

    Ok(())
}

async fn reencrypt_one(
    db: &SqlitePool,
    encryptor: &TokenEncryptor,
    cred_id: &str,
    status: &str,
) -> Result<()> {
    // Pull the matching legacy row. If it's gone (shouldn't happen — we
    // copied from it in the same migration), there's nothing to do.
    let legacy: Option<(
        Option<String>, // device_id
        Option<String>, // device_token
        Option<String>, // device_info
        Option<String>, // metadata
    )> = sqlx::query_as(
        r#"SELECT device_id, device_token, device_info, metadata
           FROM action_credentials WHERE id = ?"#,
    )
    .bind(cred_id)
    .fetch_optional(db)
    .await
    .map_err(|e| Error::Database(format!("failed to read legacy row: {e}")))?;

    let Some((device_id, device_token_enc, device_info_raw, metadata_raw)) = legacy else {
        return Err(Error::Other(format!(
            "no legacy action_credentials row for placeholder credential {cred_id}"
        )));
    };

    // Build the new metadata: legacy metadata JSON, with device_info keys
    // and an explicit device_id merged on top. Each layer is best-effort.
    let mut metadata = parse_object(metadata_raw.as_deref()).unwrap_or_default();
    if let Some(info) = parse_object(device_info_raw.as_deref()) {
        for (k, v) in info {
            metadata.insert(k, v);
        }
    }
    if let Some(did) = device_id {
        metadata.insert("device_id".into(), json!(did));
    }
    let metadata_json = serde_json::Value::Object(metadata);
    let metadata_str = serde_json::to_string(&metadata_json)
        .map_err(|e| Error::Other(format!("failed to serialize metadata: {e}")))?;

    // Build the new secrets payload.
    let (secrets_ciphertext, secret_lookup_hash) =
        if let Some(enc_token) = device_token_enc.filter(|s| !s.is_empty()) {
            let plaintext = encryptor.decrypt(&enc_token).map_err(|e| {
                Error::Other(format!("failed to decrypt legacy device_token: {e}"))
            })?;
            let payload = json!({ "token": plaintext }).to_string();
            let ciphertext = encryptor.encrypt(&payload)?;
            // Only active credentials need an O(1) lookup hash for webhook
            // auth. Revoked / pending rows leave it NULL so the unique
            // partial index doesn't pin them.
            let lookup_hash = if status == "active" {
                Some(encryptor.lookup_hash(&plaintext)?)
            } else {
                None
            };
            (ciphertext, lookup_hash)
        } else {
            // Pending row with no token yet — encrypt a placeholder empty
            // object. The runtime won't use it (status != 'active'); this
            // just keeps the column NOT NULL well-formed.
            let ciphertext = encryptor.encrypt("{}")?;
            (ciphertext, None)
        };

    sqlx::query(
        r#"UPDATE credentials
           SET secrets_ciphertext = ?,
               secret_lookup_hash = ?,
               metadata = ?
         WHERE id = ?"#,
    )
    .bind(&secrets_ciphertext)
    .bind(&secret_lookup_hash)
    .bind(&metadata_str)
    .bind(cred_id)
    .execute(db)
    .await
    .map_err(|e| Error::Database(format!("failed to update credential row: {e}")))?;

    Ok(())
}

fn parse_object(raw: Option<&str>) -> Option<serde_json::Map<String, serde_json::Value>> {
    let s = raw?;
    match serde_json::from_str::<serde_json::Value>(s) {
        Ok(serde_json::Value::Object(m)) => Some(m),
        _ => None,
    }
}

//! Credential lifecycle helpers — webhook auth, list/rename/revoke.
//!
//! New home (Phase 6) for the small set of `device_pairing.rs` functions
//! that survived the cutover. The pair-flow functions themselves are gone;
//! their logic now lives in `virtues_helpers::auth::*` and is wired by the
//! HTTP handlers in `core/src/api/source_auth.rs`.
//!
//! Functions here:
//!   - `list_credentials` / `rename_credential` / `revoke_credential` —
//!     management API
//!   - `DeviceInfo` / `CredentialListItem` — response shapes
//!   - `device_info_from_metadata` — parse `metadata` JSON into a typed shape
//!
//! Credentials are OUTBOUND secrets (OAuth / API keys). Inbound auth is the
//! proven, allowlisted iroh key — there is no device bearer to validate here.

use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::error::{Error, Result};

// ─────────────────────────────────────────────────────────────────────────────
// DeviceInfo + parsing helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Device information stored in `credentials.metadata` for self_issued_bearer
/// credentials (iOS, Mac, custom paired IoT).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeviceInfo {
    pub device_id: String,
    pub device_name: String,
    pub device_model: String,
    pub os_version: String,
    pub app_version: Option<String>,
}

/// Parse `credentials.metadata` JSON back into a DeviceInfo. Returns None if
/// the JSON doesn't have the expected shape (e.g. for a pending row that has
/// only an empty `{}`, or for a `via_proxy` credential whose metadata holds
/// `{email, ...}` instead).
pub fn device_info_from_metadata(raw: Option<&str>) -> Option<DeviceInfo> {
    raw.and_then(|s| serde_json::from_str::<DeviceInfo>(s).ok())
}

/// Pairing status snapshot — used by legacy management endpoints.
#[derive(Debug, Clone)]
pub enum PairingStatus {
    Pending,
    Active(DeviceInfo),
    Revoked,
}

// ─────────────────────────────────────────────────────────────────────────────
// Credential list / rename / revoke (management API)
// ─────────────────────────────────────────────────────────────────────────────

/// One entry in the `GET /api/credentials` list response. Field shape is kept
/// stable for the existing frontend; under the hood `is_active` is derived
/// from `status`, `provider` is `source_id`, and `auth_type` is derived from
/// the source catalog (`device` for self_issued_bearer, `oauth` for via_proxy,
/// `api_key` for api_key).
#[derive(Debug, Clone, serde::Serialize)]
pub struct CredentialListItem {
    pub id: String,
    pub provider: String,
    pub name: String,
    pub auth_type: String,
    /// Raw status enum: `pending` (mid-pairing), `active`, `revoked`,
    /// `reauth_required`, or `error`. The frontend uses this to filter
    /// transient pending rows out of the list, to distinguish revoked from
    /// pending in display, and to offer Reconnect on the two broken states.
    pub status: String,
    /// Why the credential is in a broken state — the provider's own words
    /// where we have them (a Plaid `ITEM_LOGIN_REQUIRED`, a refresh failure).
    /// Without it a Reconnect button can only say that something is wrong.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_reason: Option<String>,
    pub is_active: bool,
    pub device_info: Option<DeviceInfo>,
    pub last_seen_at: Option<String>,
    pub created_at: String,
    /// Number of `app_applets` rows linked to this credential.
    pub applet_count: i64,
    /// Derived initial-sync lifecycle for active credentials (Tier 2 UX):
    /// `connected` (paired, no run yet) → `backfilling` (runs in flight, no
    /// success) → `live` (≥1 successful run). `None` for pending/revoked rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_state: Option<String>,
}

/// Credentials that are the box's own plumbing rather than a source the user
/// connected: the subscription key that pays for inference (`virtues_api`) and
/// the BYO provider key (`__byo_ai_key__`). Both are `credentials` rows because
/// that is where encrypted secrets live, and neither has a `[[source]]` in the
/// catalog — so the Sources room rendered them as orphaned connections whose
/// "source is no longer installed", offering to disconnect the thing paying for
/// the box. Each already has its own surface in Settings → Billing.
const INTERNAL_SOURCE_IDS: [&str; 2] = [
    crate::virtues_api::renew::SOURCE_ID,
    crate::api::settings_byo::BYO_SOURCE_ID,
];

/// List credentials. Returns pending and revoked rows too so the UI can show
/// them with a distinct status. Ordered newest first.
pub async fn list_credentials(db: &PgPool) -> Result<Vec<CredentialListItem>> {
    let rows: Vec<(
        String,
        String,
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        String,
        i64,
        i64,
        i64,
    )> = sqlx::query_as(
        r#"SELECT
              c.id,
              c.key,
              c.name,
              c.status,
              c.status_reason,
              c.metadata::text,
              c.last_seen_at::text,
              c.created_at::text,
              (SELECT COUNT(*) FROM app_applets WHERE credential_id = c.id) AS applet_count,
              (SELECT COUNT(*) FROM app_applet_runs r JOIN app_applets a ON a.id = r.applet_id
                 WHERE a.credential_id = c.id) AS total_runs,
              (SELECT COUNT(*) FROM app_applet_runs r JOIN app_applets a ON a.id = r.applet_id
                 WHERE a.credential_id = c.id AND r.status = 'success') AS success_runs
           FROM credentials c
          WHERE c.kind = 'source' AND c.key <> ALL($1)
           ORDER BY c.created_at DESC"#,
    )
    .bind(&INTERNAL_SOURCE_IDS[..])
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(
            |(id, source_id, name, status, status_reason, metadata_raw, last_seen_at, created_at, applet_count, total_runs, success_runs)| {
                let device_info = device_info_from_metadata(Some(&metadata_raw));
                let auth_type = auth_type_for_source(&source_id).to_string();
                let is_active = status == "active";
                let sync_state = is_active.then(|| sync_state_for(total_runs, success_runs).to_string());
                CredentialListItem {
                    id,
                    provider: source_id,
                    name,
                    auth_type,
                    is_active,
                    status,
                    status_reason,
                    device_info,
                    last_seen_at,
                    created_at,
                    applet_count,
                    sync_state,
                }
            },
        )
        .collect())
}

/// Derive the Tier-2 sync lifecycle from an active credential's run history.
/// Pure for unit-testing: `live` once anything succeeded, `backfilling` while
/// runs are in flight with no success yet, else `connected`.
fn sync_state_for(total_runs: i64, success_runs: i64) -> &'static str {
    if success_runs > 0 {
        "live"
    } else if total_runs > 0 {
        "backfilling"
    } else {
        "connected"
    }
}

/// Map a source id to the legacy `auth_type` string the frontend expects.
/// Catalog-driven via `lookup_source` — no per-provider matching here.
fn auth_type_for_source(source_id: &str) -> &'static str {
    use crate::applet_templates::{lookup_source, SourceAuth};
    match lookup_source(source_id).map(|s| s.auth) {
        Some(SourceAuth::SelfIssuedBearer) => "device",
        Some(SourceAuth::ViaProxy { .. }) => "oauth",
        Some(SourceAuth::ApiKey { .. }) => "api_key",
        None => "unknown",
    }
}

/// Rename a credential (display name only — does not change routing).
pub async fn rename_credential(db: &PgPool, credential_id: &str, new_name: &str) -> Result<()> {
    if new_name.trim().is_empty() {
        return Err(Error::InvalidInput("name cannot be empty".into()));
    }
    let affected = sqlx::query("UPDATE credentials SET name = $1 WHERE id = $2 AND kind = 'source'")
        .bind(new_name)
        .bind(credential_id)
        .execute(db)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(Error::NotFound(format!(
            "credential not found: {credential_id}"
        )));
    }
    Ok(())
}

/// Revoke a credential and delete its fan-out `app_applets` rows.
///
/// Flow:
/// 1. Set `status = 'revoked'` so template reconcile skips this credential,
///    and in the same statement replace the sealed tokens or session with
///    an empty payload and clear the refresh schedule. The provider still
///    honors the grant until the owner revokes it there; the box no longer
///    holds it.
/// 2. Nullify `applet_id` on any historical runs for the credential's
///    fan-out actions (FK safety).
/// 3. Delete the per-credential action rows. Reconcile won't re-create
///    them because the credential is no longer active.
///
/// The row itself stays, and run history is preserved with
/// `applet_id = NULL`, so the history view can still surface past runs.
///
/// The box's own keys ([`INTERNAL_SOURCE_IDS`]) are not found here, as in
/// the list. Billing is the only place that removes them: removing the BYO
/// key needs the app's owner proof (`api::owner_proof`), and this route asks
/// for none.
pub async fn revoke_credential(db: &PgPool, credential_id: &str) -> Result<()> {
    let emptied = crate::credentials::emptied_secrets()?;
    let affected = sqlx::query(
        r#"UPDATE credentials
              SET status = 'revoked',
                  status_reason = 'user_revoked',
                  secrets_ciphertext = $2,
                  expires_at = NULL,
                  next_refresh_at = NULL
            WHERE id = $1 AND kind = 'source' AND key <> ALL($3)"#,
    )
    .bind(credential_id)
    .bind(&emptied)
    .bind(&INTERNAL_SOURCE_IDS[..])
    .execute(db)
    .await?
    .rows_affected();
    if affected == 0 {
        return Err(Error::NotFound(format!(
            "credential not found: {credential_id}"
        )));
    }

    sqlx::query(
        r#"UPDATE app_applet_runs SET applet_id = NULL
           WHERE applet_id IN (SELECT id FROM app_applets WHERE credential_id = $1)"#,
    )
    .bind(credential_id)
    .execute(db)
    .await?;

    sqlx::query("DELETE FROM app_applets WHERE credential_id = $1")
        .bind(credential_id)
        .execute(db)
        .await?;

    Ok(())
}

/// Hard-delete a credential row. Only safe for `pending` rows that never
/// completed pairing (no secrets, no fan-out actions yet).
/// For active or revoked rows, callers should use `revoke_credential`
/// instead so run history is preserved.
pub async fn delete_pending_credential(db: &PgPool, credential_id: &str) -> Result<()> {
    let affected = sqlx::query(
        "DELETE FROM credentials WHERE id = $1 AND kind = 'source' AND status = 'pending'",
    )
    .bind(credential_id)
    .execute(db)
    .await?
    .rows_affected();
    if affected == 0 {
        // Either the row doesn't exist or it's no longer pending — both are
        // benign in the context where this is called (modal cancel races with
        // successful pair_complete).
        return Err(Error::NotFound(format!(
            "no pending credential to delete: {credential_id}"
        )));
    }
    Ok(())
}

/// Look up the pairing status of a credential. Used by the legacy poll endpoint
/// (web UI checks if QR scan completed). Phase 6+: superseded by SSE/WebSocket
/// or by `pair_complete` returning the final state synchronously.
pub async fn check_pairing_status(
    db: &PgPool,
    credential_id: String,
) -> Result<PairingStatus> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT status, metadata::text FROM credentials WHERE id = $1 AND kind = 'source'")
            .bind(&credential_id)
            .fetch_optional(db)
            .await?;
    let (status, metadata) =
        row.ok_or_else(|| Error::NotFound(format!("credential not found: {credential_id}")))?;
    Ok(match status.as_str() {
        "active" => PairingStatus::Active(
            device_info_from_metadata(Some(&metadata))
                .ok_or_else(|| Error::Other("active credential has no device_info".into()))?,
        ),
        "revoked" => PairingStatus::Revoked,
        _ => PairingStatus::Pending,
    })
}

/// Pending pairing list item — used by the management API.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PendingPairing {
    pub source_id: String,
    pub name: String,
    pub device_type: String,
    pub code: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// List pending pairings — used by the legacy management UI.
pub async fn list_pending_pairings(db: &PgPool) -> Result<Vec<PendingPairing>> {
    let rows: Vec<(String, String, String, String)> = sqlx::query_as(
        r#"SELECT id, name, key, created_at
           FROM credentials WHERE kind = 'source' AND status = 'pending'
           ORDER BY created_at DESC"#,
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, name, source_id, created_at)| PendingPairing {
            source_id: id,
            name,
            device_type: source_id,
            code: String::new(),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(10),
            created_at: chrono::DateTime::parse_from_rfc3339(&created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::ensure_test_key;

    /// Revoking forgets the value. The row stays for run history, but a
    /// backup taken afterwards must not carry the tokens the owner removed.
    #[sqlx::test(migrations = "./migrations")]
    async fn revoking_a_source_wipes_its_sealed_tokens(pool: PgPool) {
        ensure_test_key();
        let enc = crate::crypto::TokenEncryptor::from_env().unwrap();
        let sealed = enc
            .encrypt(r#"{"access_token":"at-example","refresh_token":"rt-example"}"#)
            .unwrap();
        sqlx::query(
            "INSERT INTO credentials (id, key, name, status, secrets_ciphertext, expires_at, next_refresh_at) \
             VALUES ('cred_example', 'google', 'Google', 'active', $1, now() + interval '1 hour', now() + interval '59 minutes')",
        )
        .bind(&sealed)
        .execute(&pool)
        .await
        .unwrap();

        revoke_credential(&pool, "cred_example").await.unwrap();

        let (status, ciphertext, expires_at, next_refresh_at): (
            String,
            String,
            Option<DateTime<Utc>>,
            Option<DateTime<Utc>>,
        ) = sqlx::query_as(
            "SELECT status, secrets_ciphertext, expires_at, next_refresh_at FROM credentials WHERE id = 'cred_example'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(status, "revoked");
        assert_eq!(enc.decrypt(&ciphertext).unwrap(), "{}", "the tokens are gone");
        assert_eq!((expires_at, next_refresh_at), (None, None));

        // An id that is not a source row is still an error.
        assert!(matches!(revoke_credential(&pool, "cred_missing").await, Err(Error::NotFound(_))));
    }

    /// The subscription key and the BYO key are removed from Billing only.
    /// A revoke by id leaves them active and sealed as they were.
    #[sqlx::test(migrations = "./migrations")]
    async fn the_box_own_keys_are_not_revoked_by_id(pool: PgPool) {
        ensure_test_key();
        let enc = crate::crypto::TokenEncryptor::from_env().unwrap();
        for (id, key) in [
            ("cred_subscription", INTERNAL_SOURCE_IDS[0]),
            ("cred_byo", INTERNAL_SOURCE_IDS[1]),
        ] {
            sqlx::query(
                "INSERT INTO credentials (id, key, name, status, secrets_ciphertext) \
                 VALUES ($1, $2, 'Box key', 'active', $3)",
            )
            .bind(id)
            .bind(key)
            .bind(enc.encrypt(r#"{"api_key":"vk-example"}"#).unwrap())
            .execute(&pool)
            .await
            .unwrap();

            assert!(matches!(revoke_credential(&pool, id).await, Err(Error::NotFound(_))), "{key}");

            let (status, ciphertext): (String, String) =
                sqlx::query_as("SELECT status, secrets_ciphertext FROM credentials WHERE id = $1")
                    .bind(id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(status, "active", "{key}");
            assert_eq!(enc.decrypt(&ciphertext).unwrap(), r#"{"api_key":"vk-example"}"#, "{key}");
        }
    }

    #[test]
    fn sync_state_lifecycle() {
        // No runs yet → just connected.
        assert_eq!(sync_state_for(0, 0), "connected");
        // Runs in flight, none succeeded → backfilling.
        assert_eq!(sync_state_for(3, 0), "backfilling");
        // Anything succeeded → live (data is flowing).
        assert_eq!(sync_state_for(5, 1), "live");
    }
}

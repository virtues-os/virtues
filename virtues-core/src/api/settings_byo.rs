//! `POST /api/settings/byo-key` + `DELETE /api/settings/byo-key`.
//!
//! BYO ("bring your own") provider key is the user's escape hatch from the
//! Virtues wallet. When set, all chat traffic routes box → upstream provider
//! directly, bypassing virtues-api entirely. Virtues is no longer in the
//! inference path — that's the whole point.
//!
//! The key is stored as a `credentials` row with `source_id = "__byo_ai_key__"`,
//! encrypted at rest via the same `TokenEncryptor` that protects every other
//! secret. The agent module reads it just before each call; it never lives
//! in the chat request body or in the URL.
//!
//! Both endpoints are sudo-gated (`change_byo_key` is one of the four locked
//! sudo actions). The handler verifies a sudo request id before doing
//! anything destructive.

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;

use crate::middleware::auth::AuthUser;

/// BYO credential's `source_id`. Stable string so the agent + the
/// settings handlers agree on the row to look up.
pub const BYO_SOURCE_ID: &str = "__byo_ai_key__";

#[derive(Debug, Deserialize)]
pub struct SaveRequest {
    /// Sudo request id obtained from `/api/sudo/request`. Required.
    pub sudo_request_id: String,
    /// Provider slug. v1 understands `openai`, `anthropic`, `xai`, `google`,
    /// `custom`. `custom` requires `endpoint_url` to be set.
    pub provider: String,
    /// Raw API key as the user copied it from the provider's dashboard.
    pub api_key: String,
    /// Required when `provider = "custom"`. Ignored otherwise.
    #[serde(default)]
    pub endpoint_url: Option<String>,
    /// Optional default model name to use (e.g. `claude-3-5-sonnet-latest`).
    /// When omitted, the agent module's per-provider default applies.
    #[serde(default)]
    pub default_model: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DeleteRequest {
    pub sudo_request_id: String,
}

#[derive(Debug, Serialize)]
pub struct ByoStatus {
    pub configured: bool,
    pub provider: Option<String>,
    pub default_model: Option<String>,
    pub endpoint_url: Option<String>,
    pub created_at: Option<String>,
}

/// `GET /api/settings/byo-key` — non-sudo'd status read. Returns the
/// provider + model + endpoint metadata so the UI can render "BYO active
/// (Anthropic)" or "Using Virtues subscription"; never returns the key
/// itself.
pub async fn status_handler(State(pool): State<PgPool>, _user: AuthUser) -> impl IntoResponse {
    let row: Option<(serde_json::Value, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT metadata, created_at FROM credentials \
         WHERE source_id = $1 AND status = 'active' \
         LIMIT 1",
    )
    .bind(BYO_SOURCE_ID)
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten();

    match row {
        Some((metadata, created_at)) => {
            let provider = metadata.get("provider").and_then(|v| v.as_str()).map(String::from);
            let default_model = metadata
                .get("default_model")
                .and_then(|v| v.as_str())
                .map(String::from);
            let endpoint_url = metadata
                .get("endpoint_url")
                .and_then(|v| v.as_str())
                .map(String::from);
            (
                StatusCode::OK,
                Json(ByoStatus {
                    configured: true,
                    provider,
                    default_model,
                    endpoint_url,
                    created_at: Some(created_at.to_rfc3339()),
                }),
            )
                .into_response()
        }
        None => (
            StatusCode::OK,
            Json(ByoStatus {
                configured: false,
                provider: None,
                default_model: None,
                endpoint_url: None,
                created_at: None,
            }),
        )
            .into_response(),
    }
}

/// `POST /api/settings/byo-key` — save a BYO provider key. Requires sudo.
pub async fn save_handler(
    State(pool): State<PgPool>,
    user: AuthUser,
    Json(req): Json<SaveRequest>,
) -> impl IntoResponse {
    // Validate provider + endpoint shape before we even check sudo, so a
    // simple input error doesn't burn the sudo approval.
    if let Err(msg) = validate_provider(&req.provider, req.endpoint_url.as_deref()) {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))).into_response();
    }
    if req.api_key.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "api_key required" })),
        )
            .into_response();
    }

    // Sudo gate. The id must be approved + matched to the requesting device.
    if let Err(resp) =
        crate::api::sudo::verify_and_consume(&pool, &req.sudo_request_id, "change_byo_key", &user.device_id)
            .await
            .map(|_| ())
    {
        tracing::warn!("BYO key save: sudo verify failed: {resp}");
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "sudo_not_approved" })),
        )
            .into_response();
    }

    // Encrypt the key + assemble the credential row.
    let encryptor = match crate::crypto::TokenEncryptor::from_env() {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("BYO key save: encryptor init failed: {e:#}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "encryption_unavailable" })),
            )
                .into_response();
        }
    };
    let ciphertext = match encryptor.encrypt(&json!({ "api_key": req.api_key.trim() }).to_string()) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("BYO key save: encrypt failed: {e:#}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "encrypt_failed" })),
            )
                .into_response();
        }
    };

    let credential_id = crate::ids::generate_id(
        crate::ids::AUTH_TOKEN_PREFIX,
        &[BYO_SOURCE_ID, &chrono::Utc::now().to_rfc3339()],
    );
    let mut metadata = json!({
        "provider": req.provider,
        "default_model": req.default_model,
    });
    if let Some(url) = req.endpoint_url.as_deref() {
        metadata["endpoint_url"] = json!(url);
    }

    // Replace any existing BYO credential in one transaction. We never
    // keep both old and new live at the same time — the user expects "I
    // pasted a new key, the old one is gone."
    let mut tx = match pool.begin().await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("BYO key save: tx begin failed: {e:#}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal" })),
            )
                .into_response();
        }
    };
    let _ = sqlx::query(
        "UPDATE credentials SET status = 'revoked', \
                                 status_reason = 'replaced_by_user', updated_at = now() \
         WHERE source_id = $1 AND status = 'active'",
    )
    .bind(BYO_SOURCE_ID)
    .execute(&mut *tx)
    .await;

    if let Err(e) = sqlx::query(
        "INSERT INTO credentials \
         (id, source_id, name, status, secrets_ciphertext, metadata) \
         VALUES ($1, $2, $3, 'active', $4, $5)",
    )
    .bind(&credential_id)
    .bind(BYO_SOURCE_ID)
    .bind(format!("BYO {}", req.provider))
    .bind(&ciphertext)
    .bind(&metadata)
    .execute(&mut *tx)
    .await
    {
        tracing::warn!("BYO key save: insert failed: {e:#}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "insert_failed" })),
        )
            .into_response();
    }

    if let Err(e) = tx.commit().await {
        tracing::warn!("BYO key save: tx commit failed: {e:#}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "internal" })),
        )
            .into_response();
    }

    // Audit log so the user can see "BYO key changed" in /virtues/activity.
    let _ = sqlx::query(
        "INSERT INTO app_auth_event (user_id, device_id, event_type, detail) \
         VALUES ($1, $2, 'byo_key_set', $3)",
    )
    .bind(&user.id)
    .bind(&user.device_id)
    .bind(json!({ "provider": req.provider }))
    .execute(&pool)
    .await;

    (StatusCode::OK, Json(json!({ "ok": true }))).into_response()
}

/// `DELETE /api/settings/byo-key` — clear the BYO key. Requires sudo.
pub async fn delete_handler(
    State(pool): State<PgPool>,
    user: AuthUser,
    Json(req): Json<DeleteRequest>,
) -> impl IntoResponse {
    if let Err(_) = crate::api::sudo::verify_and_consume(
        &pool,
        &req.sudo_request_id,
        "change_byo_key",
        &user.device_id,
    )
    .await
    {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "sudo_not_approved" })),
        )
            .into_response();
    }

    let _ = sqlx::query(
        "UPDATE credentials SET status = 'revoked', \
                                 status_reason = 'user_deleted', updated_at = now() \
         WHERE source_id = $1 AND status = 'active'",
    )
    .bind(BYO_SOURCE_ID)
    .execute(&pool)
    .await;

    let _ = sqlx::query(
        "INSERT INTO app_auth_event (user_id, device_id, event_type, detail) \
         VALUES ($1, $2, 'byo_key_cleared', '{}'::jsonb)",
    )
    .bind(&user.id)
    .bind(&user.device_id)
    .execute(&pool)
    .await;

    (StatusCode::OK, Json(json!({ "ok": true }))).into_response()
}

fn validate_provider(provider: &str, endpoint_url: Option<&str>) -> Result<(), &'static str> {
    match provider {
        "openai" | "anthropic" | "xai" | "google" => Ok(()),
        "custom" => match endpoint_url {
            Some(url) if !url.trim().is_empty() => Ok(()),
            _ => Err("provider=custom requires endpoint_url"),
        },
        _ => Err("unsupported provider"),
    }
}

// ─── Direct-upstream resolution ────────────────────────────────────────────

/// What the agent module gets back from `load_byo_credential` — the
/// decrypted key + the routing metadata. Returned only when an active BYO
/// credential exists; otherwise `None` and the agent uses the wallet path.
#[derive(Debug)]
pub struct ByoCredential {
    pub provider: String,
    pub api_key: String,
    pub endpoint_url: String,
    pub default_model: Option<String>,
}

/// Look up the active BYO credential, decrypt the key, and return the
/// upstream-routing tuple the agent needs. None when the user is on the
/// wallet path.
pub async fn load_byo_credential(pool: &PgPool) -> Result<Option<ByoCredential>, crate::Error> {
    let row: Option<(String, serde_json::Value)> = sqlx::query_as(
        "SELECT secrets_ciphertext, metadata FROM credentials \
         WHERE source_id = $1 AND status = 'active' \
         LIMIT 1",
    )
    .bind(BYO_SOURCE_ID)
    .fetch_optional(pool)
    .await
    .map_err(|e| crate::Error::Database(format!("BYO lookup: {e}")))?;
    let Some((ciphertext, metadata)) = row else {
        return Ok(None);
    };

    let encryptor = crate::crypto::TokenEncryptor::from_env()
        .map_err(|e| crate::Error::Other(format!("BYO encryptor init: {e}")))?;
    let decrypted = encryptor
        .decrypt(&ciphertext)
        .map_err(|e| crate::Error::Other(format!("BYO decrypt: {e}")))?;
    let parsed: serde_json::Value = serde_json::from_str(&decrypted)
        .map_err(|e| crate::Error::Other(format!("BYO json: {e}")))?;
    let api_key = parsed["api_key"]
        .as_str()
        .ok_or_else(|| crate::Error::Other("BYO credential missing api_key".to_string()))?
        .to_string();
    let provider = metadata
        .get("provider")
        .and_then(|v| v.as_str())
        .unwrap_or("openai")
        .to_string();
    let endpoint_url = metadata
        .get("endpoint_url")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| default_endpoint_for(&provider).to_string());
    let default_model = metadata
        .get("default_model")
        .and_then(|v| v.as_str())
        .map(String::from);

    Ok(Some(ByoCredential {
        provider,
        api_key,
        endpoint_url,
        default_model,
    }))
}

fn default_endpoint_for(provider: &str) -> &'static str {
    match provider {
        "openai" => "https://api.openai.com/v1/chat/completions",
        "anthropic" => "https://api.anthropic.com/v1/messages",
        "xai" => "https://api.x.ai/v1/chat/completions",
        "google" => "https://generativelanguage.googleapis.com/v1beta",
        // `custom` always comes with an explicit endpoint_url.
        _ => "",
    }
}

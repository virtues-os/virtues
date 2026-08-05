//! `POST /api/settings/byo-key` + `DELETE /api/settings/byo-key`.
//!
//! BYO ("bring your own") provider key is the user's escape hatch from the
//! Virtues wallet: with one set, every `/v1/ai/*` call leaves by their
//! endpoint and virtues-api is out of the inference path.
//!
//! **This module is storage; the routing lives in one place —
//! `virtues_api/client.rs`.** Both `stream()` and `post_json()` call
//! `load_byo_credential` below and divert, gated on the same `is_ai_path`
//! predicate, so all seven AI callers are covered without opting in:
//! `agent/stream.rs`, `api/compaction.rs`, `api/day_summary.rs`,
//! `api/image_gen.rs`, `api/entity_article_gen.rs`,
//! `api/narrative_identity_gen.rs`, `api/chats.rs`, plus the
//! `transcription_resolution` applet.
//!
//! Until 2026-08-05 only `stream()` honored the key and the other seven
//! quietly billed the wallet while the UI said "BYO active" — a claim false by
//! omission, which is the harder kind to notice. Two lessons worth keeping:
//!
//! - **Do not conclude from grepping `BYO_SOURCE_ID` that nothing reads the
//!   key.** The routing goes through `load_byo_credential` and never mentions
//!   the constant. The consumers that only *report* a key is present are
//!   `status_json.rs`, `box_status.rs`, `billing_state.rs`, `credentials.rs`.
//! - **The audit undercounted the leak at four paths; it was seven.** Keying
//!   the fork on the route rather than adding an opt-in `post_ai()` method is
//!   what caught the other three, and is why a new AI caller cannot regress
//!   this.
//!
//! **A BYO credential is a URL and a key.** There is no provider taxonomy:
//! `provider` never drove behavior, and as of 2026-08-05 it is a deprecated
//! input kept only so an un-updated client still saves. We speak exactly one
//! contract — OpenAI-style `/chat/completions` with a Bearer token — which is
//! what OpenAI, xAI, Groq, DeepSeek, Together and Mistral serve natively,
//! what Anthropic and Google serve on their compat endpoints, what every
//! gateway is, and what Ollama, LM Studio, vLLM and llama.cpp serve locally.
//! Bedrock alone is out, because SigV4 is not a bearer token; it is reached
//! through a gateway that fronts it.
//!
//! Two honest gaps remain, both design rather than oversight: request bodies
//! that pin a model send *our* gateway's id (`google/gemini-3-flash`), which
//! another gateway may spell differently, and audio rides as an `image_url`
//! data-URI, which is a Vercel quirk that OpenRouter will reject. Per-slot
//! model ids and a per-route audio encoder fix them —
//! `docs/byo-ai-plan.md` phases 2 and 3. The Billing UI and
//! `docs/virtues-api.md` are now true as written.
//!
//! The key is stored as a `credentials` row with `source_id = "__byo_ai_key__"`,
//! encrypted at rest via the same `TokenEncryptor` that protects every other
//! secret. It never lives in the chat request body or in the URL.
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
    /// The endpoint to POST to. **This is the field that matters** — any URL
    /// speaking OpenAI-style `/chat/completions` with a Bearer token.
    #[serde(default)]
    pub endpoint_url: Option<String>,
    /// Raw API key as the user copied it from their provider's dashboard.
    pub api_key: String,
    /// **Deprecated.** A provider slug used to look up a hardcoded URL. Kept
    /// only so a client that has not moved to `endpoint_url` keeps working;
    /// see [`legacy_preset_endpoint`] for why the table is going away.
    #[serde(default)]
    pub provider: Option<String>,
    /// Optional model id to send when a request body does not pin one.
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
    // Resolve + validate the endpoint before we even check sudo, so a simple
    // input error doesn't burn the sudo approval.
    let endpoint_url = match resolve_endpoint(req.endpoint_url.as_deref(), req.provider.as_deref())
    {
        Ok(url) => url,
        Err(msg) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))).into_response()
        }
    };
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
    // `endpoint_url` is always written now, even when it came from a legacy
    // preset — so a row saved today never depends on the preset table still
    // existing tomorrow. `provider` is recorded only as the label the user
    // chose, never read back for routing.
    let mut metadata = json!({
        "endpoint_url": endpoint_url,
        "default_model": req.default_model,
    });
    if let Some(p) = req.provider.as_deref() {
        metadata["provider"] = json!(p);
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
    .bind(format!(
        "BYO {}",
        endpoint_host(&endpoint_url).unwrap_or_else(|| "endpoint".to_string())
    ))
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

    // Audit log — `app_auth_event`, readable at GET /api/audit/auth. (It had a
    // UI at /virtues/activity; that page is gone. The row is still written.)
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

/// Is a BYO credential active right now?
///
/// For the *streaming* recorders (`api/chat.rs`, `agent/applet_runner.rs`)
/// which build their own `AiCall` rows far from the fork in `client.rs` and so
/// cannot see which way the call went. One indexed `EXISTS` and no decryption,
/// which is nothing beside the LLM call it is labelling.
///
/// Prefer plumbing the actual route through where you can — this answers "is
/// BYO on now", not "did *that* call take it", so flipping BYO mid-call could
/// mislabel one row. Harmless for a usage breakdown; never make a billing
/// decision on it.
pub async fn byo_is_active(pool: &PgPool) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM credentials \
          WHERE source_id = $1 AND status = 'active')",
    )
    .bind(BYO_SOURCE_ID)
    .fetch_one(pool)
    .await
    .unwrap_or(false)
}

/// Settle on the URL to POST to: the user's, or a legacy preset.
///
/// The user's `endpoint_url` always wins. `provider` is consulted only when no
/// URL was sent, which happens for clients that predate this change.
fn resolve_endpoint(
    endpoint_url: Option<&str>,
    provider: Option<&str>,
) -> Result<String, &'static str> {
    if let Some(url) = endpoint_url.map(str::trim).filter(|u| !u.is_empty()) {
        validate_endpoint(url)?;
        return Ok(url.to_string());
    }
    match provider.map(str::trim).map(legacy_preset_endpoint) {
        Some(Some(url)) => Ok(url.to_string()),
        _ => Err("endpoint_url required"),
    }
}

/// Is this a URL we can actually POST to?
///
/// Deliberately shallow. We check the scheme and that a host exists, and stop
/// — not the path, because gateway layouts vary too much to predict (Azure
/// OpenAI buries a deployment name and an `api-version` query param in
/// theirs), and a rule we cannot state correctly would reject working setups
/// to no benefit. A wrong path fails loudly on first use with the provider's
/// own 404, which is a better teacher than our guess.
///
/// `http` is allowed **only for loopback**, so that Ollama, LM Studio, vLLM
/// and llama.cpp — none of which serve TLS by default — work without making
/// plaintext keys over a network the easy path.
fn validate_endpoint(url: &str) -> Result<(), &'static str> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or("endpoint_url must be an absolute http(s) URL")?;
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .rsplit('@')
        .next()
        .unwrap_or_default();
    let hostname = host.rsplit_once(':').map_or(host, |(h, _)| h);
    if hostname.is_empty() {
        return Err("endpoint_url has no host");
    }
    match scheme {
        "https" => Ok(()),
        "http" if is_loopback(hostname) => Ok(()),
        "http" => Err("endpoint_url must use https (http is allowed for localhost only)"),
        _ => Err("endpoint_url must be an absolute http(s) URL"),
    }
}

fn is_loopback(hostname: &str) -> bool {
    matches!(hostname, "localhost" | "127.0.0.1" | "::1" | "[::1]")
        || hostname.ends_with(".localhost")
}

/// The host of an endpoint, for labelling. `None` if it cannot be parsed.
fn endpoint_host(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let host = rest.split(['/', '?', '#']).next()?.rsplit('@').next()?;
    (!host.is_empty()).then(|| host.to_string())
}

/// **Deprecated: resolves old rows and old clients only. Do not add to it.**
///
/// This table was a false taxonomy. `provider` never drove any behavior —
/// nothing in the routing path branches on it, because there is nothing to
/// branch on: one contract, always Bearer. All it ever did was pick a URL,
/// and a hardcoded URL rots. Two of the four entries were wrong for months in
/// a user-facing dropdown: `anthropic` pointed at `/v1/messages`, which wants
/// different headers, a top-level `system`, and returns `content[].text`; and
/// `google` at a bare `/v1beta`, which is not a callable path at all. Both
/// were fixed to the vendors' OpenAI-compat URLs before this function was
/// demoted — the demotion is the actual fix, because it removes the class.
///
/// The user brings a URL. Example URLs belong in help text, where going stale
/// makes a doc wrong instead of a shipped option broken.
fn legacy_preset_endpoint(provider: &str) -> Option<&'static str> {
    match provider {
        "openai" => Some("https://api.openai.com/v1/chat/completions"),
        "anthropic" => Some("https://api.anthropic.com/v1/chat/completions"),
        "xai" => Some("https://api.x.ai/v1/chat/completions"),
        "google" => Some("https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"),
        // `custom` never had a preset; it always carried its own URL.
        _ => None,
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
    let stored_provider = metadata.get("provider").and_then(|v| v.as_str());
    // Rows saved from now on always carry `endpoint_url`. Older ones may have
    // only a provider slug, so the legacy table resolves those — which is why
    // dropping the concept needs no migration.
    let endpoint_url = match metadata.get("endpoint_url").and_then(|v| v.as_str()) {
        Some(url) => url.to_string(),
        None => stored_provider
            .and_then(legacy_preset_endpoint)
            .ok_or_else(|| {
                crate::Error::Other(
                    "BYO credential has neither endpoint_url nor a known provider".to_string(),
                )
            })?
            .to_string(),
    };
    // Label only. Nothing downstream branches on it — see `resolve_endpoint`.
    let provider = stored_provider
        .map(String::from)
        .or_else(|| endpoint_host(&endpoint_url))
        .unwrap_or_else(|| "custom".to_string());
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The legacy table resolves old rows and old clients; it must still be
    /// callable and must still speak the one contract. `anthropic` pointed at
    /// `/v1/messages` and `google` at a bare `/v1beta` for months in a
    /// user-facing dropdown, because nothing asserted this.
    #[test]
    fn legacy_presets_still_speak_the_one_contract() {
        for provider in ["openai", "anthropic", "xai", "google"] {
            let url = legacy_preset_endpoint(provider).expect("legacy preset missing");
            assert!(url.starts_with("https://"), "`{provider}` is not https: {url}");
            assert!(
                url.ends_with("/chat/completions"),
                "`{provider}` is not an OpenAI-style chat/completions URL: {url}"
            );
        }
        assert!(legacy_preset_endpoint("custom").is_none());
        assert!(legacy_preset_endpoint("bedrock").is_none());
    }

    /// The user's URL always wins; the slug is consulted only in its absence.
    #[test]
    fn the_users_endpoint_beats_the_legacy_slug() {
        let mine = "https://gateway.example/v1/chat/completions";
        assert_eq!(resolve_endpoint(Some(mine), Some("openai")).unwrap(), mine);
        assert_eq!(
            resolve_endpoint(None, Some("openai")).unwrap(),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(resolve_endpoint(Some("   "), Some("xai")).unwrap(), "https://api.x.ai/v1/chat/completions");
    }

    /// Neither a URL nor a resolvable slug is a rejection, never a default.
    /// Silently posting a user's key at an endpoint they did not name would be
    /// worse than failing.
    #[test]
    fn nothing_usable_is_an_error_not_a_fallback() {
        assert!(resolve_endpoint(None, None).is_err());
        assert!(resolve_endpoint(None, Some("bedrock")).is_err());
        assert!(resolve_endpoint(Some(""), Some("")).is_err());
    }

    /// https anywhere; http only for loopback, so a local Ollama or LM Studio
    /// works without making plaintext keys over a network the easy path.
    #[test]
    fn http_is_loopback_only() {
        assert!(validate_endpoint("https://api.openai.com/v1/chat/completions").is_ok());
        assert!(validate_endpoint("http://localhost:11434/v1/chat/completions").is_ok());
        assert!(validate_endpoint("http://127.0.0.1:1234/v1/chat/completions").is_ok());
        assert!(validate_endpoint("http://[::1]:8080/v1/chat/completions").is_ok());
        assert!(validate_endpoint("http://gateway.example/v1/chat/completions").is_err());
        assert!(validate_endpoint("ftp://gateway.example/v1").is_err());
        assert!(validate_endpoint("api.openai.com/v1/chat/completions").is_err());
        assert!(validate_endpoint("https:///v1/chat/completions").is_err());
    }

    /// Paths are deliberately unchecked — Azure OpenAI buries a deployment
    /// name and an `api-version` query param in its own, and a rule we cannot
    /// state correctly would reject working setups to no benefit.
    #[test]
    fn unusual_paths_are_accepted() {
        assert!(validate_endpoint(
            "https://res.openai.azure.com/openai/deployments/gpt4/chat/completions?api-version=2024-02-01"
        )
        .is_ok());
        assert!(validate_endpoint("https://gw.example/").is_ok());
    }

    #[test]
    fn host_is_extracted_for_labelling() {
        assert_eq!(
            endpoint_host("https://openrouter.ai/api/v1/chat/completions").as_deref(),
            Some("openrouter.ai")
        );
        assert_eq!(
            endpoint_host("http://localhost:11434/v1/chat/completions").as_deref(),
            Some("localhost:11434")
        );
        assert_eq!(endpoint_host("not-a-url").as_deref(), None);
    }
}

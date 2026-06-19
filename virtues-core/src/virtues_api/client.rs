//! Bearer-authenticated client for the new virtues-api routes.
//!
//! Attaches the home server's current bearer (from the credential vault)
//! and auto-renews once on a `bearer_expired` (402) — the OAuth
//! refresh-token pattern, with `renew::renew` as the refresh.
//!
//! Use this for the bearer routes (`/v1/ai/*`, `/v1/places/*`, `/v1/exa/*`,
//! `/v1/unsplash/*`). The legacy `with_*_auth` header helpers remain for
//! the old `/v1/services/*` routes until every caller migrates.
//!
//! ## Purpose tagging (two-pool wallet model)
//!
//! Every charged call carries an `X-Virtues-Purpose` header so the server
//! can route the debit to either the OS reserve or the chat wallet (see
//! `services/virtues-api/src/entitlement.rs` for the routing semantics).
//!
//! Default is [`Purpose::User`] — anything user-initiated (chat, agent
//! loops, on-demand search, places autocomplete) stays as-is. For
//! background/system work (nightly summaries, entity resolution,
//! transcription, embeddings indexing, compaction) call
//! `BearerClient::from_env(pool).with_purpose(Purpose::System)` so the
//! charge hits the protected OS reserve and chat budget stays untouched.

use anyhow::{anyhow, Result};
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::PgPool;

use super::renew;

/// Convert an `AutoTopupOutcome` non-Funded variant into the same 402
/// error shape virtues-api would have returned. Lets iOS handle every
/// error via one contract regardless of whether the box did recovery.
fn synthesize_topup_failure(outcome: renew::AutoTopupOutcome) -> ApiResponse {
    let (code, extra) = match &outcome {
        renew::AutoTopupOutcome::CardDeclined { stripe_code, message } => (
            "card_declined",
            json!({ "stripe_code": stripe_code, "message": message }),
        ),
        renew::AutoTopupOutcome::AuthenticationRequired { payment_intent } => (
            "authentication_required",
            json!({ "payment_intent": payment_intent }),
        ),
        renew::AutoTopupOutcome::MonthlyCapReached { cap_micros, charged_micros } => (
            "monthly_cap_reached",
            json!({
                "monthly_cap_micros": cap_micros,
                "monthly_charges_micros": charged_micros,
            }),
        ),
        renew::AutoTopupOutcome::SubscriptionInactive => (
            "subscription_inactive",
            json!({ "message": "subscription is not active" }),
        ),
        // Funded shouldn't reach here.
        renew::AutoTopupOutcome::Funded { .. } => (
            "internal",
            json!({ "message": "unexpected Funded in synthesize" }),
        ),
    };
    let mut error = json!({ "code": code });
    if let Some(obj) = extra.as_object() {
        for (k, v) in obj {
            error[k] = v.clone();
        }
    }
    ApiResponse {
        status: 402,
        body: json!({ "error": error }),
    }
}

/// Synthesize the same 402 shape used for top-up failures, but with code
/// `topup_disabled`. Bubbles up when the user has turned auto-top-up off
/// or the breaker tripped after 3 consecutive failures in 24h.
fn synthesize_topup_disabled() -> ApiResponse {
    ApiResponse {
        status: 402,
        body: json!({
            "error": {
                "code": "topup_disabled",
                "message":
                    "auto-top-up is disabled (either by you or by the \
                     3-failure circuit breaker). Top up manually from \
                     Settings → Billing, or set a BYO provider key.",
            }
        }),
    }
}

/// Check whether the box should attempt auto-top-up for the next 402.
/// False when the user has explicitly disabled it, or when the
/// `auto_topup_failures_24h` counter has reached the breaker threshold.
async fn auto_topup_allowed(pool: &sqlx::PgPool) -> bool {
    let row: Option<(bool, i32)> = sqlx::query_as(
        "SELECT auto_topup_enabled, auto_topup_failures_24h \
         FROM app_user_profile \
         WHERE id = '00000000-0000-0000-0000-000000000001'",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    match row {
        Some((enabled, failures)) => enabled && failures < AUTO_TOPUP_FAILURE_THRESHOLD,
        // Singleton row should always exist; if it somehow doesn't, default
        // to allowed (matches the column default).
        None => true,
    }
}

/// Reset the failure counter to zero. Called after a successful top-up so
/// transient blips don't accumulate into a breaker trip across days.
async fn record_topup_success(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE app_user_profile \
         SET auto_topup_failures_24h = 0, \
             auto_topup_disabled_at = NULL \
         WHERE id = '00000000-0000-0000-0000-000000000001'",
    )
    .execute(pool)
    .await
    .map(|_| ())
}

/// Increment the failure counter; if we hit the breaker threshold, flip
/// `auto_topup_enabled = FALSE` and record the timestamp. The user can
/// re-enable from `/settings/billing` after fixing whatever was wrong
/// (declined card, payment method, etc).
async fn record_topup_failure(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE app_user_profile \
         SET auto_topup_failures_24h = auto_topup_failures_24h + 1, \
             auto_topup_enabled = CASE \
                 WHEN auto_topup_failures_24h + 1 >= $1 THEN FALSE \
                 ELSE auto_topup_enabled \
             END, \
             auto_topup_disabled_at = CASE \
                 WHEN auto_topup_failures_24h + 1 >= $1 THEN now() \
                 ELSE auto_topup_disabled_at \
             END \
         WHERE id = '00000000-0000-0000-0000-000000000001'",
    )
    .bind(AUTO_TOPUP_FAILURE_THRESHOLD)
    .execute(pool)
    .await
    .map(|_| ())
}

/// How many consecutive auto-top-up failures we tolerate before the
/// breaker trips and auto-top-up is disabled. The counter is reset by a
/// successful top-up OR by the sweeper rolling the daily window.
const AUTO_TOPUP_FAILURE_THRESHOLD: i32 = 3;

/// A fully-read virtues-api response: status + parsed JSON body.
pub struct ApiResponse {
    pub status: u16,
    pub body: Value,
}

impl ApiResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// Outcome of opening a streaming request. On success the body is left
/// untouched so the caller can read the SSE stream; on a non-2xx the error
/// body is already drained into `body` (we can't both peek and stream).
pub enum StreamOutcome {
    Stream(reqwest::Response),
    Error { status: u16, body: String },
}

/// What the call is for — drives `X-Virtues-Purpose` and which pool gets
/// debited server-side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    /// User-initiated work (chat, agent loops, on-demand search). Debits
    /// `wallet_chat_micros` only; never touches the OS reserve.
    User,
    /// Box-essential background work (transcription, ER, summaries,
    /// event summaries, embeddings indexing). Debits `os_reserve_micros`
    /// first; falls back to the chat wallet only if the OS reserve is
    /// exhausted (heavy-OS users subsidize the OS from their chat credit;
    /// light users never notice).
    System,
}

impl Purpose {
    fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::System => "system",
        }
    }
}

#[derive(Clone, Debug)]
pub struct BearerClient {
    http: reqwest::Client,
    stream_http: reqwest::Client,
    pool: PgPool,
    api_url: String,
    atlas_url: String,
    purpose: Purpose,
}

impl BearerClient {
    pub fn from_env(pool: PgPool) -> Self {
        let api_url =
            std::env::var("VIRTUES_API_URL").unwrap_or_else(|_| "http://localhost:9002".into());
        let atlas_url =
            std::env::var("VIRTUES_ATLAS_URL").unwrap_or_else(|_| "http://localhost:9100".into());
        Self {
            http: crate::http_client::virtues_api_client(),
            stream_http: crate::http_client::virtues_api_streaming_client(),
            pool,
            api_url,
            atlas_url,
            // Safer default: user calls 402 on chat exhaustion, OS reserve
            // is never raided. Background callers must explicitly opt into
            // System via `.with_purpose(Purpose::System)`.
            purpose: Purpose::User,
        }
    }

    /// Override the call purpose. Use `Purpose::System` for background
    /// box-essential work (nightly summaries, entity resolution,
    /// transcription, embeddings indexing) so charges hit the protected
    /// OS reserve and the user's chat budget is preserved.
    pub fn with_purpose(mut self, purpose: Purpose) -> Self {
        self.purpose = purpose;
        self
    }

    /// POST JSON to a virtues-api bearer route. Two automatic recoveries:
    ///   * `bearer_expired` (402) → run the voucher renewal, retry once.
    ///   * `insufficient_budget` (402) → trigger auto-top-up via atlas
    ///     (`/credits/auto-topup`), redeem the resulting voucher onto the
    ///     existing bearer, retry once. Surfaces typed errors back to the
    ///     caller for `card_declined`, `monthly_cap_reached`, etc — these
    ///     are not retryable from the box's side.
    pub async fn post_json(&self, path: &str, body: &Value) -> Result<ApiResponse> {
        let bearer = self.ensure_bearer().await?;
        let resp = self.send(path, body, &bearer).await?;
        self.handle_402_and_retry_post(path, body, resp).await
    }

    /// GET a virtues-api bearer route. Same recovery semantics as `post_json`.
    pub async fn get_json(&self, path: &str) -> Result<ApiResponse> {
        let bearer = self.ensure_bearer().await?;
        let resp = self.send_get(path, &bearer).await?;
        self.handle_402_and_retry_get(path, resp).await
    }

    async fn handle_402_and_retry_post(
        &self,
        path: &str,
        body: &Value,
        resp: ApiResponse,
    ) -> Result<ApiResponse> {
        if resp.status != 402 {
            return Ok(resp);
        }
        let code = resp.body["error"]["code"].as_str().unwrap_or("");
        match code {
            "bearer_expired" => {
                let fresh = renew::renew(&self.pool, &self.http, &self.atlas_url, &self.api_url)
                    .await?;
                self.send(path, body, &fresh.bearer).await
            }
            "insufficient_budget" | "wallet_empty" => {
                self.auto_topup_and_retry_post(path, body).await
            }
            _ => Ok(resp), // daily_cap_reached, call_too_expensive, etc — surface
        }
    }

    async fn handle_402_and_retry_get(
        &self,
        path: &str,
        resp: ApiResponse,
    ) -> Result<ApiResponse> {
        if resp.status != 402 {
            return Ok(resp);
        }
        let code = resp.body["error"]["code"].as_str().unwrap_or("");
        match code {
            "bearer_expired" => {
                let fresh = renew::renew(&self.pool, &self.http, &self.atlas_url, &self.api_url)
                    .await?;
                self.send_get(path, &fresh.bearer).await
            }
            "insufficient_budget" | "wallet_empty" => {
                self.auto_topup_and_retry_get(path).await
            }
            _ => Ok(resp),
        }
    }

    /// Trigger auto-top-up via atlas, then retry the failed POST. On a
    /// non-`Funded` outcome (card declined, monthly cap reached, etc) we
    /// return a synthetic 402 response with the same shape virtues-api
    /// would have returned, so the iOS app gets a unified error contract.
    ///
    /// **Circuit breaker:** if auto-top-up is disabled by the user OR has
    /// failed 3 times in the last 24h (counter persisted on
    /// `app_user_profile`), we skip the call entirely and bubble the
    /// original 402. The user must intervene (top up manually, fix card,
    /// or switch to BYO key).
    async fn auto_topup_and_retry_post(
        &self,
        path: &str,
        body: &Value,
    ) -> Result<ApiResponse> {
        if !auto_topup_allowed(&self.pool).await {
            return Ok(synthesize_topup_disabled());
        }
        match renew::auto_topup(&self.pool, &self.http, &self.atlas_url, &self.api_url).await? {
            renew::AutoTopupOutcome::Funded { wallet_micros } => {
                let _ = record_topup_success(&self.pool).await;
                tracing::info!(wallet_micros, "auto-top-up funded; retrying request");
                let bearer = self.ensure_bearer().await?;
                self.send(path, body, &bearer).await
            }
            other => {
                let _ = record_topup_failure(&self.pool).await;
                Ok(synthesize_topup_failure(other))
            }
        }
    }

    async fn auto_topup_and_retry_get(&self, path: &str) -> Result<ApiResponse> {
        if !auto_topup_allowed(&self.pool).await {
            return Ok(synthesize_topup_disabled());
        }
        match renew::auto_topup(&self.pool, &self.http, &self.atlas_url, &self.api_url).await? {
            renew::AutoTopupOutcome::Funded { wallet_micros } => {
                let _ = record_topup_success(&self.pool).await;
                tracing::info!(wallet_micros, "auto-top-up funded; retrying GET");
                let bearer = self.ensure_bearer().await?;
                self.send_get(path, &bearer).await
            }
            other => {
                let _ = record_topup_failure(&self.pool).await;
                Ok(synthesize_topup_failure(other))
            }
        }
    }

    /// Open a streaming POST to a virtues-api bearer route. Renewal/top-up
    /// can only happen *before* the body starts flowing (mid-stream
    /// recovery is impossible), so we ensure a valid bearer up front and,
    /// if the server still answers 402 at connect time, recover-then-retry
    /// once. On success the response body is returned untouched for the
    /// caller to stream; non-recoverable 402s (daily_cap_reached,
    /// card_declined, etc) come back as `StreamOutcome::Error` with the
    /// drained body.
    pub async fn stream(&self, path: &str, body: &Value) -> Result<StreamOutcome> {
        // BYO key escape hatch. When the user has set their own provider
        // key, every chat call goes box → upstream directly. virtues-api
        // (wallet, markup, renewal, caps) is bypassed entirely — that's
        // the point of "bring your own key": Virtues is no longer in the
        // inference path.
        if let Ok(Some(byo)) = crate::api::settings_byo::load_byo_credential(&self.pool).await {
            return self.stream_direct_upstream(body, &byo).await;
        }

        let bearer = self.ensure_bearer().await?;
        let resp = self.send_stream(path, body, &bearer).await?;

        if resp.status().as_u16() == 402 {
            let err_body = resp.text().await.unwrap_or_default();
            if err_body.contains("bearer_expired") {
                let fresh =
                    renew::renew(&self.pool, &self.http, &self.atlas_url, &self.api_url).await?;
                let retry = self.send_stream(path, body, &fresh.bearer).await?;
                return Ok(Self::classify_stream(retry).await);
            }
            if err_body.contains("insufficient_budget") || err_body.contains("wallet_empty") {
                match renew::auto_topup(&self.pool, &self.http, &self.atlas_url, &self.api_url)
                    .await?
                {
                    renew::AutoTopupOutcome::Funded { wallet_micros } => {
                        tracing::info!(wallet_micros, "auto-top-up funded; retrying stream");
                        let bearer = self.ensure_bearer().await?;
                        let retry = self.send_stream(path, body, &bearer).await?;
                        return Ok(Self::classify_stream(retry).await);
                    }
                    other => {
                        let synth = synthesize_topup_failure(other);
                        return Ok(StreamOutcome::Error {
                            status: synth.status,
                            body: synth.body.to_string(),
                        });
                    }
                }
            }
            return Ok(StreamOutcome::Error { status: 402, body: err_body });
        }
        Ok(Self::classify_stream(resp).await)
    }

    async fn classify_stream(resp: reqwest::Response) -> StreamOutcome {
        let status = resp.status().as_u16();
        if (200..300).contains(&status) {
            StreamOutcome::Stream(resp)
        } else {
            let body = resp.text().await.unwrap_or_default();
            StreamOutcome::Error { status, body }
        }
    }

    async fn send_stream(
        &self,
        path: &str,
        body: &Value,
        bearer: &str,
    ) -> Result<reqwest::Response> {
        self.stream_http
            .post(format!("{}{}", self.api_url.trim_end_matches('/'), path))
            .header("Authorization", format!("Bearer {}", bearer))
            .header("X-Virtues-Purpose", self.purpose.as_str())
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .map_err(|e| anyhow!("virtues-api stream request failed: {e}"))
    }

    /// BYO path: POST the chat body to the user's configured upstream
    /// endpoint with their API key. v1 supports OpenAI-compatible APIs
    /// (OpenAI, xAI, OpenRouter, LiteLLM, custom proxies). For
    /// Anthropic-native or Google-native body shapes the user points BYO
    /// at a translation proxy (LiteLLM / OpenRouter) — keeps the per-
    /// provider request-shape mess out of our codebase.
    ///
    /// If `default_model` is set on the credential AND the request body
    /// doesn't already pin a model, we splice it in so the user's choice
    /// "from Settings" wins over the agent's default.
    async fn stream_direct_upstream(
        &self,
        body: &Value,
        byo: &crate::api::settings_byo::ByoCredential,
    ) -> Result<StreamOutcome> {
        let mut body = body.clone();
        if let (Some(model), Value::Object(map)) = (byo.default_model.as_deref(), &mut body) {
            if !map.contains_key("model") {
                map.insert("model".to_string(), Value::String(model.to_string()));
            }
        }
        let resp = self
            .stream_http
            .post(&byo.endpoint_url)
            .header("Authorization", format!("Bearer {}", byo.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow!("BYO upstream request failed: {e}"))?;
        Ok(Self::classify_stream(resp).await)
    }

    /// Return a usable bearer: the current one if still valid, otherwise
    /// renew. Renewal requires a previously-claimed billing token.
    ///
    /// Dev override: when `VIRTUES_API_BEARER` is set we present it verbatim
    /// and skip the vault/renew path entirely. This pairs with the gated
    /// seed in virtues-api (`ENVIRONMENT=dev`), which funds an entitlement
    /// keyed by `sha256(VIRTUES_API_BEARER)` — so a local virtues-api accepts
    /// our calls without a real subscription. Unset in prod → no effect.
    async fn ensure_bearer(&self) -> Result<String> {
        if let Ok(bearer) = std::env::var("VIRTUES_API_BEARER") {
            if !bearer.is_empty() {
                return Ok(bearer);
            }
        }
        match renew::current_bearer(&self.pool).await? {
            Some((bearer, Some(exp))) if exp > Utc::now() => Ok(bearer),
            _ => {
                let fresh =
                    renew::renew(&self.pool, &self.http, &self.atlas_url, &self.api_url).await?;
                Ok(fresh.bearer)
            }
        }
    }

    async fn send(&self, path: &str, body: &Value, bearer: &str) -> Result<ApiResponse> {
        let resp = self
            .http
            .post(format!("{}{}", self.api_url.trim_end_matches('/'), path))
            .header("Authorization", format!("Bearer {}", bearer))
            .header("X-Virtues-Purpose", self.purpose.as_str())
            .json(body)
            .send()
            .await
            .map_err(|e| anyhow!("virtues-api request failed: {e}"))?;
        let status = resp.status().as_u16();
        let body = resp.json::<Value>().await.unwrap_or_else(|_| json!({}));
        Ok(ApiResponse { status, body })
    }

    async fn send_get(&self, path: &str, bearer: &str) -> Result<ApiResponse> {
        let resp = self
            .http
            .get(format!("{}{}", self.api_url.trim_end_matches('/'), path))
            .header("Authorization", format!("Bearer {}", bearer))
            .header("X-Virtues-Purpose", self.purpose.as_str())
            .send()
            .await
            .map_err(|e| anyhow!("virtues-api request failed: {e}"))?;
        let status = resp.status().as_u16();
        let body = resp.json::<Value>().await.unwrap_or_else(|_| json!({}));
        Ok(ApiResponse { status, body })
    }
}

//! Stripe Customer Portal route (billing self-service).
//!
//! `POST /billing/portal/sessions { api_key, return_url }` →
//! `{ url }`. Backs core's `POST /api/billing/portal`. The customer manages
//! their card, invoices, and cancellation entirely on Stripe's hosted portal,
//! so Atlas implements no billing UI of its own.
//!
//! Privacy: same wall as the other api_key routes. We resolve
//! api_key → customer to mint the portal session, and learn nothing
//! about the box's usage. The portal is a billing-plane concern; no api_key or wallet detail

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::post,
    Router,
};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::routes::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/billing/portal/sessions", post(create_portal_session))
}

#[derive(Debug, Deserialize)]
struct PortalSessionBody {
    api_key: String,
    /// Where Stripe sends the customer when they leave the portal. Core
    /// supplies the box's own URL; we fall back to atlas's public URL.
    #[serde(default)]
    return_url: Option<String>,
}

async fn create_portal_session(
    State(state): State<AppState>,
    Json(body): Json<PortalSessionBody>,
) -> axum::response::Response {
    let customer_id = match resolve_active_customer(&state, &body.api_key).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    let return_url = body
        .return_url
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| format!("{}/billing/done", state.public_url.trim_end_matches('/')));

    let session = match state
        .stripe
        .create_billing_portal_session(&customer_id, &return_url)
        .await
    {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("billing portal session create failed: {e:#}");
            return err(
                StatusCode::BAD_GATEWAY,
                "stripe_error",
                "could not open the billing portal",
            );
        }
    };

    (StatusCode::OK, Json(json!({ "url": session.url }))).into_response()
}

/// Resolve a api_key → active customer's `stripe_customer_id`. Errors on
/// unknown token or inactive subscription. Mirrors `credits::resolve_active_customer`.
async fn resolve_active_customer(
    state: &AppState,
    api_key: &str,
) -> Result<String, axum::response::Response> {
    let token_hash = sha256(api_key.as_bytes());

    // Per-box keys first, legacy column fallback — via the shared lookup.
    let cid = super::claim::customer_id_by_key_hash(&state.pool, &token_hash[..])
        .await
        .map_err(|e| {
            tracing::warn!("key lookup failed: {e:#}");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "customer lookup failed",
            )
        })?;
    let Some(cid) = cid else {
        return Err(err(
            StatusCode::UNAUTHORIZED,
            "invalid_api_key",
            "unknown api key",
        ));
    };

    let row: Option<(String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT c.stripe_customer_id,
               (SELECT s.status FROM subscriptions s
                WHERE s.stripe_customer_id = c.stripe_customer_id
                ORDER BY s.current_period_end DESC NULLS LAST
                LIMIT 1) AS sub_status
        FROM customers c
        WHERE c.stripe_customer_id = $1
        "#,
    )
    .bind(&cid)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("customer lookup failed: {e:#}");
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "customer lookup failed",
        )
    })?;

    let Some((customer_id, sub_status)) = row else {
        return Err(err(
            StatusCode::UNAUTHORIZED,
            "invalid_api_key",
            "unknown api key",
        ));
    };

    if sub_status.as_deref() != Some("active") {
        return Err(err(
            StatusCode::PAYMENT_REQUIRED,
            "subscription_inactive",
            "subscription is not active",
        ));
    }

    Ok(customer_id)
}

fn err(status: StatusCode, code: &str, message: &str) -> axum::response::Response {
    (
        status,
        Json(json!({ "error": { "code": code, "message": message } })),
    )
        .into_response()
}

fn sha256(data: &[u8]) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().to_vec()
}

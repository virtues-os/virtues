//! Stripe webhook handler.
//!
//! `POST /webhooks/stripe`
//!
//! In the voucher model the webhook's only job is to keep Atlas's own
//! `subscriptions` table current. It makes NO calls to virtues-api —
//! revocation happens by expiry: a canceled subscription simply stops
//! producing vouchers, and the device's bearer runs out.
//!
//!   1. Verify the `Stripe-Signature` HMAC.
//!   2. Idempotency via `stripe_webhook_events`.
//!   3. Update `subscriptions.status` / `current_period_end`.

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
    routing::post,
    Router,
};
use chrono::{TimeZone, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use virtues_helpers::crypto::{verify_stripe_signature, StripeWebhookError};

use crate::routes::AppState;

const STRIPE_TIMESTAMP_TOLERANCE_SECONDS: i64 = 300;

pub fn router() -> Router<AppState> {
    Router::new().route("/webhooks/stripe", post(handle_webhook))
}

#[derive(Debug, Deserialize)]
struct StripeEvent {
    id: String,
    #[serde(rename = "type")]
    event_type: String,
    data: StripeEventData,
}

#[derive(Debug, Deserialize)]
struct StripeEventData {
    object: Value,
}

async fn handle_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    if state.stripe_webhook_secret.is_empty() {
        return err(StatusCode::SERVICE_UNAVAILABLE, "stripe_not_configured", "STRIPE_WEBHOOK_SECRET not set");
    }
    let Some(sig) = headers.get("stripe-signature").and_then(|v| v.to_str().ok()) else {
        return err(StatusCode::BAD_REQUEST, "missing_signature", "Stripe-Signature header missing");
    };
    if let Err(e) = verify_stripe_signature(&body, sig, &state.stripe_webhook_secret, STRIPE_TIMESTAMP_TOLERANCE_SECONDS) {
        return err(StatusCode::UNAUTHORIZED, stripe_err_code(&e), &e.to_string());
    }

    let event: StripeEvent = match serde_json::from_slice(&body) {
        Ok(e) => e,
        Err(e) => return err(StatusCode::BAD_REQUEST, "invalid_event_json", &e.to_string()),
    };

    // Idempotency.
    match record_event(&state.pool, &event.id, &event.event_type).await {
        Ok(true) => {}
        Ok(false) => {
            return (StatusCode::OK, Json(json!({ "duplicate": true }))).into_response();
        }
        Err(e) => {
            tracing::warn!("idempotency insert failed: {e:#}");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "idempotency insert failed");
        }
    }

    let result = match event.event_type.as_str() {
        "customer.subscription.updated" => set_period_and_status(&state.pool, &event.data.object, None, None).await,
        "customer.subscription.deleted" => set_status(&state.pool, &event.data.object, "canceled").await,
        // M4: `invoice.paid`'s event object has no top-level `current_period_end`
        // (that lives on the *subscription*). Without retrieving the
        // subscription, renewals silently leave the period in the past and
        // `/voucher` denies a *paying* customer. Pull the period from the sub.
        "invoice.paid" => {
            let mut explicit = None;
            if let Some(sid) = event.data.object.get("subscription").and_then(|v| v.as_str()) {
                match state.stripe.retrieve_subscription(sid).await {
                    Ok(sub) => explicit = Some(sub.period_end()),
                    Err(e) => tracing::warn!(subscription_id = %sid, "retrieve_subscription on invoice.paid failed: {e:#}"),
                }
            }
            let r = set_period_and_status(&state.pool, &event.data.object, Some("active"), explicit).await;
            // Renewal: SET the wallet to the monthly allotment. If the credit
            // fails (transient virtues-api blip), propagate the error so the
            // webhook 500s and Stripe retries (the idempotency row is released
            // above). `renew_wallet` returns Ok when the customer hasn't claimed
            // yet, so the first-invoice-before-claim race doesn't retry forever
            // (claim funds the wallet itself).
            match r {
                Ok(()) => match event.data.object.get("customer").and_then(|v| v.as_str()) {
                    Some(cust) => renew_wallet(&state, cust).await,
                    None => Ok(()),
                },
                Err(e) => Err(e),
            }
        }
        // H3 partial: handle dunning so `/voucher`'s status gate closes on
        // failed renewals (otherwise it keeps minting until period lapses).
        "invoice.payment_failed" => set_status(&state.pool, &event.data.object, "past_due").await,
        // Pre-order deposit settled. No-op for subscription checkouts (those
        // settle via /claim + invoice.paid); only `metadata.type ==
        // "preorder_deposit"` sessions are recorded.
        "checkout.session.completed" => record_preorder(&state, &event.data.object).await,
        "charge.refunded" | "charge.dispute.created" => {
            // Flip any matching pre-order deposit to 'refunded' (no-op for
            // subscription charges).
            let pre = refund_preorder(&state.pool, &event.data.object).await;
            // Subscription refunds flip the sub to 'refunded'. A deposit charge
            // (mode=payment) may carry no customer — skip rather than error so
            // the delivery isn't retried forever.
            let sub = if event.data.object.get("customer").and_then(|v| v.as_str()).is_some() {
                set_status(&state.pool, &event.data.object, "refunded").await
            } else {
                Ok(())
            };
            pre.and(sub)
        }
        other => {
            tracing::debug!(event_type = %other, "stripe webhook ignored");
            Ok(())
        }
    };

    match result {
        Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
        Err(e) => {
            // The idempotency row was recorded BEFORE the handler ran. If the
            // handler failed (e.g. a transient virtues-api blip during a
            // renewal credit), release that row so Stripe's retry re-processes
            // the event instead of seeing a duplicate and skipping it. Every
            // handler is idempotent (UPSERT period / credit `set` / preorder
            // upsert), so re-running on retry is safe.
            let _ = sqlx::query("DELETE FROM stripe_webhook_events WHERE stripe_event_id = $1")
                .bind(&event.id)
                .execute(&state.pool)
                .await;
            tracing::warn!(event_id = %event.id, "handler failed (released for retry): {e:#}");
            err(StatusCode::INTERNAL_SERVER_ERROR, "handler_failed", &e.to_string())
        }
    }
}

/// On subscription renewal (`invoice.paid`), set the account's wallet to the
/// monthly allotment via virtues-api. Looks up the customer's opaque
/// `account_id` + daily cap; the api side overwrites the balance + bumps the
/// cohort expiry (use-it-or-lose-it).
async fn renew_wallet(state: &AppState, stripe_customer_id: &str) -> anyhow::Result<()> {
    let row: Option<(String, i64)> = sqlx::query_as(
        "SELECT account_id, daily_cap_micros FROM customers WHERE stripe_customer_id = $1",
    )
    .bind(stripe_customer_id)
    .fetch_optional(&state.pool)
    .await?;
    let Some((account_id, daily_cap_micros)) = row else {
        // Customer hasn't claimed/linked yet — nothing to credit.
        return Ok(());
    };
    state
        .virtues_api
        .credit(&crate::virtues_api_client::Credit {
            account_id,
            amount_micros: state.voucher.renewal_micros,
            mode: "set",
            daily_cap_micros,
            reference: Some("renewal".to_string()),
        })
        .await
}

async fn record_event(pool: &PgPool, event_id: &str, event_type: &str) -> anyhow::Result<bool> {
    let res = sqlx::query(
        "INSERT INTO stripe_webhook_events (stripe_event_id, event_type) \
         VALUES ($1, $2) ON CONFLICT (stripe_event_id) DO NOTHING",
    )
    .bind(event_id)
    .bind(event_type)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() == 1)
}

/// Record a completed pre-order deposit. Only sessions stamped with
/// `metadata.type == "preorder_deposit"` are handled here; subscription
/// checkouts settle via `/claim` + invoice events and are ignored. Idempotent
/// on the session id (the webhook-event ledger also dedups deliveries).
async fn record_preorder(state: &AppState, object: &Value) -> anyhow::Result<()> {
    let is_deposit = object
        .get("metadata")
        .and_then(|m| m.get("type"))
        .and_then(|v| v.as_str())
        == Some("preorder_deposit");
    if !is_deposit {
        return Ok(());
    }
    // Only record genuinely-paid deposits.
    if object.get("payment_status").and_then(|v| v.as_str()) != Some("paid") {
        return Ok(());
    }

    let session_id = object.get("id").and_then(|v| v.as_str()).unwrap_or_default();
    let payment_intent = object.get("payment_intent").and_then(|v| v.as_str());
    let email = object
        .get("customer_details")
        .and_then(|d| d.get("email"))
        .and_then(|v| v.as_str());
    let amount_total = object.get("amount_total").and_then(|v| v.as_i64());
    let currency = object.get("currency").and_then(|v| v.as_str());

    // Shipping address. Stripe moved this under `collected_information` in
    // newer API versions but still populates the legacy top-level
    // `shipping_details` — try the new location first, fall back to the old.
    // `ship_address` is stored as jsonb (bound as text, cast in SQL, so we
    // don't need sqlx's `json` feature).
    let shipping = object
        .get("collected_information")
        .and_then(|c| c.get("shipping_details"))
        .or_else(|| object.get("shipping_details"));
    let ship_name = shipping
        .and_then(|s| s.get("name"))
        .and_then(|v| v.as_str());
    let ship_address_val = shipping.and_then(|s| s.get("address"));
    let ship_country = ship_address_val
        .and_then(|a| a.get("country"))
        .and_then(|v| v.as_str());
    let ship_address_json = ship_address_val.map(|a| a.to_string());

    let res = sqlx::query(
        r#"
        INSERT INTO preorders
            (stripe_session_id, stripe_payment_intent, email, amount_total, currency,
             ship_name, ship_address, ship_country, status)
        VALUES ($1, $2, $3, $4, $5, $6, $7::jsonb, $8, 'deposit_paid')
        ON CONFLICT (stripe_session_id) DO NOTHING
        "#,
    )
    .bind(session_id)
    .bind(payment_intent)
    .bind(email)
    .bind(amount_total)
    .bind(currency)
    .bind(ship_name)
    .bind(ship_address_json.as_deref())
    .bind(ship_country)
    .execute(&state.pool)
    .await?;

    // Already recorded (replayed delivery that slipped past the event ledger,
    // or a re-sent session): don't insert twice and don't re-send the email.
    if res.rows_affected() == 0 {
        tracing::info!(session_id = %session_id, "preorder deposit already recorded; skipping");
        return Ok(());
    }
    tracing::info!(session_id = %session_id, "preorder deposit recorded");

    // Founder thank-you note — best-effort. A Resend hiccup must not fail the
    // webhook (that would make Stripe retry a delivery whose real work, the
    // INSERT above, already succeeded). Skipped when RESEND_API_KEY is unset.
    if let Some(to) = email {
        if state.resend_api_key.is_empty() {
            tracing::debug!("RESEND_API_KEY unset; skipping preorder thank-you email");
        } else {
            match crate::email::send_preorder_thanks(
                &state.resend_api_key,
                &state.preorder.email_from,
                &state.preorder.email_reply_to,
                to,
            )
            .await
            {
                Ok(()) => tracing::info!(session_id = %session_id, "preorder thank-you email sent"),
                Err(e) => {
                    tracing::warn!(session_id = %session_id, "preorder thank-you email failed: {e:#}")
                }
            }
        }
    } else {
        tracing::warn!(session_id = %session_id, "preorder has no email; skipping thank-you");
    }

    Ok(())
}

/// Flip a pre-order deposit to 'refunded' when its charge is refunded or
/// disputed. Matched on the PaymentIntent; a no-op for non-preorder charges.
async fn refund_preorder(pool: &PgPool, object: &Value) -> anyhow::Result<()> {
    let Some(pi) = object.get("payment_intent").and_then(|v| v.as_str()) else {
        return Ok(());
    };
    sqlx::query("UPDATE preorders SET status = 'refunded' WHERE stripe_payment_intent = $1")
        .bind(pi)
        .execute(pool)
        .await?;
    Ok(())
}

/// Set status by customer id (used for cancel/refund).
async fn set_status(pool: &PgPool, object: &Value, status: &str) -> anyhow::Result<()> {
    let customer_id = extract_customer(object)?;
    sqlx::query("UPDATE subscriptions SET status = $1 WHERE stripe_customer_id = $2")
        .bind(status)
        .bind(&customer_id)
        .execute(pool)
        .await?;
    tracing::info!(customer_id = %customer_id, status, "subscription status updated");
    Ok(())
}

/// Update current_period_end (and optionally status) from the event object.
/// `explicit_period` overrides the object's `current_period_end` — used for
/// `invoice.paid` where the period lives on the subscription, not the invoice.
async fn set_period_and_status(
    pool: &PgPool,
    object: &Value,
    status: Option<&str>,
    explicit_period: Option<chrono::DateTime<Utc>>,
) -> anyhow::Result<()> {
    let customer_id = extract_customer(object)?;
    let period_end = explicit_period.or_else(|| {
        object
            .get("current_period_end")
            .and_then(|v| v.as_i64())
            .and_then(|ts| Utc.timestamp_opt(ts, 0).single())
    });

    match (period_end, status) {
        (Some(end), Some(st)) => {
            sqlx::query(
                "UPDATE subscriptions SET current_period_end = $1, status = $2, updated_at = now() \
                 WHERE stripe_customer_id = $3",
            )
            .bind(end)
            .bind(st)
            .bind(&customer_id)
            .execute(pool)
            .await?;
        }
        (Some(end), None) => {
            sqlx::query(
                "UPDATE subscriptions SET current_period_end = $1, updated_at = now() \
                 WHERE stripe_customer_id = $2",
            )
            .bind(end)
            .bind(&customer_id)
            .execute(pool)
            .await?;
        }
        (None, Some(st)) => {
            sqlx::query("UPDATE subscriptions SET status = $1 WHERE stripe_customer_id = $2")
                .bind(st)
                .bind(&customer_id)
                .execute(pool)
                .await?;
        }
        (None, None) => {}
    }
    Ok(())
}

fn extract_customer(object: &Value) -> anyhow::Result<String> {
    object
        .get("customer")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("event object missing 'customer' field"))
}

fn stripe_err_code(e: &StripeWebhookError) -> &'static str {
    match e {
        StripeWebhookError::MalformedHeader => "malformed_signature",
        StripeWebhookError::TimestampOutsideTolerance(_) => "stale_signature",
        StripeWebhookError::SignatureMismatch => "signature_mismatch",
    }
}

fn err(status: StatusCode, code: &str, message: &str) -> axum::response::Response {
    (
        status,
        Json(json!({ "error": { "code": code, "message": message } })),
    )
        .into_response()
}

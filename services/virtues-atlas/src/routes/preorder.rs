//! Pre-order deposit checkout (customer-facing, one-time payment).
//!
//! `POST /preorder/checkout` -> { url }
//!
//! Creates a `mode=payment` Stripe Checkout Session for a fully-refundable
//! deposit and returns the hosted URL for the caller to redirect to. Email and
//! card are collected on Stripe's page; the remaining balance is collected
//! later through a separate "finish your order" flow when the unit ships.
//!
//! The completed deposit is recorded by the webhook handler on
//! `checkout.session.completed` (where `metadata.type == "preorder_deposit"`),
//! so this route holds no DB state of its own.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde_json::json;
use sqlx::PgPool;

use crate::routes::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/preorder/checkout", post(checkout))
        .route("/preorder/session/:id", get(session))
}

async fn checkout(State(state): State<AppState>) -> axum::response::Response {
    if !state.stripe.is_configured() {
        return err(
            StatusCode::SERVICE_UNAVAILABLE,
            "stripe_not_configured",
            "billing is not configured",
        );
    }

    let p = &state.preorder;
    match state
        .stripe
        .create_deposit_checkout_session(
            &p.price_id,
            p.amount_cents,
            &p.currency,
            &p.product_name,
            &p.product_image,
            &p.success_url,
            &p.cancel_url,
            &p.allowed_countries,
        )
        .await
    {
        Ok(session) => (StatusCode::OK, Json(json!({ "url": session.url }))).into_response(),
        Err(e) => {
            tracing::warn!("preorder checkout create failed: {e:#}");
            err(StatusCode::BAD_GATEWAY, "stripe_error", &format!("could not start checkout: {e}"))
        }
    }
}

/// Read-only order lookup for the success page.
///
/// `GET /preorder/session/:id -> { paid, deposit_amount, currency, email, position }`
///
/// The success page is reached with `?session_id=cs_…` (Stripe substitutes it
/// into the success URL). That id is unguessable, so it doubles as the access
/// token — we only ever return a session stamped `metadata.type ==
/// "preorder_deposit"`, and only the handful of fields the page shows (all of
/// which belong to the person who just paid). This endpoint never writes:
/// recording the deposit and sending the thank-you email are the webhook's job,
/// so landing here before the webhook fires can't double-send or skip the mail.
async fn session(State(state): State<AppState>, Path(id): Path<String>) -> axum::response::Response {
    if !state.stripe.is_configured() {
        return err(
            StatusCode::SERVICE_UNAVAILABLE,
            "stripe_not_configured",
            "billing is not configured",
        );
    }
    // Cheap shape guard before spending a Stripe round-trip.
    if !id.starts_with("cs_")
        || id.len() > 100
        || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return err(StatusCode::BAD_REQUEST, "bad_session", "invalid session id");
    }

    let obj = match state.stripe.retrieve_checkout_session_raw(&id).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("preorder session lookup failed: {e:#}");
            return err(StatusCode::BAD_GATEWAY, "stripe_error", "could not load order");
        }
    };

    // Only ever expose our own pre-order deposit sessions.
    let is_deposit = obj
        .get("metadata")
        .and_then(|m| m.get("type"))
        .and_then(|v| v.as_str())
        == Some("preorder_deposit");
    if !is_deposit {
        return err(StatusCode::NOT_FOUND, "not_found", "no such order");
    }

    let paid = obj.get("payment_status").and_then(|v| v.as_str()) == Some("paid");
    let amount = obj.get("amount_total").and_then(|v| v.as_i64());
    let currency = obj.get("currency").and_then(|v| v.as_str());
    let email = obj
        .get("customer_details")
        .and_then(|d| d.get("email"))
        .and_then(|v| v.as_str());
    let position = if paid {
        preorder_position(&state.pool, &id).await.ok()
    } else {
        None
    };

    (
        StatusCode::OK,
        Json(json!({
            "paid": paid,
            "deposit_amount": amount,
            "currency": currency,
            "email": email,
            "position": position,
        })),
    )
        .into_response()
}

/// Rank this deposit among all paid deposits by recording time. If the webhook
/// hasn't recorded the row yet (the success redirect can beat it), fall back to
/// "newest in line" = current paid count + 1.
async fn preorder_position(pool: &PgPool, session_id: &str) -> anyhow::Result<i64> {
    let their_created: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT created_at FROM preorders WHERE stripe_session_id = $1")
            .bind(session_id)
            .fetch_optional(pool)
            .await?;
    let pos: i64 = match their_created {
        Some(ts) => {
            sqlx::query_scalar(
                "SELECT count(*) FROM preorders WHERE status = 'deposit_paid' AND created_at <= $1",
            )
            .bind(ts)
            .fetch_one(pool)
            .await?
        }
        None => {
            let total: i64 =
                sqlx::query_scalar("SELECT count(*) FROM preorders WHERE status = 'deposit_paid'")
                    .fetch_one(pool)
                    .await?;
            total + 1
        }
    };
    Ok(pos)
}

fn err(status: StatusCode, code: &str, message: &str) -> axum::response::Response {
    (
        status,
        Json(json!({ "error": { "code": code, "message": message } })),
    )
        .into_response()
}

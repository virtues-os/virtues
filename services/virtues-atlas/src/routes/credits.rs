//! Credit top-up routes (linked prepaid model).
//!
//! Two device-facing endpoints, both authed via the device `api_key`:
//!
//! - `POST /credits/auto-topup` — box hit a 402 with `wallet_empty`. We
//!   charge the saved card off-session (`AUTO_TOPUP_MICROS`, default $20),
//!   then credit the account's wallet in virtues-api (`/internal/credit` mode
//!   `add`).
//!
//! The wallet gets the charge minus the card fee ([`credit_after_fee`]), so
//! a top-up never costs Virtues money and usage can stay at cost.
//! - `POST /credits/topup { amount_micros }` — user explicitly tapped
//!   "Add credit" with a chosen amount in `$10–$50`. Same flow.
//!
//! Both routes enforce:
//!   1. Active subscription (no auto-charge if sub canceled/past_due).
//!   2. Monthly cap (`customers.monthly_cap_micros`): refuse if this month's
//!      total charges + new amount would exceed it.
//!   3. Amount band ($10 ≤ amount ≤ $50 for manual; fixed $10 for auto).
//!
//! Order matters: we reserve the amount against the cap, charge the card,
//! then credit the wallet keyed by the opaque `account_id`. The charge carries
//! a Stripe idempotency key, so a box that retries after a timeout gets the
//! first PaymentIntent back rather than a second charge, and `topup_charges`
//! records which PaymentIntents were credited, so a retry credits once. A
//! credit that fails after the charge leaves the PaymentIntent uncredited,
//! and the box's retry credits it.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::post,
    Router,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::routes::AppState;
use crate::stripe_api::OffSessionChargeError;
use crate::virtues_api_client::Credit;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/credits/auto-topup", post(auto_topup))
        .route("/credits/topup", post(manual_topup))
}

#[derive(Debug, Deserialize)]
struct AutoTopupBody {
    api_key: String,
}

#[derive(Debug, Deserialize)]
struct ManualTopupBody {
    api_key: String,
    amount_micros: i64,
}

/// Box-triggered auto-top-up. Fired when virtues-api returned 402 with
/// `wallet_empty`. Fixed amount (`state.credit.auto_topup_micros`,
/// default $10). On success, credits the wallet and returns `{ok}`.
async fn auto_topup(
    State(state): State<AppState>,
    Json(body): Json<AutoTopupBody>,
) -> axum::response::Response {
    let amount = state.credit.auto_topup_micros;
    let (customer_id, account_id) = match resolve_active_customer(&state, &body.api_key).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    // The owner can turn auto top-up off in Settings. The box checks this
    // too, but atlas charges the card, so atlas must honor it.
    match sqlx::query_scalar::<_, bool>(
        "SELECT auto_topup_enabled FROM customers WHERE stripe_customer_id = $1",
    )
    .bind(&customer_id)
    .fetch_one(&state.pool)
    .await
    {
        Ok(true) => {}
        Ok(false) => {
            return err(StatusCode::PAYMENT_REQUIRED, "auto_topup_disabled", "auto top-up is turned off");
        }
        Err(e) => {
            tracing::warn!("auto top-up setting read failed: {e:#}");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "settings lookup failed");
        }
    }

    if let Err(resp) = reserve_monthly_cap(&state.pool, &customer_id, amount).await {
        return resp;
    }

    do_topup(&state, &customer_id, &account_id, amount, "Virtues auto top-up", true).await
}

/// User-initiated top-up from iOS. Amount must lie in
/// [`state.credit.topup_min_micros`, `state.credit.topup_max_micros`]
/// (defaults $10–$50). Same Stripe flow as auto-top-up.
async fn manual_topup(
    State(state): State<AppState>,
    Json(body): Json<ManualTopupBody>,
) -> axum::response::Response {
    let min = state.credit.topup_min_micros;
    let max = state.credit.topup_max_micros;
    if body.amount_micros < min || body.amount_micros > max {
        return err(
            StatusCode::BAD_REQUEST,
            "amount_out_of_band",
            &format!("amount_micros must be between {min} and {max}"),
        );
    }

    let (customer_id, account_id) = match resolve_active_customer(&state, &body.api_key).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    if let Err(resp) = reserve_monthly_cap(&state.pool, &customer_id, body.amount_micros).await {
        return resp;
    }

    do_topup(&state, &customer_id, &account_id, body.amount_micros, "Virtues credit", false).await
}

/// The Stripe idempotency key for a top-up: the same customer, amount and
/// kind within one ten-minute window share a key. A box retrying a top-up
/// whose first attempt timed out after the charge therefore gets that first
/// PaymentIntent back. The cost is that two identical top-ups inside ten
/// minutes are one charge.
fn topup_idempotency_key(customer_id: &str, amount_micros: i64, is_auto: bool, now_secs: i64) -> String {
    let kind = if is_auto { "auto" } else { "manual" };
    format!("topup:{kind}:{customer_id}:{amount_micros}:{}", now_secs.div_euclid(600))
}

/// Shared body: the amount is already reserved against the cap. Charge the
/// saved card off-session, then credit the wallet once per PaymentIntent.
async fn do_topup(
    state: &AppState,
    customer_id: &str,
    account_id: &str,
    amount_micros: i64,
    description: &str,
    is_auto: bool,
) -> axum::response::Response {
    let key = topup_idempotency_key(customer_id, amount_micros, is_auto, Utc::now().timestamp());
    let credited = credit_after_fee(amount_micros, &state.credit);
    let pi_id = match state
        .stripe
        .charge_off_session(customer_id, amount_micros, description, &key, account_id, credited)
        .await
    {
        Ok(pi) => pi,
        Err(OffSessionChargeError::StripeDeclined { code, message }) => {
            tracing::warn!(customer = customer_id, %code, "off-session charge declined");
            release_monthly_cap(&state.pool, customer_id, amount_micros).await;
            return (
                StatusCode::PAYMENT_REQUIRED,
                Json(json!({
                    "error": {
                        "code": "card_declined",
                        "stripe_code": code,
                        "message": message,
                    }
                })),
            )
                .into_response();
        }
        Err(OffSessionChargeError::AuthenticationRequired(pi)) => {
            // 3-D Secure required: nothing is charged yet. If the owner
            // confirms it later, the `payment_intent.succeeded` webhook
            // credits it (credit_topup_once), and counts it toward the cap.
            release_monthly_cap(&state.pool, customer_id, amount_micros).await;
            return (
                StatusCode::PAYMENT_REQUIRED,
                Json(json!({
                    "error": {
                        "code": "authentication_required",
                        "payment_intent": pi,
                        "message": "card requires authentication; confirm in app",
                    }
                })),
            )
                .into_response();
        }
        Err(OffSessionChargeError::InvalidAmount) => {
            release_monthly_cap(&state.pool, customer_id, amount_micros).await;
            return err(StatusCode::BAD_REQUEST, "invalid_amount", "amount must be positive");
        }
        // Network, parse or unexpected status: Stripe may have charged. The
        // reservation stays (the cap errs toward counting it), and the box's
        // retry reuses the idempotency key.
        Err(e) => {
            tracing::warn!(error = %e, "off-session charge failed");
            return err(StatusCode::BAD_GATEWAY, "stripe_error", &e.to_string());
        }
    };

    match credit_topup_once(state, &pi_id, customer_id, account_id, amount_micros, credited, false).await {
        Ok(true) => {}
        // A repeat of a top-up that already finished got the same
        // PaymentIntent from Stripe; its own reservation is returned.
        Ok(false) => {
            release_monthly_cap(&state.pool, customer_id, amount_micros).await;
            return (
                StatusCode::OK,
                Json(json!({ "ok": true, "amount_micros": amount_micros, "already_credited": true })),
            )
                .into_response();
        }
        Err(()) => {
            return err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "credit_failed",
                "credit failed after charge; try again",
            );
        }
    }

    if is_auto {
        if let Err(e) = sqlx::query("UPDATE customers SET last_auto_topup_at = now() WHERE stripe_customer_id = $1")
            .bind(customer_id)
            .execute(&state.pool)
            .await
        {
            tracing::warn!("last_auto_topup_at update failed: {e:#}");
        }
    }

    (
        StatusCode::OK,
        Json(json!({ "ok": true, "amount_micros": amount_micros, "credited_micros": credited })),
    )
        .into_response()
}

/// Credit a succeeded top-up PaymentIntent once. `Ok(true)` means this call
/// credited it, `Ok(false)` that it was already credited. On a failed credit
/// the PaymentIntent is forgotten, so the next attempt (the box's retry or
/// Stripe's webhook retry) credits it. `count_toward_cap` adds the charge to
/// the month's spend, for charges whose reservation was not held (the webhook).
pub(crate) async fn credit_topup_once(
    state: &AppState,
    pi_id: &str,
    customer_id: &str,
    account_id: &str,
    charged_micros: i64,
    credit_micros: i64,
    count_toward_cap: bool,
) -> Result<bool, ()> {
    let first = match sqlx::query(
        "INSERT INTO topup_charges (stripe_payment_intent_id, stripe_customer_id, amount_micros) \
         VALUES ($1, $2, $3) ON CONFLICT (stripe_payment_intent_id) DO NOTHING",
    )
    .bind(pi_id)
    .bind(customer_id)
    .bind(charged_micros)
    .execute(&state.pool)
    .await
    {
        Ok(r) => r.rows_affected() == 1,
        Err(e) => {
            tracing::error!(customer = customer_id, payment_intent = %pi_id, "topup_charges insert failed after charge: {e:#}");
            return Err(());
        }
    };
    if !first {
        return Ok(false);
    }
    if let Err(e) = state
        .virtues_api
        .credit(&Credit {
            account_id: account_id.to_string(),
            amount_micros: credit_micros,
            mode: "add",
            reference: Some(format!("topup:{pi_id}")),
        })
        .await
    {
        tracing::error!(
            customer = customer_id,
            payment_intent = %pi_id,
            error = %e,
            "credit with virtues-api failed after Stripe charge; a retry credits it"
        );
        if let Err(e) = sqlx::query("DELETE FROM topup_charges WHERE stripe_payment_intent_id = $1")
            .bind(pi_id)
            .execute(&state.pool)
            .await
        {
            tracing::error!(payment_intent = %pi_id, "could not reopen uncredited top-up, reconcile by hand: {e:#}");
        }
        return Err(());
    }
    if count_toward_cap {
        if let Err(e) = sqlx::query(
            "UPDATE customers SET monthly_charges_micros = monthly_charges_micros + $2 \
             WHERE stripe_customer_id = $1",
        )
        .bind(customer_id)
        .bind(charged_micros)
        .execute(&state.pool)
        .await
        {
            tracing::warn!("monthly spend update failed: {e:#}");
        }
    }
    Ok(true)
}

/// The wallet credit for a top-up of `amount_micros`: the amount Stripe
/// charges (whole cents, rounded up as `charge_off_session` does) minus the
/// card fee, rounded to the cent the way Stripe rounds it. $20 → $19.12.
pub(crate) fn credit_after_fee(amount_micros: i64, policy: &crate::routes::CreditPolicy) -> i64 {
    let cents = (amount_micros + 9_999) / 10_000;
    let fee_cents = (cents * policy.card_fee_basis_points + 5_000) / 10_000
        + policy.card_fee_fixed_micros / 10_000;
    ((cents - fee_cents).max(0)) * 10_000
}

/// Resolve an api_key → (active customer id, account id). Errors on unknown
/// key or inactive subscription. Shared with `routes::billing_portal`; the relay
/// door deliberately does not use it (see `routes::relay::resolve_account`).
pub(crate) async fn resolve_active_customer(
    state: &AppState,
    api_key: &str,
) -> Result<(String, String), axum::response::Response> {
    let key_hash = sha256(api_key.as_bytes());

    // The ONE shared lookup (claim.rs), so every key-authed door moves
    // together.
    let cid = super::claim::customer_id_by_key_hash(&state.pool, &key_hash[..])
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
        // Free-vs-unknown (0017): see claim::KeyOwner — a free account's key
        // is valid, so it gets the honest 402, not a "revoked key" 401.
        return Err(
            match super::claim::key_owner(&state.pool, &key_hash[..]).await {
                Ok(super::claim::KeyOwner::FreeAccount) => err(
                    StatusCode::PAYMENT_REQUIRED,
                    "no_subscription",
                    "no subscription on this account yet",
                ),
                Ok(_) => err(
                    StatusCode::UNAUTHORIZED,
                    "invalid_api_key",
                    "unknown api key",
                ),
                Err(e) => {
                    tracing::warn!("account lookup failed: {e:#}");
                    err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "internal",
                        "account lookup failed",
                    )
                }
            },
        );
    };

    let row: Option<(String, String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT c.stripe_customer_id, c.account_id,
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

    let Some((customer_id, account_id, sub_status)) = row else {
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

    Ok((customer_id, account_id))
}

/// Reserve `amount_micros` against `customers.monthly_cap_micros` in one
/// statement, so parallel top-ups cannot both pass a read and overshoot the
/// cap. Resets the month lazily at the cohort-aligned 1st-of-month UTC.
/// Returns Err with a 402 response if the amount would exceed the cap.
async fn reserve_monthly_cap(
    pool: &sqlx::PgPool,
    customer_id: &str,
    amount_micros: i64,
) -> Result<(), axum::response::Response> {
    let internal = |e: sqlx::Error| {
        tracing::warn!("monthly cap reserve failed: {e:#}");
        err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "cap check failed")
    };
    let now = Utc::now();
    sqlx::query(
        r#"
        UPDATE customers
        SET monthly_charges_micros = 0,
            month_reset_at = date_trunc('month', $2::timestamptz + interval '1 month')
        WHERE stripe_customer_id = $1
          AND month_reset_at <= $2
        "#,
    )
    .bind(customer_id)
    .bind(now)
    .execute(pool)
    .await
    .map_err(internal)?;

    let reserved = sqlx::query(
        "UPDATE customers SET monthly_charges_micros = monthly_charges_micros + $2 \
         WHERE stripe_customer_id = $1 AND monthly_charges_micros + $2 <= monthly_cap_micros",
    )
    .bind(customer_id)
    .bind(amount_micros)
    .execute(pool)
    .await
    .map_err(internal)?;
    if reserved.rows_affected() == 1 {
        return Ok(());
    }

    let row: Option<(i64, i64)> = sqlx::query_as(
        "SELECT monthly_cap_micros, monthly_charges_micros FROM customers WHERE stripe_customer_id = $1",
    )
    .bind(customer_id)
    .fetch_optional(pool)
    .await
    .map_err(internal)?;
    let Some((cap, spent)) = row else {
        return Err(err(StatusCode::UNAUTHORIZED, "customer_not_found", "customer not found"));
    };
    Err((
        StatusCode::PAYMENT_REQUIRED,
        Json(json!({
            "error": {
                "code": "monthly_cap_reached",
                "monthly_cap_micros": cap,
                "monthly_charges_micros": spent,
                "message": "monthly spend cap reached — raise it in Settings to continue",
            }
        })),
    )
        .into_response())
}

/// Return a reservation for a top-up that charged nothing.
async fn release_monthly_cap(pool: &sqlx::PgPool, customer_id: &str, amount_micros: i64) {
    if let Err(e) = sqlx::query(
        "UPDATE customers SET monthly_charges_micros = GREATEST(monthly_charges_micros - $2, 0) \
         WHERE stripe_customer_id = $1",
    )
    .bind(customer_id)
    .bind(amount_micros)
    .execute(pool)
    .await
    {
        // The cap then over-counts until the month resets: a refused top-up,
        // never an extra charge.
        tracing::warn!("monthly cap release failed: {e:#}");
    }
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

#[cfg(test)]
mod tests {
    use super::{release_monthly_cap, reserve_monthly_cap, topup_idempotency_key};

    #[test]
    fn top_up_credit_is_the_charge_minus_the_card_fee() {
        let policy = crate::routes::CreditPolicy {
            renewal_micros: 20_000_000,
            card_fee_basis_points: 290,
            card_fee_fixed_micros: 300_000,
            auto_topup_micros: 20_000_000,
            topup_min_micros: 10_000_000,
            topup_max_micros: 50_000_000,
        };
        assert_eq!(super::credit_after_fee(20_000_000, &policy), 19_120_000);
        assert_eq!(super::credit_after_fee(10_000_000, &policy), 9_410_000);
        assert_eq!(super::credit_after_fee(50_000_000, &policy), 48_250_000);
        assert_eq!(super::credit_after_fee(100_000, &policy), 0, "never negative");
    }

    #[test]
    fn retries_inside_ten_minutes_share_a_key() {
        let a = topup_idempotency_key("cus_x", 10_000_000, true, 1_800_000_000);
        let b = topup_idempotency_key("cus_x", 10_000_000, true, 1_800_000_000 + 30);
        assert_eq!(a, b);
        assert_ne!(a, topup_idempotency_key("cus_x", 10_000_000, false, 1_800_000_000));
        assert_ne!(a, topup_idempotency_key("cus_x", 20_000_000, true, 1_800_000_000));
        assert_ne!(a, topup_idempotency_key("cus_y", 10_000_000, true, 1_800_000_000));
    }

    async fn spent(pool: &sqlx::PgPool) -> i64 {
        sqlx::query_scalar("SELECT monthly_charges_micros FROM customers WHERE stripe_customer_id = 'cus_x'")
            .fetch_one(pool)
            .await
            .unwrap()
    }

    /// Parallel top-ups each reserve in one statement, so together they
    /// cannot pass the cap; a refused charge gives its reservation back.
    #[sqlx::test(migrations = "./migrations")]
    async fn parallel_topups_cannot_overshoot_the_cap(pool: sqlx::PgPool) {
        sqlx::query(
            "INSERT INTO customers (stripe_customer_id, email, account_id, monthly_cap_micros) \
             VALUES ('cus_x', 'owner@example.com', 'acct_x', 50000000)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let tries = (0..8).map(|_| {
            let pool = pool.clone();
            tokio::spawn(async move { reserve_monthly_cap(&pool, "cus_x", 20_000_000).await.is_ok() })
        });
        let mut ok = 0;
        for t in tries {
            if t.await.unwrap() {
                ok += 1;
            }
        }
        assert_eq!(ok, 2, "two $20 top-ups fit under a $50 cap");
        assert_eq!(spent(&pool).await, 40_000_000);
        release_monthly_cap(&pool, "cus_x", 20_000_000).await;
        assert_eq!(spent(&pool).await, 20_000_000);
    }
}

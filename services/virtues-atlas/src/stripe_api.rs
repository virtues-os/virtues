//! Thin HTTP client for the few Stripe API calls Atlas needs.
//!
//! Scope is intentionally minimal: we don't pull in the full `async-stripe`
//! SDK because we only need (a) checkout session retrieval at activation
//! time and (b) webhook signature verification (which lives in
//! `crates/virtues-helpers/src/crypto/`). Less surface area = less to
//! audit for the privacy story.

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use reqwest::Client;
use serde::Deserialize;

const STRIPE_API: &str = "https://api.stripe.com/v1";

/// Pull Stripe's human-readable `error.message` out of an error response body.
///
/// Stripe returns errors as `{"error": {"message": "...", "code": "...", …}}`.
/// We surface only the `message` to callers (which render it on an error page),
/// while the full body is logged server-side. Falls back to a generic line if
/// the body isn't the shape we expect (e.g. an HTML 502 from an outage).
pub fn stripe_error_message(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(str::to_owned))
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| "Stripe rejected the request".to_string())
}

#[derive(Clone)]
pub struct StripeClient {
    http: Client,
    secret_key: String,
}

impl StripeClient {
    pub fn new(secret_key: String) -> Self {
        Self {
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .expect("reqwest client"),
            secret_key,
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.secret_key.is_empty()
    }

    /// Retrieve a checkout session by ID with `line_items` expanded so the
    /// caller can validate the session was for *our* configured price (a
    /// cheap-price-on-the-same-account attack would otherwise pass our paid
    /// gate). Used in `POST /claim` and `GET /link/done`.
    pub async fn retrieve_checkout_session(
        &self,
        session_id: &str,
    ) -> Result<CheckoutSession> {
        let resp = self
            .http
            .get(format!(
                "{}/checkout/sessions/{}?expand[]=line_items",
                STRIPE_API, session_id
            ))
            .basic_auth(&self.secret_key, Some(""))
            .send()
            .await
            .context("GET checkout session")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!(
                "stripe checkout session retrieval failed: {status} — {body}"
            ));
        }

        resp.json::<CheckoutSession>()
            .await
            .context("parse checkout session")
    }

    /// Retrieve a Checkout Session as raw JSON. Used by the success-page lookup,
    /// which only needs a few display fields (amount, currency, email) plus the
    /// `preorder_deposit` metadata gate — no typed struct worth maintaining.
    pub async fn retrieve_checkout_session_raw(
        &self,
        session_id: &str,
    ) -> Result<serde_json::Value> {
        let resp = self
            .http
            .get(format!("{}/checkout/sessions/{}", STRIPE_API, session_id))
            .basic_auth(&self.secret_key, Some(""))
            .send()
            .await
            .context("GET checkout session (raw)")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!(
                "stripe checkout session retrieval failed: {status} — {body}"
            ));
        }

        resp.json::<serde_json::Value>()
            .await
            .context("parse checkout session json")
    }

    /// Create a subscription Checkout Session for the device-link flow. The
    /// `user_code` is stamped into metadata so the success handler can match
    /// the completed session back to its pending device-link row. Returns the
    /// hosted Checkout URL to redirect the customer to.
    pub async fn create_checkout_session(
        &self,
        price_id: &str,
        success_url: &str,
        cancel_url: &str,
        user_code: &str,
        allow_promotion_codes: bool,
    ) -> Result<CreatedCheckoutSession> {
        self.create_checkout_session_for(
            price_id,
            success_url,
            cancel_url,
            user_code,
            allow_promotion_codes,
            None,
        )
        .await
    }

    /// [`create_checkout_session`] with the payer's email pre-filled. Used by
    /// the api_key-authed checkout door (a linked free account buying a
    /// subscription): `account_checkout_done` attaches the subscription to
    /// the account by the email Stripe reports, so pre-filling it is what
    /// keeps "free me" and "paying me" the same account. Stripe still lets
    /// the person change it; if they do, the subscription lands on a second
    /// account — the same outcome as any mistyped email at checkout.
    pub async fn create_checkout_session_for(
        &self,
        price_id: &str,
        success_url: &str,
        cancel_url: &str,
        user_code: &str,
        allow_promotion_codes: bool,
        customer_email: Option<&str>,
    ) -> Result<CreatedCheckoutSession> {
        let mut params: Vec<(&str, &str)> = vec![
            ("mode", "subscription"),
            ("line_items[0][price]", price_id),
            ("line_items[0][quantity]", "1"),
            ("success_url", success_url),
            ("cancel_url", cancel_url),
            ("metadata[user_code]", user_code),
            // v3: force card collection even when a 100%-off coupon zeros
            // the first invoice. Stripe's default `if_required` skips the
            // payment-method step in that case, leaving us with no saved
            // card for auto-top-up off-session charges. We always need
            // a card on file — auto-top-up is part of the launch product.
            ("payment_method_collection", "always"),
        ];
        // Staging only: surface Stripe's "Add promotion code" field. Caller
        // (link.rs) reads `state.allow_promotion_codes` — stays false in prod.
        if allow_promotion_codes {
            params.push(("allow_promotion_codes", "true"));
        }
        if let Some(email) = customer_email {
            params.push(("customer_email", email));
        }
        let resp = self
            .http
            .post(format!("{}/checkout/sessions", STRIPE_API))
            .basic_auth(&self.secret_key, Some(""))
            .form(&params)
            .send()
            .await
            .context("POST create checkout session")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            // Log the full body server-side; return only Stripe's clean
            // `error.message` so the caller can show it on the error page.
            tracing::warn!("stripe checkout session create failed: {status} — {body}");
            return Err(anyhow!("{}", stripe_error_message(&body)));
        }

        resp.json::<CreatedCheckoutSession>()
            .await
            .context("parse created checkout session")
    }

    /// Create a one-time `mode=payment` Checkout Session for a pre-order
    /// deposit and return the hosted URL. Either references a configured
    /// `price_id` or defines the amount inline (when `price_id` is empty) so no
    /// dashboard Price is required. The session is stamped with
    /// `metadata[type] = "preorder_deposit"` (on both the session and the
    /// PaymentIntent) so the webhook can route the completion to `preorders`.
    ///
    /// A payment-mode session can never satisfy `finalize_paid_session` (which
    /// requires `mode == "subscription"`), so a deposit can never be replayed
    /// into a device api_key.
    pub async fn create_deposit_checkout_session(
        &self,
        price_id: &str,
        amount_cents: i64,
        currency: &str,
        product_name: &str,
        product_image: &str,
        success_url: &str,
        cancel_url: &str,
        allowed_countries: &[String],
    ) -> Result<CreatedCheckoutSession> {
        let amount = amount_cents.to_string();
        let mut params: Vec<(&str, &str)> = vec![
            ("mode", "payment"),
            ("submit_type", "pay"),
            ("line_items[0][quantity]", "1"),
            ("success_url", success_url),
            ("cancel_url", cancel_url),
            ("metadata[type]", "preorder_deposit"),
            ("payment_intent_data[metadata][type]", "preorder_deposit"),
            // Card only. Naming a payment method type disables automatic methods,
            // so no Klarna / Afterpay / other BNPL on a refundable deposit. Card
            // wallets (Apple Pay / Google Pay) still ride in under "card".
            ("payment_method_types[0]", "card"),
        ];

        // Collect a shipping address and restrict the country dropdown to the
        // configured set (US-only for the first batch). This both gives
        // fulfillment an address AND hard-blocks out-of-region checkouts —
        // Stripe won't let a customer complete from an unlisted country.
        // The keys are dynamic (`...[allowed_countries][N]`), so the formatted
        // key strings are held in `country_keys` to outlive the `&str` params.
        let country_keys: Vec<String> = (0..allowed_countries.len())
            .map(|i| format!("shipping_address_collection[allowed_countries][{i}]"))
            .collect();
        for (i, code) in allowed_countries.iter().enumerate() {
            params.push((country_keys[i].as_str(), code.as_str()));
        }

        if !price_id.is_empty() {
            params.push(("line_items[0][price]", price_id));
        } else {
            params.push(("line_items[0][price_data][currency]", currency));
            params.push(("line_items[0][price_data][unit_amount]", amount.as_str()));
            params.push(("line_items[0][price_data][product_data][name]", product_name));
            if !product_image.is_empty() {
                params.push(("line_items[0][price_data][product_data][images][0]", product_image));
            }
        }

        let resp = self
            .http
            .post(format!("{}/checkout/sessions", STRIPE_API))
            .basic_auth(&self.secret_key, Some(""))
            .form(&params)
            .send()
            .await
            .context("POST create deposit checkout session")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tracing::warn!("stripe deposit checkout create failed: {status} — {body}");
            return Err(anyhow!("{}", stripe_error_message(&body)));
        }

        resp.json::<CreatedCheckoutSession>()
            .await
            .context("parse created deposit checkout session")
    }

    /// Create a Stripe-hosted Customer Portal session and return its URL.
    /// The portal lets the customer update their card, view invoices, and
    /// cancel — all on Stripe's side, so Atlas implements no billing UI.
    /// Backed by `POST /api/billing/portal` in core. `return_url` is where
    /// Stripe sends the customer when they click "Return to Virtues".
    pub async fn create_billing_portal_session(
        &self,
        customer_id: &str,
        return_url: &str,
    ) -> Result<BillingPortalSession> {
        let params = [("customer", customer_id), ("return_url", return_url)];
        let resp = self
            .http
            .post(format!("{}/billing_portal/sessions", STRIPE_API))
            .basic_auth(&self.secret_key, Some(""))
            .form(&params)
            .send()
            .await
            .context("POST create billing portal session")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("stripe billing portal create failed: {status} — {body}"));
        }

        resp.json::<BillingPortalSession>()
            .await
            .context("parse billing portal session")
    }

    /// Off-session charge to a customer's saved default payment method.
    /// Used by the v3 top-up flow (`POST /credits/auto-topup` and
    /// `POST /credits/topup`).
    ///
    /// Stripe requires `payment_method` to be passed *explicitly* for
    /// off-session charges (`automatic_payment_methods` is for user-present
    /// flows). We look up the customer's `invoice_settings.default_payment_method`
    /// first, then create the PaymentIntent with that PM bound.
    ///
    /// Returns the payment_intent ID on success. On Stripe-side decline
    /// (insufficient funds, expired card, fraud) returns the Stripe error
    /// code unmodified so the caller can surface to iOS appropriately
    /// ("Update payment method", etc).
    pub async fn charge_off_session(
        &self,
        customer_id: &str,
        amount_micros: i64,
        description: &str,
    ) -> Result<String, OffSessionChargeError> {
        // Stripe takes cents, not micros. Round up to be safe.
        let cents = (amount_micros + 9_999) / 10_000;
        if cents <= 0 {
            return Err(OffSessionChargeError::InvalidAmount);
        }

        // Step 1: resolve a payment method to charge.
        //
        // The preferred source is the customer's
        // `invoice_settings.default_payment_method`, but Stripe Checkout in
        // `mode=subscription` sets the card on the *subscription*, not the
        // customer — so a Checkout-created customer has a null customer-level
        // default even though a perfectly good card is on file (the
        // subscription bills fine off its own PM). Fall back to the active
        // subscription's PM, then to any saved card, before declining. When
        // we resolve via a fallback we persist it back to the customer so the
        // next charge hits the fast path and the data self-heals.
        let cust_resp = self
            .http
            .get(format!("{}/customers/{}", STRIPE_API, customer_id))
            .basic_auth(&self.secret_key, Some(""))
            .send()
            .await
            .map_err(|e| OffSessionChargeError::Network(e.to_string()))?;
        let cust_body: serde_json::Value = cust_resp
            .json()
            .await
            .map_err(|e| OffSessionChargeError::ParseFailed(e.to_string()))?;
        let pm = match cust_body["invoice_settings"]["default_payment_method"]
            .as_str()
            .filter(|s| !s.is_empty())
        {
            Some(pm) => pm.to_string(),
            None => {
                let fallback = self.fallback_payment_method(customer_id).await?;
                // Best-effort: a failure here only costs the next charge a
                // repeat of the fallback lookup, so we ignore the result.
                let _ = self
                    .http
                    .post(format!("{}/customers/{}", STRIPE_API, customer_id))
                    .basic_auth(&self.secret_key, Some(""))
                    .form(&[("invoice_settings[default_payment_method]", fallback.as_str())])
                    .send()
                    .await;
                fallback
            }
        };

        // Step 2: create + confirm the PaymentIntent with the PM bound.
        let cents_str = cents.to_string();
        let params = [
            ("customer", customer_id),
            ("amount", cents_str.as_str()),
            ("currency", "usd"),
            ("off_session", "true"),
            ("confirm", "true"),
            ("description", description),
            ("payment_method", pm.as_str()),
            // The customer's default PM is frequently a Stripe Link method
            // (a Link-wrapped card from Checkout/Link autofill), not a raw
            // `card`. Restricting payment_method_types to `card` makes Stripe
            // reject it off-session with "The PaymentMethod provided (link) is
            // not allowed for this PaymentIntent ... include 'link'" — which
            // declines every auto-top-up and trips the failure circuit breaker.
            // Allow both so saved Link PMs (and plain cards) charge off-session.
            ("payment_method_types[0]", "card"),
            ("payment_method_types[1]", "link"),
        ];
        let resp = self
            .http
            .post(format!("{}/payment_intents", STRIPE_API))
            .basic_auth(&self.secret_key, Some(""))
            .form(&params)
            .send()
            .await
            .map_err(|e| OffSessionChargeError::Network(e.to_string()))?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| OffSessionChargeError::ParseFailed(e.to_string()))?;

        if !status.is_success() {
            // Stripe returns structured error: `{"error":{"code":"card_declined", ...}}`.
            let code = body["error"]["code"]
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            let message = body["error"]["message"].as_str().unwrap_or("").to_string();
            return Err(OffSessionChargeError::StripeDeclined { code, message });
        }

        // `status` on the PI tells us whether the charge actually settled.
        // `succeeded` is the only no-friction outcome; `requires_action`
        // means 3DS prompted the customer (we surface "tap to confirm" UX).
        let pi_id = body["id"].as_str().unwrap_or("").to_string();
        let pi_status = body["status"].as_str().unwrap_or("");
        match pi_status {
            "succeeded" => Ok(pi_id),
            "requires_action" => Err(OffSessionChargeError::AuthenticationRequired(pi_id)),
            other => Err(OffSessionChargeError::UnexpectedStatus(other.to_string())),
        }
    }

    /// Find a usable saved payment method when the customer carries no
    /// `invoice_settings.default_payment_method`. Tries the active
    /// subscription's `default_payment_method` first (the card entered at
    /// Checkout), then any attached card. Returns a `no_payment_method`
    /// decline only if nothing is genuinely on file.
    async fn fallback_payment_method(
        &self,
        customer_id: &str,
    ) -> Result<String, OffSessionChargeError> {
        // (a) the active subscription's default PM.
        let subs: serde_json::Value = self
            .http
            .get(format!("{}/subscriptions", STRIPE_API))
            .basic_auth(&self.secret_key, Some(""))
            .query(&[("customer", customer_id), ("status", "active"), ("limit", "1")])
            .send()
            .await
            .map_err(|e| OffSessionChargeError::Network(e.to_string()))?
            .json()
            .await
            .map_err(|e| OffSessionChargeError::ParseFailed(e.to_string()))?;
        if let Some(pm) = subs["data"][0]["default_payment_method"]
            .as_str()
            .filter(|s| !s.is_empty())
        {
            return Ok(pm.to_string());
        }

        // (b) any card attached to the customer.
        let pms: serde_json::Value = self
            .http
            .get(format!("{}/payment_methods", STRIPE_API))
            .basic_auth(&self.secret_key, Some(""))
            .query(&[("customer", customer_id), ("type", "card"), ("limit", "1")])
            .send()
            .await
            .map_err(|e| OffSessionChargeError::Network(e.to_string()))?
            .json()
            .await
            .map_err(|e| OffSessionChargeError::ParseFailed(e.to_string()))?;
        if let Some(pm) = pms["data"][0]["id"].as_str().filter(|s| !s.is_empty()) {
            return Ok(pm.to_string());
        }

        Err(OffSessionChargeError::StripeDeclined {
            code: "no_payment_method".to_string(),
            message: "customer has no default payment method on file".to_string(),
        })
    }

    /// Retrieve a subscription by ID. Used by `invoice.paid` to compute
    /// `current_period_end` (which is on the subscription, NOT the invoice).
    pub async fn retrieve_subscription(&self, subscription_id: &str) -> Result<Subscription> {
        let resp = self
            .http
            .get(format!("{}/subscriptions/{}", STRIPE_API, subscription_id))
            .basic_auth(&self.secret_key, Some(""))
            .send()
            .await
            .context("GET subscription")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!(
                "stripe subscription retrieval failed: {status} — {body}"
            ));
        }

        resp.json::<Subscription>()
            .await
            .context("parse subscription")
    }
}

/// Outcomes of an off-session charge attempt. The caller maps these to
/// iOS-facing errors: `StripeDeclined` → "card declined, update payment
/// method", `AuthenticationRequired` → push a 3DS confirmation to the
/// user, etc.
#[derive(Debug)]
pub enum OffSessionChargeError {
    InvalidAmount,
    Network(String),
    ParseFailed(String),
    /// Stripe returned a structured error. `code` is e.g. `card_declined`,
    /// `expired_card`, `insufficient_funds`. `message` is the human-readable
    /// version Stripe surfaces.
    StripeDeclined { code: String, message: String },
    /// PI succeeded the network call but needs 3DS / Strong Customer
    /// Authentication. Stripe returns a `payment_intent` with `status =
    /// requires_action`; the iOS app prompts the user to confirm.
    AuthenticationRequired(String),
    UnexpectedStatus(String),
}

impl std::fmt::Display for OffSessionChargeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidAmount => write!(f, "amount_micros must be > 0"),
            Self::Network(e) => write!(f, "network error contacting Stripe: {e}"),
            Self::ParseFailed(e) => write!(f, "failed to parse Stripe response: {e}"),
            Self::StripeDeclined { code, message } => write!(f, "stripe declined ({code}): {message}"),
            Self::AuthenticationRequired(pi) => write!(f, "3DS required on payment_intent {pi}"),
            Self::UnexpectedStatus(s) => write!(f, "unexpected payment_intent status: {s}"),
        }
    }
}

/// Minimal Stripe Checkout Session shape — only the fields Atlas needs.
#[derive(Debug, Deserialize)]
pub struct CheckoutSession {
    #[allow(dead_code)] // surfaced in reconciliation logs once that lands
    pub id: String,
    pub payment_status: String,
    /// `"open" | "complete" | "expired"`. Validated == "complete" in finalize.
    pub status: String,
    /// `"payment" | "subscription" | "setup"`. Validated == "subscription".
    pub mode: String,
    pub customer: Option<String>,
    pub subscription: Option<String>,
    pub customer_details: Option<CustomerDetails>,
    /// Free-form metadata set at session creation. We stamp `user_code` here in
    /// `create_checkout_session` and verify it back in `/link/done`.
    #[serde(default)]
    pub metadata: serde_json::Value,
    /// Present when retrieved with `expand[]=line_items`. Used to validate the
    /// session was for OUR price, not a cheaper one on the same account.
    pub line_items: Option<LineItems>,
}

#[derive(Debug, Deserialize)]
pub struct CustomerDetails {
    pub email: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LineItems {
    pub data: Vec<LineItem>,
}

#[derive(Debug, Deserialize)]
pub struct LineItem {
    pub price: Option<Price>,
}

#[derive(Debug, Deserialize)]
pub struct Price {
    pub id: String,
}

/// Minimal shape of a freshly-created Billing Portal session — just the
/// hosted URL the customer is redirected to.
#[derive(Debug, Deserialize)]
pub struct BillingPortalSession {
    pub url: String,
}

/// Minimal shape of a freshly-created Checkout Session — the hosted URL to
/// send the customer to, plus its id.
#[derive(Debug, Deserialize)]
pub struct CreatedCheckoutSession {
    #[allow(dead_code)]
    pub id: String,
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct Subscription {
    #[allow(dead_code)]
    pub id: String,
    #[allow(dead_code)] // available for future cross-checks; not load-bearing yet
    pub customer: String,
    #[allow(dead_code)]
    pub status: String,
    /// Unix timestamp.
    pub current_period_end: i64,
}

impl Subscription {
    pub fn period_end(&self) -> DateTime<Utc> {
        Utc.timestamp_opt(self.current_period_end, 0)
            .single()
            .unwrap_or_else(Utc::now)
    }
}

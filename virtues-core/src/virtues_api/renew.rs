//! The box's virtues-api credential: a single rotatable device `api_key`.
//!
//! Linked prepaid model — no vouchers, no bearer rotation, no client-side
//! renewal. atlas mints the api_key at link, registers it with virtues-api,
//! and credits the wallet (renewal via Stripe webhook, top-ups via card). The
//! box just stores the key and sends it on every proxy call. A 402 means the
//! wallet is empty (surface / auto-topup); a 401 means the key is unknown
//! (re-link).

use anyhow::{anyhow, Context, Result};
use sqlx::PgPool;
use virtues_helpers::auth::vault;

/// `source_id` of the box's billing credential (stores the api_key). NOT a
/// user-connected data source — onboarding's source counts exclude it, like
/// `__device__` and the BYO key.
pub const SOURCE_ID: &str = "virtues_api";
const CREDENTIAL_NAME: &str = "Virtues API";

pub struct ClaimResult {
    pub api_key: String,
}

/// `POST {atlas}/claim` — exchange a Stripe checkout session for the device
/// api_key. One-time, at onboarding.
pub async fn claim(
    http: &reqwest::Client,
    atlas_url: &str,
    session_id: &str,
) -> Result<ClaimResult> {
    let resp = http
        .post(format!("{}/claim", atlas_url.trim_end_matches('/')))
        .json(&serde_json::json!({ "session_id": session_id }))
        .send()
        .await
        .context("POST /claim")?;
    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(anyhow!("claim failed: {s} — {b}"));
    }
    let v: serde_json::Value = resp.json().await?;
    let api_key = v["api_key"]
        .as_str()
        .ok_or_else(|| anyhow!("claim response missing api_key"))?
        .to_string();
    Ok(ClaimResult { api_key })
}

/// Store (or replace) the api_key in the vault. Creates the `virtues_api`
/// credential on first claim.
pub async fn store_api_key(db: &PgPool, api_key: &str) -> Result<()> {
    let secrets = serde_json::json!({ "api_key": api_key });
    match find_credential_id(db).await? {
        Some(id) => vault::update_credential_secrets(db, &id, &secrets, None)
            .await
            .map_err(|e| anyhow!(e.to_string()))?,
        None => {
            vault::finalize_apikey_credential(db, SOURCE_ID, CREDENTIAL_NAME, &secrets)
                .await
                .map_err(|e| anyhow!(e.to_string()))?;
        }
    }
    Ok(())
}

/// Read the stored api_key, if linked. The proxy client attaches it as the
/// `Authorization: Bearer` credential on every call.
pub async fn read_api_key(db: &PgPool) -> Result<Option<String>> {
    let Some(id) = find_credential_id(db).await? else {
        return Ok(None);
    };
    let secrets = vault::read_credential_secrets(db, &id)
        .await
        .map_err(|e| anyhow!(e.to_string()))?;
    Ok(secrets["api_key"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string()))
}

/// Whether the box has linked a subscription (an api_key is present).
pub async fn has_api_key(db: &PgPool) -> Result<bool> {
    Ok(read_api_key(db).await?.is_some())
}

/// Outcome of an auto-top-up trigger.
#[derive(Debug)]
pub enum AutoTopupOutcome {
    /// atlas charged the saved card and credited the wallet; caller retries.
    Funded { amount_micros: i64 },
    /// Stripe declined. Caller surfaces "update payment method."
    CardDeclined { stripe_code: String, message: String },
    /// Card needs 3DS confirmation. iOS prompts the user.
    AuthenticationRequired { payment_intent: String },
    /// Customer hit their monthly cap. Caller surfaces "raise it in Settings."
    MonthlyCapReached { cap_micros: i64, charged_micros: i64 },
    /// Subscription past_due / canceled. Caller prompts re-subscribe.
    SubscriptionInactive,
}

/// Auto-top-up: box hit a `wallet_empty` 402. POST atlas `/credits/auto-topup`
/// with the api_key; atlas charges the saved card and credits the wallet
/// directly in virtues-api (no voucher, no redeem). Caller retries the call.
pub async fn auto_topup(
    db: &PgPool,
    http: &reqwest::Client,
    atlas_url: &str,
) -> Result<AutoTopupOutcome> {
    let api_key = read_api_key(db)
        .await?
        .ok_or_else(|| anyhow!("no virtues_api credential — link a subscription first"))?;

    let resp = http
        .post(format!("{}/credits/auto-topup", atlas_url.trim_end_matches('/')))
        .json(&serde_json::json!({ "api_key": api_key }))
        .send()
        .await
        .context("POST /credits/auto-topup")?;

    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap_or_else(|_| serde_json::json!({}));

    if status.is_success() {
        return Ok(AutoTopupOutcome::Funded {
            amount_micros: body["amount_micros"].as_i64().unwrap_or(0),
        });
    }

    let code = body["error"]["code"].as_str().unwrap_or("").to_string();
    let message = body["error"]["message"].as_str().unwrap_or("").to_string();
    let outcome = match code.as_str() {
        "card_declined" => AutoTopupOutcome::CardDeclined {
            stripe_code: body["error"]["stripe_code"].as_str().unwrap_or("").to_string(),
            message,
        },
        "authentication_required" => AutoTopupOutcome::AuthenticationRequired {
            payment_intent: body["error"]["payment_intent"].as_str().unwrap_or("").to_string(),
        },
        "monthly_cap_reached" => AutoTopupOutcome::MonthlyCapReached {
            cap_micros: body["error"]["monthly_cap_micros"].as_i64().unwrap_or(0),
            charged_micros: body["error"]["monthly_charges_micros"].as_i64().unwrap_or(0),
        },
        "subscription_inactive" => AutoTopupOutcome::SubscriptionInactive,
        _ => return Err(anyhow!("auto-topup failed: {status} — {body}")),
    };
    Ok(outcome)
}

/// `POST {atlas}/billing/portal/sessions` — exchange the api_key for a
/// Stripe-hosted Customer Portal URL (card, invoices, cancellation).
/// What atlas said when asked for a portal session. `NoSubscription` is a
/// real, expected state since the accounts decoupling (2026-08-31): a linked
/// free account holds a perfectly valid key and simply has no Stripe customer
/// to open a portal for — the UI must say that, not "try again" (a beta owner
/// read the generic copy as a transient failure and retried a permanent one).
///
/// Every non-URL variant carries atlas's own error `code` rather than only its
/// prose, because three unrelated conditions used to print one sentence and a
/// screenshot could not say which had happened.
pub enum PortalSession {
    Url(String),
    /// A valid key with no Stripe customer behind it: free, or lapsed.
    /// `code` is atlas's (`no_subscription` / `subscription_inactive`).
    NoSubscription { code: String },
    /// atlas answered, and the failure is its own — Stripe down, unknown key,
    /// internal. `status` is atlas's HTTP status, for the box's log line.
    Failed { code: String, status: u16 },
}

pub async fn fetch_portal_session(
    http: &reqwest::Client,
    atlas_url: &str,
    api_key: &str,
    return_url: &str,
) -> Result<PortalSession> {
    let resp = http
        .post(format!(
            "{}/billing/portal/sessions",
            atlas_url.trim_end_matches('/')
        ))
        .json(&serde_json::json!({
            "api_key": api_key,
            "return_url": return_url,
        }))
        .send()
        .await
        .context("POST /billing/portal/sessions")?;
    let status = resp.status();
    if status.is_success() {
        let v: serde_json::Value = resp.json().await?;
        return v["url"]
            .as_str()
            .map(|s| PortalSession::Url(s.to_string()))
            .ok_or_else(|| anyhow!("portal response missing url"));
    }

    // A refusal is `{error:{code,message}}`. An unparseable body is not fatal
    // here — "unknown" plus the status still names the branch better than the
    // prose the owner sees.
    let body: serde_json::Value = resp.json().await.unwrap_or_else(|_| serde_json::json!({}));
    let code = body["error"]["code"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();
    if status == reqwest::StatusCode::PAYMENT_REQUIRED {
        return Ok(PortalSession::NoSubscription { code });
    }
    Ok(PortalSession::Failed {
        code,
        status: status.as_u16(),
    })
}

/// What atlas says about this box's account. Distinct from "do we hold an
/// api_key", which is all the box could see before this door existed — and
/// which stopped meaning "subscribed" on 2026-08-31, when linking became
/// identity rather than billing (0017).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entitlement {
    /// Linked, and an active subscription stands behind the key.
    Subscribed,
    /// Linked, no subscription: free, or lapsed. Hosted AI, web search,
    /// Places and Plaid all refuse; the box itself is unaffected.
    Free,
    /// atlas rejected the key outright — revoked, or minted against an
    /// account that no longer exists. Distinct from Free: this one wants a
    /// re-link, and must never be shown as "you have no subscription".
    KeyUnknown,
}

/// `POST {atlas}/account/entitlement` — read-only, Stripe-free, safe to poll.
///
/// Err means we did not get an answer (transport, 5xx, malformed body). The
/// caller must hold its last known answer rather than render "unsubscribed"
/// from a network blip — atlas being unreachable is not a billing event.
pub async fn fetch_entitlement(
    http: &reqwest::Client,
    atlas_url: &str,
    api_key: &str,
) -> Result<Entitlement> {
    let resp = http
        .post(format!(
            "{}/account/entitlement",
            atlas_url.trim_end_matches('/')
        ))
        .json(&serde_json::json!({ "api_key": api_key }))
        .send()
        .await
        .context("POST /account/entitlement")?;

    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Ok(Entitlement::KeyUnknown);
    }
    if !status.is_success() {
        let b = resp.text().await.unwrap_or_default();
        return Err(anyhow!("entitlement fetch failed: {status} — {b}"));
    }
    let v: serde_json::Value = resp.json().await?;
    // Absent `subscribed` is a contract break, not a "no" — an older atlas
    // that does not serve this door 404s above, so reaching here with a body
    // we cannot read means something changed underneath us. Say so.
    let subscribed = v["subscribed"]
        .as_bool()
        .ok_or_else(|| anyhow!("entitlement response missing `subscribed`"))?;
    Ok(if subscribed {
        Entitlement::Subscribed
    } else {
        Entitlement::Free
    })
}

async fn find_credential_id(db: &PgPool) -> Result<Option<String>> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT id FROM credentials WHERE source_id = $1 LIMIT 1")
            .bind(SOURCE_ID)
            .fetch_optional(db)
            .await?;
    Ok(row.map(|r| r.0))
}

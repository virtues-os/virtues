//! Voucher-based renewal of the home server's virtues-api access bearer.
//!
//! Mirrors OAuth refresh: a stable `billing_token` (≈refresh token) is
//! exchanged for a fresh monthly `bearer` (≈access token) via the voucher
//! dance — Atlas `/voucher` → virtues-api `/v1/redeem`. Both secrets live
//! in the credential vault on this box; that's the one place the
//! customer↔bearer link exists.

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use rand::RngCore;
use sqlx::PgPool;
use virtues_helpers::auth::vault;

const SOURCE_ID: &str = "virtues_api";
const CREDENTIAL_NAME: &str = "Virtues API";

/// Generate a fresh random bearer. Only its SHA-256 ever leaves the device
/// (virtues-api stores the hash; the raw value stays in the local vault).
pub fn generate_bearer() -> String {
    let mut b = [0u8; 32];
    rand::rng().fill_bytes(&mut b);
    hex::encode(b)
}

pub struct ClaimResult {
    pub billing_token: String,
}

/// `POST {atlas}/claim` — exchange a Stripe checkout session for a billing
/// token. One-time, at onboarding.
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
    let billing_token = v["billing_token"]
        .as_str()
        .ok_or_else(|| anyhow!("claim response missing billing_token"))?
        .to_string();
    Ok(ClaimResult { billing_token })
}

/// Store (or replace) the billing token in the vault. Creates the
/// `virtues_api` credential on first claim.
pub async fn store_billing_token(db: &PgPool, billing_token: &str) -> Result<()> {
    let secrets = serde_json::json!({ "billing_token": billing_token, "bearer": "" });
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

pub struct RenewResult {
    pub bearer: String,
    pub expires_at: DateTime<Utc>,
}

/// Full renewal: read the billing token from the vault, generate a fresh
/// bearer, run the voucher dance, store the new bearer + expiry. Returns
/// the new bearer so the caller can use it immediately.
pub async fn renew(
    db: &PgPool,
    http: &reqwest::Client,
    atlas_url: &str,
    api_url: &str,
) -> Result<RenewResult> {
    let id = find_credential_id(db)
        .await?
        .ok_or_else(|| anyhow!("no virtues_api credential — claim a billing token first"))?;
    let secrets = vault::read_credential_secrets(db, &id)
        .await
        .map_err(|e| anyhow!(e.to_string()))?;
    let billing_token = secrets["billing_token"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("credential has no billing_token"))?;

    let bearer = generate_bearer();
    let voucher_code = fetch_voucher(http, atlas_url, billing_token).await?;
    let expires_at = redeem(http, api_url, &voucher_code, &bearer).await?;

    let expires_in = (expires_at - Utc::now()).num_seconds().max(0);
    let new_secrets = serde_json::json!({ "billing_token": billing_token, "bearer": bearer });
    vault::update_credential_secrets(db, &id, &new_secrets, Some(expires_in))
        .await
        .map_err(|e| anyhow!(e.to_string()))?;

    Ok(RenewResult { bearer, expires_at })
}

/// Outcome of an auto-top-up trigger.
#[derive(Debug)]
pub enum AutoTopupOutcome {
    /// Atlas charged the saved card, minted a $X voucher, the box redeemed
    /// it; new wallet balance returned for caller logging/display.
    Funded { wallet_micros: i64 },
    /// Stripe declined (card_declined, expired_card, insufficient_funds,
    /// etc). Caller should surface to iOS — "update payment method."
    CardDeclined { stripe_code: String, message: String },
    /// Card needs 3DS confirmation. iOS prompts the user.
    AuthenticationRequired { payment_intent: String },
    /// Customer hit their iOS-tunable monthly cap. Caller surfaces "raise
    /// the cap in Settings or wait until [date]."
    MonthlyCapReached { cap_micros: i64, charged_micros: i64 },
    /// Subscription is past_due / canceled. Caller prompts re-subscribe.
    SubscriptionInactive,
}

/// Auto-top-up: box hit a `wallet_empty` 402 on virtues-api. We POST atlas
/// `/credits/auto-topup` with the billing_token. On success atlas charges
/// the saved card $10 (off-session), mints a top-up voucher, returns the
/// voucher_code. We redeem it onto the EXISTING bearer (top-ups add to the
/// wallet — no new bearer needed). Caller retries the failed call.
///
/// Distinct from `renew()` because top-ups don't rotate the bearer or
/// reset wallet — they ADD credit to the current bearer's wallet within
/// the same cohort-aligned month.
pub async fn auto_topup(
    db: &PgPool,
    http: &reqwest::Client,
    atlas_url: &str,
    api_url: &str,
) -> Result<AutoTopupOutcome> {
    let id = find_credential_id(db)
        .await?
        .ok_or_else(|| anyhow!("no virtues_api credential — claim a billing token first"))?;
    let secrets = vault::read_credential_secrets(db, &id)
        .await
        .map_err(|e| anyhow!(e.to_string()))?;
    let billing_token = secrets["billing_token"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("credential has no billing_token"))?;
    let bearer = secrets["bearer"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("no current bearer to top up — run renew first"))?;

    let resp = http
        .post(format!("{}/credits/auto-topup", atlas_url.trim_end_matches('/')))
        .json(&serde_json::json!({ "billing_token": billing_token }))
        .send()
        .await
        .context("POST /credits/auto-topup")?;

    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap_or_else(|_| serde_json::json!({}));

    if status.is_success() {
        let voucher_code = body["voucher_code"]
            .as_str()
            .ok_or_else(|| anyhow!("auto-topup response missing voucher_code"))?;
        let new_wallet = redeem_topup(http, api_url, voucher_code, bearer).await?;
        return Ok(AutoTopupOutcome::Funded {
            wallet_micros: new_wallet,
        });
    }

    // Map atlas error codes to typed outcomes so BearerClient can decide
    // whether to retry, surface to iOS, or give up.
    let code = body["error"]["code"].as_str().unwrap_or("").to_string();
    let message = body["error"]["message"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let outcome = match code.as_str() {
        "card_declined" => AutoTopupOutcome::CardDeclined {
            stripe_code: body["error"]["stripe_code"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            message,
        },
        "authentication_required" => AutoTopupOutcome::AuthenticationRequired {
            payment_intent: body["error"]["payment_intent"]
                .as_str()
                .unwrap_or("")
                .to_string(),
        },
        "monthly_cap_reached" => AutoTopupOutcome::MonthlyCapReached {
            cap_micros: body["error"]["monthly_cap_micros"].as_i64().unwrap_or(0),
            charged_micros: body["error"]["monthly_charges_micros"]
                .as_i64()
                .unwrap_or(0),
        },
        "subscription_inactive" => AutoTopupOutcome::SubscriptionInactive,
        _ => return Err(anyhow!("auto-topup failed: {status} — {body}")),
    };
    Ok(outcome)
}

/// Redeem a TOP-UP voucher onto the current bearer (adds to wallet, does
/// NOT rotate the bearer or reset wallet). Returns the new `wallet_micros`
/// post-credit for caller logging.
async fn redeem_topup(
    http: &reqwest::Client,
    api_url: &str,
    voucher_code: &str,
    bearer: &str,
) -> Result<i64> {
    let resp = http
        .post(format!("{}/v1/redeem", api_url.trim_end_matches('/')))
        .header("Authorization", format!("Bearer {}", bearer))
        .json(&serde_json::json!({ "voucher_code": voucher_code }))
        .send()
        .await
        .context("POST /v1/redeem (top-up)")?;
    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(anyhow!("top-up redeem failed: {s} — {b}"));
    }
    let v: serde_json::Value = resp.json().await?;
    Ok(v["wallet_micros"].as_i64().unwrap_or(0))
}

/// Read the current bearer + expiry from the vault, if a non-empty bearer
/// has been minted. The virtues-api HTTP client uses this to authenticate
/// calls and to decide whether a renewal is due.
pub async fn current_bearer(db: &PgPool) -> Result<Option<(String, Option<DateTime<Utc>>)>> {
    let Some(id) = find_credential_id(db).await? else {
        return Ok(None);
    };
    let secrets = vault::read_credential_secrets(db, &id)
        .await
        .map_err(|e| anyhow!(e.to_string()))?;
    let bearer = secrets["bearer"].as_str().unwrap_or("").to_string();
    if bearer.is_empty() {
        return Ok(None);
    }
    let row: Option<(Option<DateTime<Utc>>,)> =
        sqlx::query_as("SELECT expires_at FROM credentials WHERE id = $1")
            .bind(&id)
            .fetch_optional(db)
            .await?;
    Ok(Some((bearer, row.and_then(|r| r.0))))
}

/// Whether a billing token has been claimed on this box (i.e. the onboarding
/// `/claim` step ran). Distinct from [`current_bearer`]: a billing token can
/// be present before any monthly bearer has been minted.
pub async fn has_billing_token(db: &PgPool) -> Result<bool> {
    let Some(id) = find_credential_id(db).await? else {
        return Ok(false);
    };
    let secrets = vault::read_credential_secrets(db, &id)
        .await
        .map_err(|e| anyhow!(e.to_string()))?;
    Ok(secrets["billing_token"]
        .as_str()
        .map(|s| !s.is_empty())
        .unwrap_or(false))
}

/// Read the raw billing token from the vault, if one has been claimed. Used
/// by the billing-portal handler, which needs to authenticate to Atlas as the
/// customer without minting a bearer.
pub async fn read_billing_token(db: &PgPool) -> Result<Option<String>> {
    let Some(id) = find_credential_id(db).await? else {
        return Ok(None);
    };
    let secrets = vault::read_credential_secrets(db, &id)
        .await
        .map_err(|e| anyhow!(e.to_string()))?;
    Ok(secrets["billing_token"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string()))
}

/// `POST {atlas}/billing/portal/sessions` — exchange the billing token for a
/// Stripe-hosted Customer Portal URL. The customer manages their card,
/// invoices, and cancellation there.
pub async fn fetch_portal_session(
    http: &reqwest::Client,
    atlas_url: &str,
    billing_token: &str,
    return_url: &str,
) -> Result<String> {
    let resp = http
        .post(format!(
            "{}/billing/portal/sessions",
            atlas_url.trim_end_matches('/')
        ))
        .json(&serde_json::json!({
            "billing_token": billing_token,
            "return_url": return_url,
        }))
        .send()
        .await
        .context("POST /billing/portal/sessions")?;
    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(anyhow!("portal session fetch failed: {s} — {b}"));
    }
    let v: serde_json::Value = resp.json().await?;
    v["url"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("portal response missing url"))
}

async fn find_credential_id(db: &PgPool) -> Result<Option<String>> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT id FROM credentials WHERE source_id = $1 LIMIT 1")
            .bind(SOURCE_ID)
            .fetch_optional(db)
            .await?;
    Ok(row.map(|r| r.0))
}

async fn fetch_voucher(
    http: &reqwest::Client,
    atlas_url: &str,
    billing_token: &str,
) -> Result<String> {
    let resp = http
        .post(format!("{}/voucher", atlas_url.trim_end_matches('/')))
        .json(&serde_json::json!({ "billing_token": billing_token }))
        .send()
        .await
        .context("POST /voucher")?;
    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(anyhow!("voucher fetch failed: {s} — {b}"));
    }
    let v: serde_json::Value = resp.json().await?;
    v["voucher_code"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("voucher response missing voucher_code"))
}

async fn redeem(
    http: &reqwest::Client,
    api_url: &str,
    voucher_code: &str,
    bearer: &str,
) -> Result<DateTime<Utc>> {
    let resp = http
        .post(format!("{}/v1/redeem", api_url.trim_end_matches('/')))
        .header("Authorization", format!("Bearer {}", bearer))
        .json(&serde_json::json!({ "voucher_code": voucher_code }))
        .send()
        .await
        .context("POST /v1/redeem")?;
    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(anyhow!("redeem failed: {s} — {b}"));
    }
    let v: serde_json::Value = resp.json().await?;
    let exp = v["expires_at"]
        .as_str()
        .ok_or_else(|| anyhow!("redeem response missing expires_at"))?;
    Ok(DateTime::parse_from_rfc3339(exp)?.with_timezone(&Utc))
}

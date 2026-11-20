//! Paid-session verification and the box-key helpers every key door shares.
//!
//! A Stripe Checkout session is verified here before anything is attached to
//! it: claimed once (anti-replay), paid, a completed subscription for our
//! price, with a customer, and stamped with the `account_id` it was created
//! for. The account always comes from that stamp, which atlas wrote for a
//! caller it had already authenticated. It never comes from the email Stripe
//! collected, because Stripe does not check that the payer owns that address.
//!
//! The rest of the module is the key lookups (`key_owner` and friends) and
//! `mint_box_key`, the one place a box key is recorded.

use chrono::{TimeZone, Utc};
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::routes::AppState;

/// Why a checkout session was refused. `code` is stable and logged.
pub(crate) struct FinalizeErr {
    pub code: &'static str,
    pub message: String,
}

/// A Stripe checkout session that passed every guard: paid, complete,
/// subscription-mode, our price, with a customer, bound to an account.
pub(crate) struct PaidSession {
    /// The account atlas stamped into `metadata[account_id]` when it created
    /// the session. The only input that chooses the account.
    pub account_id: String,
    pub stripe_customer_id: String,
    pub stripe_subscription_id: String,
    /// The address the payer typed at Stripe. Stored as the customer's
    /// contact address only; it never chooses an account.
    pub email: String,
    pub period_end: chrono::DateTime<Utc>,
}

/// Anti-replay-claim the session id, retrieve it from Stripe, and run every
/// C1 guard. On Err the claim is NOT released — callers that want a retry to
/// be possible call [`release_session_claim`] themselves, because only they
/// know whether the failure happened before or after anything irreversible.
pub(crate) async fn verify_and_claim_session(
    state: &AppState,
    session_id: &str,
) -> Result<PaidSession, FinalizeErr> {
    if !state.stripe.is_configured() {
        return Err(FinalizeErr {
            code: "stripe_not_configured",
            message: "STRIPE_SECRET_KEY not set".to_string(),
        });
    }

    // ── Anti-replay (C1) ──
    // A `cs_*` id can be observed in browser URLs / logs / referrers. Without
    // this guard, every replay would mint a new api_key AND rotate the
    // real owner's token via the customers UPSERT (silent account DoS). Claim
    // each session at most once; subsequent attempts return 409.
    let claimed = sqlx::query(
        "INSERT INTO claimed_sessions (stripe_session_id) VALUES ($1) \
         ON CONFLICT (stripe_session_id) DO NOTHING",
    )
    .bind(session_id)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("claimed_sessions insert failed: {e:#}");
        FinalizeErr {
            code: "internal",
            message: "could not record claim".to_string(),
        }
    })?;
    if claimed.rows_affected() == 0 {
        return Err(FinalizeErr {
            code: "session_already_claimed",
            message: "this checkout session was already used".to_string(),
        });
    }

    let session = state
        .stripe
        .retrieve_checkout_session(session_id)
        .await
        .map_err(|e| {
            tracing::warn!("stripe session retrieve failed: {e:#}");
            FinalizeErr {
                code: "stripe_error",
                message: e.to_string(),
            }
        })?;

    // Prod: only `paid` settles. Staging (`ATLAS_ALLOW_PROMOTION_CODES=true`)
    // also accepts `no_payment_required` so a 100%-off coupon completes the
    // claim without a card charge. Gating on the same flag that exposes the
    // coupon field keeps the two halves consistent.
    let payment_ok = session.payment_status == "paid"
        || (state.allow_promotion_codes && session.payment_status == "no_payment_required");
    if !payment_ok {
        return Err(FinalizeErr {
            code: "payment_not_complete",
            message: format!("checkout payment_status = {}", session.payment_status),
        });
    }
    // Stripe says the session must be a *completed subscription* for OUR price.
    // Without these, a one-off `mode=payment` session, an `expired` session, or
    // a cheap-price-on-the-same-account session would all pass `paid` and yield
    // a full api_key. (C1 hardening.)
    if session.mode != "subscription" {
        return Err(FinalizeErr {
            code: "wrong_mode",
            message: format!("session.mode = {} (want subscription)", session.mode),
        });
    }
    if session.status != "complete" {
        return Err(FinalizeErr {
            code: "session_not_complete",
            message: format!("session.status = {}", session.status),
        });
    }
    if !state.stripe_price_id.is_empty() {
        let price_ok = session
            .line_items
            .as_ref()
            .map(|li| {
                li.data
                    .iter()
                    .any(|item| item.price.as_ref().map(|p| p.id == state.stripe_price_id).unwrap_or(false))
            })
            .unwrap_or(false);
        if !price_ok {
            return Err(FinalizeErr {
                code: "price_mismatch",
                message: "session was not for the configured price".to_string(),
            });
        }
    }

    let Some(stripe_customer_id) = session.customer.clone() else {
        return Err(FinalizeErr {
            code: "no_customer",
            message: "session has no customer".to_string(),
        });
    };
    let stripe_subscription_id = session.subscription.clone().unwrap_or_default();
    let email = session
        .customer_details
        .as_ref()
        .and_then(|d| d.email.clone())
        .unwrap_or_else(|| "unknown@unknown".to_string());

    // Period end from metadata if present, else 30d out (webhooks correct it).
    let period_end = session
        .metadata
        .get("current_period_end")
        .and_then(|v| v.as_str().and_then(|s| s.parse::<i64>().ok()).or_else(|| v.as_i64()))
        .and_then(|ts| Utc.timestamp_opt(ts, 0).single())
        .unwrap_or_else(|| Utc::now() + chrono::Duration::days(30));

    let Some(account_id) = bound_account_id(&session.metadata) else {
        tracing::warn!(session_id, "paid checkout session carries no account_id");
        return Err(FinalizeErr {
            code: "unbound_session",
            message: "this checkout session is not bound to an account".to_string(),
        });
    };

    Ok(PaidSession {
        account_id,
        stripe_customer_id,
        stripe_subscription_id,
        email,
        period_end,
    })
}

/// The `account_id` atlas stamped into a session's metadata at creation, or
/// `None` when there is none. A session without one was not created by a
/// door that knew who was paying, so nothing may be attached to it.
fn bound_account_id(metadata: &serde_json::Value) -> Option<String> {
    metadata
        .get("account_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Release the anti-replay claim so the caller's retry can re-run finalize.
pub(crate) async fn release_session_claim(state: &AppState, session_id: &str) {
    let _ = sqlx::query("DELETE FROM claimed_sessions WHERE stripe_session_id = $1")
        .bind(session_id)
        .execute(&state.pool)
        .await;
}

/// Resolve an api_key hash → owning `stripe_customer_id`, per-box keys first.
///
/// THE one lookup behind every key-authenticated atlas endpoint (credits,
/// billing portal, settings) — it used to live as four hand-copied
/// `WHERE api_key_hash = $1` queries, which is exactly how a schema change
/// misses a door. `box_key` (authoritative, one row per live box) wins over
/// the legacy `customers.api_key_hash` mirror; the `pri` ordering makes that
/// preference explicit rather than an accident of UNION order.
pub(crate) async fn customer_id_by_key_hash(
    pool: &sqlx::PgPool,
    key_hash: &[u8],
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        SELECT stripe_customer_id FROM (
            SELECT stripe_customer_id, 0 AS pri FROM box_key   WHERE api_key_hash = $1
            UNION ALL
            SELECT stripe_customer_id, 1 AS pri FROM customers WHERE api_key_hash = $1
        ) t
        WHERE stripe_customer_id IS NOT NULL
        ORDER BY pri
        LIMIT 1
        "#,
    )
    .bind(key_hash)
    .fetch_optional(pool)
    .await
}

/// What a key-authed door learned about the key's owner. `Customer` carries
/// the `stripe_customer_id` for billing work; `FreeAccount` means the key is
/// VALID but its account has never paid — billing doors answer that with 402
/// `no_subscription`, never 401 `invalid_api_key` (a 401 reads box-side as
/// "key revoked, re-link", sending a legitimate free owner through a
/// needless re-pair).
pub(crate) enum KeyOwner {
    Customer(String),
    FreeAccount,
    Unknown,
}

/// The one owner-resolution behind every key-authed billing door (credits,
/// settings, billing portal) — so "valid but unpaid" and "unknown" cannot
/// drift into different answers per door.
pub(crate) async fn key_owner(
    pool: &sqlx::PgPool,
    key_hash: &[u8],
) -> Result<KeyOwner, sqlx::Error> {
    if let Some(cid) = customer_id_by_key_hash(pool, key_hash).await? {
        return Ok(KeyOwner::Customer(cid));
    }
    if account_id_by_key_hash(pool, key_hash).await?.is_some() {
        return Ok(KeyOwner::FreeAccount);
    }
    Ok(KeyOwner::Unknown)
}

/// Resolve an api_key hash → owning `account_id` — identity without billing
/// (agents/record/open-relay.md). The sibling of [`customer_id_by_key_hash`] for
/// doors that need to know WHO, not whether they pay: relay config, endpoint
/// registration. `box_key.account_id` is authoritative; the legacy
/// `customers.api_key_hash` mirror covers keys minted before 0017.
pub(crate) async fn account_id_by_key_hash(
    pool: &sqlx::PgPool,
    key_hash: &[u8],
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        SELECT account_id FROM (
            SELECT account_id, 0 AS pri FROM box_key
             WHERE api_key_hash = $1 AND account_id IS NOT NULL
            UNION ALL
            SELECT account_id, 1 AS pri FROM customers WHERE api_key_hash = $1
        ) t
        ORDER BY pri
        LIMIT 1
        "#,
    )
    .bind(key_hash)
    .fetch_optional(pool)
    .await
}

/// The email the account behind this key signed in with. `None` when the key
/// resolves to no account, or the account has no email row.
pub(crate) async fn account_email_by_key_hash(
    pool: &sqlx::PgPool,
    key_hash: &[u8],
) -> Result<Option<String>, sqlx::Error> {
    let Some(account_id) = account_id_by_key_hash(pool, key_hash).await? else {
        return Ok(None);
    };
    sqlx::query_scalar("SELECT email FROM accounts WHERE account_id = $1")
        .bind(&account_id)
        .fetch_optional(pool)
        .await
}

/// Record a freshly minted box key, scoped to the box that earned it.
///
/// Known `endpoint_id` → replace only THAT box's key (a second box linking no
/// longer kills the first box's credential — the whole point of per-box
/// keys). Unknown (older box that never identified itself) → the historical
/// whole-account rotation, matching what virtues-api's `register_device`
/// does for a `box_id: None` on its side, so the two systems never disagree
/// about which keys are alive.
///
/// Also mirrors the hash into `customers.api_key_hash` (most-recent-key
/// semantics, when the account has a Stripe customer) so a rolled-back atlas
/// binary keeps authenticating.
///
/// Rotation scope is the ACCOUNT (0017): a free account's box has no
/// `stripe_customer_id`, and two customers sharing an email collapsed into
/// one account at backfill — so scoping deletes by customer would miss keys.
///
/// The mirror customer is DERIVED here from `accounts`, never passed in: a
/// session frozen before payment carries no customer, so threading the
/// session's value through would silently skip the mirror for everyone who
/// signed in free and paid later — the exact cohort 0017 creates.
pub(crate) async fn mint_box_key(
    pool: &sqlx::PgPool,
    account_id: &str,
    endpoint_id: Option<&str>,
    api_key_hash: &[u8],
) -> Result<(), sqlx::Error> {
    let stripe_customer_id: Option<String> = sqlx::query_scalar(
        "SELECT stripe_customer_id FROM accounts WHERE account_id = $1",
    )
    .bind(account_id)
    .fetch_optional(pool)
    .await?
    .flatten();
    let stripe_customer_id = stripe_customer_id.as_deref();
    match endpoint_id {
        Some(ep) => {
            // `OR endpoint_id IS NULL` is load-bearing: SQL `=` never matches
            // NULL, so without it every backfilled/legacy unlabeled key would
            // survive its box's re-link FOREVER — an immortal live credential
            // (review finding, 2026-08-24). The NULL row is the account's old
            // shared single key; the first labeled attach retires it, exactly
            // as the legacy whole-account rotation would have.
            sqlx::query(
                "DELETE FROM box_key WHERE account_id = $1 \
                 AND (endpoint_id = $2 OR endpoint_id IS NULL)",
            )
            .bind(account_id)
            .bind(ep)
            .execute(pool)
            .await?;
        }
        None => {
            sqlx::query("DELETE FROM box_key WHERE account_id = $1")
                .bind(account_id)
                .execute(pool)
                .await?;
        }
    }
    sqlx::query(
        "INSERT INTO box_key (api_key_hash, account_id, stripe_customer_id, endpoint_id) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(api_key_hash)
    .bind(account_id)
    .bind(stripe_customer_id)
    .bind(endpoint_id)
    .execute(pool)
    .await?;
    if let Some(cid) = stripe_customer_id {
        sqlx::query("UPDATE customers SET api_key_hash = $2 WHERE stripe_customer_id = $1")
            .bind(cid)
            .bind(api_key_hash)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// A random 32-byte hex token (api_key / device_code shape).
pub(crate) fn random_token() -> String {
    let mut b = [0u8; 32];
    rand::rng().fill_bytes(&mut b);
    hex::encode(b)
}

/// A fresh opaque account id (`acct_<hex>`).
pub(crate) fn new_account_id() -> String {
    let mut b = [0u8; 16];
    rand::rng().fill_bytes(&mut b);
    format!("acct_{}", hex::encode(b))
}

pub(crate) fn sha256(data: &[u8]) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().to_vec()
}

#[cfg(test)]
mod bound_account_tests {
    use super::bound_account_id;
    use serde_json::json;

    #[test]
    fn reads_the_stamped_account() {
        let md = json!({ "account_id": "acct_0123", "user_code": "ABCD-EFGH" });
        assert_eq!(bound_account_id(&md).as_deref(), Some("acct_0123"));
    }

    /// A session opened by a door that did not know who was paying carries
    /// no stamp, and nothing may be attached to it.
    #[test]
    fn refuses_a_missing_or_empty_stamp() {
        assert_eq!(bound_account_id(&json!({})), None);
        assert_eq!(bound_account_id(&json!({ "account_id": "" })), None);
        assert_eq!(bound_account_id(&json!({ "account_id": "  " })), None);
        assert_eq!(bound_account_id(&json!({ "account_id": 7 })), None);
        assert_eq!(bound_account_id(&serde_json::Value::Null), None);
    }
}

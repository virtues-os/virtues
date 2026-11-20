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
/// billing portal, settings), so a schema change cannot miss a door.
/// `box_key` holds one row per live box key.
pub(crate) async fn customer_id_by_key_hash(
    pool: &sqlx::PgPool,
    key_hash: &[u8],
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT stripe_customer_id FROM box_key \
         WHERE api_key_hash = $1 AND stripe_customer_id IS NOT NULL",
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
/// registration.
pub(crate) async fn account_id_by_key_hash(
    pool: &sqlx::PgPool,
    key_hash: &[u8],
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT account_id FROM box_key WHERE api_key_hash = $1 AND account_id IS NOT NULL",
    )
    .bind(key_hash)
    .fetch_optional(pool)
    .await
}

/// The label a new box key is scoped by: the endpoint id the box reported,
/// or, when it reported none, a fresh label of its own.
///
/// A link with no endpoint id used to rotate every key on the account. The
/// endpoint id arrives on an unauthenticated `/init/start`, so whoever
/// started a link chose that, and one clicked link could take every other
/// box on the account offline. A fresh label scopes the new key to this
/// link alone. The cost is that a box which never reports an endpoint id
/// leaves its previous key alive when it links again; boxes report one
/// except in a rare race at first boot.
pub(crate) fn key_label(endpoint_id: Option<&str>) -> String {
    match endpoint_id.map(str::trim).filter(|s| !s.is_empty()) {
        Some(ep) => ep.to_string(),
        None => format!("unlabeled-{}", &random_token()[..16]),
    }
}

/// Record a freshly minted box key, scoped to the box that earned it.
///
/// Replaces only the key with the same label (see [`key_label`]), plus the
/// account's legacy unlabeled key. virtues-api's `register_device` applies
/// the same rule to a `box_id`, so the two systems never disagree about
/// which keys are alive.
///
/// Rotation scope is the ACCOUNT (0017): a free account's box has no
/// `stripe_customer_id`, and two customers sharing an email collapsed into
/// one account at backfill — so scoping deletes by customer would miss keys.
///
/// The key's customer is derived here from `accounts`, never passed in: a
/// session frozen before payment carries no customer, so a passed-in value
/// would leave keys unlabeled for everyone who signed in free and paid later.
pub(crate) async fn mint_box_key(
    pool: &sqlx::PgPool,
    account_id: &str,
    label: &str,
    api_key_hash: &[u8],
) -> Result<(), sqlx::Error> {
    // One transaction: a failure between the DELETE and the INSERT must not
    // leave the box with no key in atlas while virtues-api holds a new one.
    let mut tx = pool.begin().await?;
    let stripe_customer_id: Option<String> = sqlx::query_scalar(
        "SELECT stripe_customer_id FROM accounts WHERE account_id = $1",
    )
    .bind(account_id)
    .fetch_optional(&mut *tx)
    .await?
    .flatten();
    let stripe_customer_id = stripe_customer_id.as_deref();
    // `OR endpoint_id IS NULL` is load-bearing: SQL `=` never matches NULL,
    // so without it every backfilled/legacy unlabeled key would survive its
    // box's re-link forever, an immortal live credential. The NULL row is
    // the account's old shared single key; the first labeled attach retires it.
    sqlx::query(
        "DELETE FROM box_key WHERE account_id = $1 \
         AND (endpoint_id = $2 OR endpoint_id IS NULL)",
    )
    .bind(account_id)
    .bind(label)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO box_key (api_key_hash, account_id, stripe_customer_id, endpoint_id) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(api_key_hash)
    .bind(account_id)
    .bind(stripe_customer_id)
    .bind(label)
    .execute(&mut *tx)
    .await?;
    tx.commit().await
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

#[cfg(test)]
mod key_label_tests {
    use super::{key_label, mint_box_key};

    #[test]
    fn keeps_a_reported_endpoint_and_labels_a_missing_one() {
        assert_eq!(key_label(Some("ep_abc")), "ep_abc");
        let a = key_label(None);
        let b = key_label(Some("  "));
        assert!(a.starts_with("unlabeled-") && b.starts_with("unlabeled-"));
        assert_ne!(a, b, "each unlabeled link gets its own label");
    }

    async fn labels(pool: &sqlx::PgPool, account: &str) -> Vec<String> {
        sqlx::query_scalar(
            "SELECT coalesce(endpoint_id, '<null>') FROM box_key WHERE account_id = $1 ORDER BY 1",
        )
        .bind(account)
        .fetch_all(pool)
        .await
        .unwrap()
    }

    /// A link that reported no endpoint id used to delete every key on the
    /// account, and whoever started the link chooses that. Now it adds one
    /// key and leaves the other boxes' keys alone.
    #[sqlx::test(migrations = "./migrations")]
    async fn an_unlabeled_link_leaves_other_boxes_keys(pool: sqlx::PgPool) {
        sqlx::query("INSERT INTO accounts (account_id, email) VALUES ('acct_a', 'owner@example.com')")
            .execute(&pool)
            .await
            .unwrap();
        mint_box_key(&pool, "acct_a", "ep_kitchen", b"k1").await.unwrap();
        mint_box_key(&pool, "acct_a", "ep_office", b"k2").await.unwrap();

        let label = key_label(None);
        mint_box_key(&pool, "acct_a", &label, b"k3").await.unwrap();

        let got = labels(&pool, "acct_a").await;
        assert_eq!(got.len(), 3, "{got:?}");
        assert!(got.contains(&"ep_kitchen".to_string()) && got.contains(&"ep_office".to_string()));
    }

    /// Re-linking the same box replaces its key and retires the account's
    /// legacy unlabeled key.
    #[sqlx::test(migrations = "./migrations")]
    async fn relinking_a_box_replaces_its_key(pool: sqlx::PgPool) {
        sqlx::query("INSERT INTO accounts (account_id, email) VALUES ('acct_a', 'owner@example.com')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO box_key (api_key_hash, account_id) VALUES ('legacy', 'acct_a')")
            .execute(&pool)
            .await
            .unwrap();
        mint_box_key(&pool, "acct_a", "ep_kitchen", b"k1").await.unwrap();
        mint_box_key(&pool, "acct_a", "ep_kitchen", b"k2").await.unwrap();
        assert_eq!(labels(&pool, "acct_a").await, vec!["ep_kitchen".to_string()]);
    }
}

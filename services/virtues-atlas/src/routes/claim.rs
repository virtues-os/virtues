//! api_key claim (customer-facing, once per signup).
//!
//! `POST /claim { session_id }`
//!
//! After Stripe Checkout, the browser is redirected to
//! `success_url?session_id=cs_xxx`. The home server posts that session here.
//! Atlas verifies the payment, creates the customer + subscription, assigns an
//! opaque `account_id`, mints the device **api_key**, registers it with
//! virtues-api (`/internal/device`) and funds this period's wallet
//! (`/internal/credit`), then returns the api_key. The box stores it and sends
//! it on every proxy call.
//!
//! Re-claiming issues a fresh api_key (rotating the stored hash); the wallet is
//! keyed by the stable `account_id`, so the balance is preserved across it.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::post,
    Router,
};
use anyhow::Context as _;
use chrono::{TimeZone, Utc};
use rand::RngCore;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::routes::AppState;
use crate::virtues_api_client::{Credit, RegisterDevice};

pub fn router() -> Router<AppState> {
    Router::new().route("/claim", post(claim))
}

#[derive(Debug, Deserialize)]
struct ClaimBody {
    session_id: String,
}

async fn claim(State(state): State<AppState>, Json(body): Json<ClaimBody>) -> axum::response::Response {
    match finalize_paid_session(&state, &body.session_id).await {
        Ok(f) => (
            StatusCode::CREATED,
            Json(json!({
                "api_key": f.api_key,
                "current_period_end": f.period_end,
            })),
        )
            .into_response(),
        Err(e) => err(e.status, e.code, &e.message),
    }
}

/// The minted credential for a verified paid session.
pub(crate) struct Finalized {
    /// The device api_key the box stores + sends to the proxy.
    pub api_key: String,
    pub period_end: chrono::DateTime<Utc>,
    /// The session id we just consumed — for `/link/done` to match against the
    /// `device_link.stripe_session_id` it stamped, so a session for code A
    /// can't finalize the row for code B.
    pub session_id: String,
    /// The `metadata[user_code]` we stamped at create time, returned so
    /// `/link/done` can verify it matches the URL code (binding C2 fix).
    pub metadata_user_code: Option<String>,
}

/// A finalize failure, carrying the HTTP shape the caller should surface.
pub(crate) struct FinalizeErr {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

/// A Stripe checkout session that passed every guard: paid, complete,
/// subscription-mode, our price, with a customer. What both finalizers (the
/// device-link claim and the account checkout) build on — extracted so the
/// two can never drift on what "paid" means.
pub(crate) struct PaidSession {
    pub stripe_customer_id: String,
    pub stripe_subscription_id: String,
    pub email: String,
    pub period_end: chrono::DateTime<Utc>,
    pub metadata_user_code: Option<String>,
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
            status: StatusCode::SERVICE_UNAVAILABLE,
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
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal",
            message: "could not record claim".to_string(),
        }
    })?;
    if claimed.rows_affected() == 0 {
        return Err(FinalizeErr {
            status: StatusCode::CONFLICT,
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
                status: StatusCode::BAD_GATEWAY,
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
            status: StatusCode::PAYMENT_REQUIRED,
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
            status: StatusCode::BAD_REQUEST,
            code: "wrong_mode",
            message: format!("session.mode = {} (want subscription)", session.mode),
        });
    }
    if session.status != "complete" {
        return Err(FinalizeErr {
            status: StatusCode::BAD_REQUEST,
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
                status: StatusCode::BAD_REQUEST,
                code: "price_mismatch",
                message: "session was not for the configured price".to_string(),
            });
        }
    }

    let Some(stripe_customer_id) = session.customer.clone() else {
        return Err(FinalizeErr {
            status: StatusCode::BAD_REQUEST,
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

    let metadata_user_code = session
        .metadata
        .get("user_code")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    Ok(PaidSession {
        stripe_customer_id,
        stripe_subscription_id,
        email,
        period_end,
        metadata_user_code,
    })
}

/// Release the anti-replay claim so the caller's retry can re-run finalize.
pub(crate) async fn release_session_claim(state: &AppState, session_id: &str) {
    let _ = sqlx::query("DELETE FROM claimed_sessions WHERE stripe_session_id = $1")
        .bind(session_id)
        .execute(&state.pool)
        .await;
}

pub(crate) async fn finalize_paid_session(
    state: &AppState,
    session_id: &str,
) -> Result<Finalized, FinalizeErr> {
    let PaidSession {
        stripe_customer_id,
        stripe_subscription_id,
        email,
        period_end,
        metadata_user_code,
    } = verify_and_claim_session(state, session_id).await?;

    // Mint a fresh device api_key (the box's single credential).
    let api_key = random_token();
    let api_key_hash = sha256(api_key.as_bytes());

    let internal = |what: &str| FinalizeErr {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        code: "internal",
        message: what.to_string(),
    };

    // The account is the identity root (0017): mint-or-fetch it by email
    // FIRST, so a customer created by this paid checkout lands on the same
    // account_id a free sign-in already minted — the wallet key never forks.
    let ensured_account_id = super::account::ensure_account(&state.pool, &email)
        .await
        .map_err(|e| {
            tracing::warn!("claim ensure_account failed: {e:#}");
            internal("account upsert failed")
        })?;

    // Upsert customer with the new api_key hash (rotate on re-claim). The
    // account_id CONVERGES on the email's account (0017) even on conflict —
    // customers.account_id is what webhooks credit, and any divergence from
    // the accounts-table id the box registered under sends a subscription's
    // renewal to a wallet nothing reads.
    let (account_id,): (String,) = sqlx::query_as(
        r#"
        INSERT INTO customers (stripe_customer_id, email, api_key_hash, account_id)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (stripe_customer_id)
        DO UPDATE SET api_key_hash = $3, email = $2, account_id = $4
        RETURNING account_id
        "#,
    )
    .bind(&stripe_customer_id)
    .bind(&email)
    .bind(&api_key_hash[..])
    .bind(&ensured_account_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("claim upsert customer failed: {e:#}");
        internal("customer upsert failed")
    })?;
    if let Err(e) =
        super::account::attach_stripe_to_account(&state.pool, &email, &stripe_customer_id).await
    {
        tracing::warn!("claim stripe attach failed: {e:#}");
    }

    // Upsert subscription.
    sqlx::query(
        r#"
        INSERT INTO subscriptions (stripe_subscription_id, stripe_customer_id, status, current_period_end)
        VALUES ($1, $2, 'active', $3)
        ON CONFLICT (stripe_subscription_id)
        DO UPDATE SET status = 'active', current_period_end = $3
        "#,
    )
    .bind(&stripe_subscription_id)
    .bind(&stripe_customer_id)
    .bind(period_end)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("claim upsert subscription failed: {e:#}");
        internal("subscription upsert failed")
    })?;

    // Which box is this checkout for? The link row carries the endpoint_id
    // the box self-reported at /init/start (migration 0015); the checkout
    // session was stamped onto that row before the Stripe redirect. A missing
    // ROW is a real, benign None (older boxes; checkouts with no device link,
    // e.g. store pre-orders) and keeps the whole-account rotation. A QUERY
    // ERROR is not: degrading it to None would turn a transient DB blip into
    // a whole-account rotation that kills every sibling box's key — silently
    // un-fixing the bug per-box keys exist to fix (review finding,
    // 2026-08-24). Fail the finalize instead; the claim is released below and
    // the box's retry is safe.
    let endpoint_id: Option<String> = match sqlx::query_scalar(
        "SELECT endpoint_id FROM device_link WHERE stripe_session_id = $1",
    )
    .bind(session_id)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(row) => row.flatten(),
        Err(e) => {
            tracing::warn!("device_link endpoint lookup failed, releasing claim for retry: {e:#}");
            release_session_claim(state, session_id).await;
            return Err(internal("endpoint lookup failed — please retry"));
        }
    };

    // Register the device key with virtues-api and fund this period's wallet.
    // A fresh paid checkout funds the monthly allotment immediately ($20);
    // invoice.paid keeps it fresh monthly.
    //
    // CRITICAL: these are the last steps, and they sit downstream of the
    // already-committed anti-replay `claimed_sessions` row. If either fails
    // (transient virtues-api blip) we must RELEASE the claim — otherwise the
    // box gets a 500, never received the api_key, and its retry would hit
    // `session_already_claimed` forever (bricked checkout). Both calls are
    // idempotent (register replaces the box's — or, unlabeled, the account's —
    // key; credit `set` overwrites), so re-running the whole finalize on the
    // box's retry is safe.
    let provision = async {
        state
            .virtues_api
            .register_device(&RegisterDevice {
                box_id: endpoint_id.clone(),
                api_key_hash: hex::encode(&api_key_hash),
                account_id: account_id.clone(),
            })
            .await
            .context("register_device")?;
        // virtues-api holds the key; now record it atlas-side, scoped to the
        // box (register-before-record, same ordering doctrine as link.rs).
        mint_box_key(
            &state.pool,
            &account_id,
            endpoint_id.as_deref(),
            &api_key_hash[..],
        )
        .await
        .context("mint_box_key")?;
        state
            .virtues_api
            .credit(&Credit {
                account_id: account_id.clone(),
                amount_micros: state.credit.renewal_micros,
                mode: "set",
                reference: Some(format!("checkout:{session_id}")),
            })
            .await
            .context("initial credit")?;
        anyhow::Ok(())
    }
    .await;
    if let Err(e) = provision {
        tracing::warn!("provisioning failed, releasing claim for retry: {e:#}");
        release_session_claim(state, session_id).await;
        return Err(internal("provisioning failed — please retry"));
    }

    Ok(Finalized {
        api_key,
        period_end,
        session_id: session_id.to_string(),
        metadata_user_code,
    })
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
/// (open-relay-plan §Work 1b). The sibling of [`customer_id_by_key_hash`] for
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

fn err(status: StatusCode, code: &str, message: &str) -> axum::response::Response {
    (
        status,
        Json(json!({ "error": { "code": code, "message": message } })),
    )
        .into_response()
}

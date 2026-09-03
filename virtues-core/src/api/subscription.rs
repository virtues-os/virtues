//! Subscription status — two facts, not one.
//!
//! `linked` (does this box hold an api_key) and `subscribed` (does an active
//! subscription stand behind it) were the same bit until 2026-08-31. Migration
//! 0017 decoupled atlas's account identity from Stripe so that relay access and
//! second-box pairing would stop depending on payment; linking became identity,
//! and an api_key stopped implying a subscription.
//!
//! Nothing downstream was told. This endpoint kept reporting `active` for
//! anyone holding a key, so a free account read as subscribed on its owner's
//! screen — and the Stripe portal, the one control that would have corrected
//! them, answered "couldn't open it, try again" because there was no portal to
//! open. The box now asks atlas (`/account/entitlement`) rather than inferring.
//!
//! The box still does NOT gate anything on this: enforcement is server-side, in
//! the wallet. A lapsed subscription stops renewing it, the balance runs down,
//! and every metered call — hosted AI, web search, Places, Plaid — 402s. This
//! is what the UI reads to tell the owner where they stand.

use crate::error::Result;
use sqlx::PgPool;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::virtues_api::renew::Entitlement;

/// How long an answer from atlas is reused. The store polls this endpoint every
/// 60s from every open tab; entitlement changes at most a few times in a box's
/// life, so this is a cache with a heartbeat, not a poll.
const TTL: Duration = Duration::from_secs(300);

/// Last answer atlas gave, and when. Held across the TTL, and held INDEFINITELY
/// when atlas is unreachable — an outage must not read as "your subscription
/// ended". Process-local by design: a restart re-asks, which costs one request.
static CACHE: OnceLock<Mutex<Option<(Instant, Entitlement)>>> = OnceLock::new();

fn cache() -> &'static Mutex<Option<(Instant, Entitlement)>> {
    CACHE.get_or_init(|| Mutex::new(None))
}

/// Drop the cached answer. Called when the box links or re-links, so the UI
/// reflects a fresh subscription immediately instead of up to `TTL` later.
pub fn invalidate() {
    if let Ok(mut c) = cache().lock() {
        *c = None;
    }
}

/// The last answer atlas gave, whatever its age — for callers that need a
/// sentence right now (a metered 402 deciding between "subscribe" and "top
/// up") and cannot wait on a network round-trip. `None` = never asked.
pub fn last_known_entitlement() -> Option<Entitlement> {
    cache().lock().ok().and_then(|c| c.map(|(_, e)| e))
}

/// Ask atlas, or reuse a recent answer. `None` means we have never had one and
/// could not get one now — the caller reports "unknown", never "unsubscribed".
/// `fresh` skips the cache: the UI passes it while it waits on a checkout the
/// owner just opened, so a new subscription shows the moment it exists.
async fn entitlement(api_key: &str, fresh: bool) -> Option<Entitlement> {
    if !fresh {
        if let Ok(c) = cache().lock() {
            if let Some((at, e)) = *c {
                if at.elapsed() < TTL {
                    return Some(e);
                }
            }
        }
    }

    let http = crate::http_client::virtues_api_client();
    let atlas_url = crate::virtues_api::atlas_url();
    match crate::virtues_api::renew::fetch_entitlement(&http, &atlas_url, api_key).await {
        Ok(e) => {
            if let Ok(mut c) = cache().lock() {
                *c = Some((Instant::now(), e));
            }
            Some(e)
        }
        Err(e) => {
            tracing::warn!("entitlement check failed (holding last known): {e}");
            // Stale beats wrong. If we have never had an answer this is None,
            // and the UI says so rather than picking a side.
            cache().lock().ok().and_then(|c| c.map(|(_, e)| e))
        }
    }
}

/// The box's billing standing, as the UI reads it.
///
/// `status` is three-valued: `none` (no api_key), `linked` (key, no
/// subscription) and `active` (subscribed). It was two-valued and the middle
/// case reported `active`, which is the bug this file exists to close.
/// `entitlement_known` is false when atlas could not be reached and we have no
/// cached answer — the UI shows neither standing in that case.
///
/// Fully-local dev (`ENVIRONMENT=dev` + a verbatim `VIRTUES_API_KEY`, i.e. the
/// seeded local virtues-api) reports subscribed unconditionally: billing is
/// bypassed locally, so the box never claims a key, and without this the
/// frontend would nag despite AI working. Pointed at staging/prod (no verbatim
/// key) we fall through to the real signal so the genuine flow is exercised.
pub async fn get_subscription_status(pool: &PgPool, fresh: bool) -> Result<serde_json::Value> {
    if crate::middleware::auth::is_dev()
        && std::env::var("VIRTUES_API_KEY").is_ok_and(|b| !b.is_empty())
    {
        return Ok(payload("active", true, true, true));
    }

    // `?`, not `unwrap_or(false)`: a vault read error is not "no api_key", and
    // reporting it as one used to send a linked box back to the connect flow.
    let api_key = crate::virtues_api::renew::read_api_key(pool).await?;
    let Some(api_key) = api_key else {
        // Not linked. Nothing to ask atlas about, and the answer is certain.
        return Ok(payload("none", false, false, true));
    };

    Ok(match entitlement(&api_key, fresh).await {
        Some(Entitlement::Subscribed) => payload("active", true, true, true),
        Some(Entitlement::Free) => payload("linked", true, false, true),
        // The key is dead: linked in name only. Reported as unknown rather than
        // unsubscribed — "you have no subscription" is the wrong instruction
        // when the fix is to link again.
        Some(Entitlement::KeyUnknown) => payload("linked", true, false, false),
        None => payload("linked", true, false, false),
    })
}

fn payload(status: &str, linked: bool, subscribed: bool, known: bool) -> serde_json::Value {
    serde_json::json!({
        "status": status,
        "linked": linked,
        "subscribed": subscribed,
        "entitlement_known": known,
        // Kept for older clients (the iOS bundle ships its own SPA build).
        // Means subscribed, which is what every consumer used it for.
        "is_active": subscribed,
    })
}

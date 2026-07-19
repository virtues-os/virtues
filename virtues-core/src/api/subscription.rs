//! Subscription status — local-first.
//!
//! The box does NOT poll a remote service to gate access. Real subscription
//! state (active / past_due / canceled) lives in Atlas (the billing domain) and
//! is enforced server-side: a lapsed subscription stops renewing the wallet via
//! `invoice.paid`, so the balance runs down + expires and AI calls 402.
//!
//! So `/api/subscription` is answered locally from the credential vault: it
//! reports whether an `api_key` has been stored on this box (i.e. linked).

use crate::error::Result;
use sqlx::PgPool;

/// Local subscription signal derived from the credential vault.
///
/// `is_active` means an api_key has been stored (the box is linked). The trial
/// fields are always null — the launch plan is a flat monthly subscription with
/// no trial.
///
/// Fully-local dev (`ENVIRONMENT=dev` + a verbatim `VIRTUES_API_KEY`, i.e.
/// the seeded local virtues-api) reports active unconditionally: billing is
/// bypassed locally, so the box never claims a token, and without this the
/// frontend would nag "Subscribe to continue using AI" despite AI working.
/// Pointed at staging/prod (no verbatim key) we fall through to the real
/// claimed-token signal so the genuine billing flow can be exercised.
pub async fn get_subscription_status(pool: &PgPool) -> Result<serde_json::Value> {
    if crate::middleware::auth::is_dev()
        && std::env::var("VIRTUES_API_KEY").is_ok_and(|b| !b.is_empty())
    {
        return Ok(serde_json::json!({
            "status": "active",
            "trial_expires_at": null,
            "days_remaining": null,
            "is_active": true,
        }));
    }

    let has_token = crate::virtues_api::renew::has_api_key(pool)
        .await
        .unwrap_or(false);

    Ok(serde_json::json!({
        "status": if has_token { "active" } else { "none" },
        "trial_expires_at": null,
        "days_remaining": null,
        "is_active": has_token,
    }))
}

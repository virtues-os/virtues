//! Subscription status — voucher-model, local-first.
//!
//! In the voucher model the home server does NOT poll a remote service to
//! gate access: access is gated by the device bearer's expiry, which renews
//! itself monthly via the voucher dance (see `virtues_api::renew`). Real
//! subscription state (active / past_due / canceled) lives in Atlas — the
//! billing domain — and is reflected here only indirectly: a lapsed
//! subscription stops producing vouchers, so the bearer expires and AI calls
//! return 402. No status poll required.
//!
//! So `/api/subscription` is answered locally from the credential vault: it
//! reports whether a billing token has been claimed on this box. The billing
//! portal is an Atlas concern not yet wired (entitlement.md §10).

use crate::error::Result;
use sqlx::PgPool;

/// Local subscription signal derived from the credential vault.
///
/// `is_active` means a billing token has been claimed (onboarding `/claim`
/// ran). The trial fields are always null — the launch plan is a flat
/// monthly subscription with no trial.
pub async fn get_subscription_status(pool: &PgPool) -> Result<serde_json::Value> {
    let has_token = crate::virtues_api::renew::has_billing_token(pool)
        .await
        .unwrap_or(false);

    Ok(serde_json::json!({
        "status": if has_token { "active" } else { "none" },
        "trial_expires_at": null,
        "days_remaining": null,
        "is_active": has_token,
    }))
}

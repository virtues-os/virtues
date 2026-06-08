use axum::Router;
use sqlx::PgPool;

use crate::stripe_api::StripeClient;
use crate::virtues_api_client::VirtuesApiClient;

mod claim;
mod credits;
mod diag;
mod health;
mod link;
mod preorder;
mod settings;
mod voucher;
mod webhooks;

/// Voucher economics, passed into the /voucher handler.
#[derive(Clone, Copy)]
pub struct VoucherPolicy {
    /// Sub renewal voucher amount. Default $15. Env: VOUCHER_RENEWAL_MICROS.
    pub renewal_micros: i64,
    /// Auto-top-up voucher amount (fixed). Default $10. Env: AUTO_TOPUP_MICROS.
    pub auto_topup_micros: i64,
    /// Manual top-up minimum. Default $10. Env: TOPUP_MIN_MICROS.
    pub topup_min_micros: i64,
    /// Manual top-up maximum. Default $50. Env: TOPUP_MAX_MICROS.
    pub topup_max_micros: i64,
    pub unredeemed_days: i64,
    pub min_interval_days: i64,
}

/// Pre-order deposit parameters, passed into the /preorder/checkout handler.
#[derive(Clone)]
pub struct PreorderPolicy {
    /// Deposit amount in the smallest currency unit (e.g. cents). Default 5000.
    pub amount_cents: i64,
    /// ISO currency code for the deposit. Default "usd".
    pub currency: String,
    /// Line-item name shown on the Stripe Checkout page.
    pub product_name: String,
    /// Optional product image URL shown on the Checkout page. Empty → none.
    pub product_image: String,
    /// Optional Stripe Price ID. Empty → the amount is defined inline, so no
    /// dashboard Price is required.
    pub price_id: String,
    /// Where Stripe sends the browser after a completed / cancelled deposit —
    /// point these at the marketing site.
    pub success_url: String,
    pub cancel_url: String,
    /// ISO alpha-2 countries Checkout collects a shipping address for and
    /// restricts to. US-only for the first batch.
    pub allowed_countries: Vec<String>,
    /// "From" header on the founder thank-you email (Resend-verified sender).
    pub email_from: String,
    /// Reply-to on the thank-you email so replies reach Adam directly.
    pub email_reply_to: String,
}

/// Shared route state for atlas.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub virtues_api: VirtuesApiClient,
    pub stripe: StripeClient,
    pub stripe_webhook_secret: String,
    /// Stripe Price ID for the device-link Checkout flow.
    pub stripe_price_id: String,
    /// Public base URL Atlas is reachable at (for building link/checkout URLs).
    pub public_url: String,
    pub voucher: VoucherPolicy,
    pub preorder: PreorderPolicy,
    /// Resend API key for transactional email (the pre-order thank-you note).
    /// Empty → email sends are skipped. See `Config::resend_api_key`.
    pub resend_api_key: String,
    /// Staging escape hatch — surface promo-code field at Checkout AND accept
    /// `no_payment_required` in finalize. Off in prod. See `Config::allow_promotion_codes`.
    pub allow_promotion_codes: bool,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(health::router())
        .merge(claim::router())
        .merge(link::router())
        .merge(preorder::router())
        .merge(voucher::router())
        .merge(credits::router())
        .merge(settings::router())
        .merge(webhooks::router())
        .merge(diag::router())
}

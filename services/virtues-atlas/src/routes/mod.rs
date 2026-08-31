use axum::Router;
use sqlx::PgPool;

use crate::stripe_api::StripeClient;
use crate::virtues_api_client::VirtuesApiClient;

mod account;
mod billing_portal;
mod claim;
mod credits;
mod diag;
mod health;
mod link;
mod preorder;
mod relay;
mod settings;
mod webhooks;

/// Credit/pricing amounts (renewal + top-up bands).
#[derive(Clone, Copy)]
pub struct CreditPolicy {
    /// Monthly renewal credit. Default $20. Env: RENEWAL_MICROS.
    pub renewal_micros: i64,
    /// Auto-top-up amount (fixed). Default $10. Env: AUTO_TOPUP_MICROS.
    pub auto_topup_micros: i64,
    /// Manual top-up minimum. Default $10. Env: TOPUP_MIN_MICROS.
    pub topup_min_micros: i64,
    /// Manual top-up maximum. Default $50. Env: TOPUP_MAX_MICROS.
    pub topup_max_micros: i64,
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

/// Relay control-plane params for the iroh reach layer. See `routes::relay`.
#[derive(Clone)]
pub struct RelayPolicy {
    /// The relay URL boxes home on and clients dial through, e.g.
    /// `https://relay.virtues.ch`. Empty → `/relay/config` returns 503.
    pub relay_url: String,
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
    pub credit: CreditPolicy,
    pub preorder: PreorderPolicy,
    pub relay: RelayPolicy,
    /// Resend API key for transactional email (the pre-order thank-you note).
    /// Empty → email sends are skipped. See `Config::resend_api_key`.
    pub resend_api_key: String,
    /// Staging escape hatch — surface promo-code field at Checkout AND accept
    /// `no_payment_required` in finalize. Off in prod. See `Config::allow_promotion_codes`.
    pub allow_promotion_codes: bool,
}

/// CORS for the JSON endpoints the APP calls from inside its webview — the
/// airlock's inline sign-in (`connect.html`). Its origin is the app shell's,
/// not a website: `tauri://localhost` on macOS/Linux, `http://tauri.localhost`
/// on Windows, the `virtues://` scheme on iOS. Those can't be enumerated
/// stably across platforms, and nothing here rides on cookies — every call is
/// bearer-authed or public — so a wildcard origin is the honest policy.
/// `AUTHORIZATION` must be listed explicitly: the CORS spec exempts it from
/// header wildcards, and a `*` here would fail exactly the authed call that
/// matters. Scoped to the app-called routes only; pages, webhooks, and
/// box-called endpoints keep no CORS at all (no browser ever needs them).
///
/// KNOWN RESIDUAL: this makes `/account/login` cross-origin callable, so a
/// hostile web page can make visitors' browsers POST it and fire an OTP email
/// at an address the page chooses. The per-email send cap (account.rs,
/// MAX_SENDS_PER_HOUR, now fail-closed) bounds each victim, but a distributed
/// spray across many addresses is not bounded here. The proportionate control
/// is edge-level (per-IP / global throttle at the WAF), which is where
/// account.rs already says IP rate-limiting belongs — not a per-request check
/// in this process. Tracked for the WAF pass; the wildcard itself is required
/// (the app's origin is genuinely non-enumerable and the flow needs the call).
pub(crate) fn app_cors() -> tower_http::cors::CorsLayer {
    tower_http::cors::CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods([axum::http::Method::POST])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
        ])
}

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(account::router())
        .merge(health::router())
        .merge(claim::router())
        .merge(link::router())
        .merge(preorder::router())
        .merge(credits::router())
        .merge(relay::router())
        .merge(billing_portal::router())
        .merge(settings::router())
        .merge(webhooks::router())
        .merge(diag::router())
}

//! atlas - identity and billing for Virtues.
//!
//! Atlas holds the Stripe customer side of the wall. It mints a stable
//! billing token at signup (`/claim`), and a one-time voucher each month
//! (`/voucher`) that the home server redeems at virtues-api. It never sees
//! a usage bearer, and the only thing it sends across the wall is a
//! voucher's *value* — no customer, no bearer.
//!
//! See docs/virtues-api.md (the idea) and docs/entitlement.md.

use anyhow::Result;
use axum::Router;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

mod config;
mod db;
mod email;
mod routes;
mod stripe_api;
mod virtues_api_client;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "atlas=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cfg = config::Config::from_env()?;

    tracing::info!("atlas starting on port {}", cfg.port);

    let pool = db::connect_and_migrate(&cfg.database_url).await?;

    let virtues_api = virtues_api_client::VirtuesApiClient::new(
        cfg.virtues_api_url.clone(),
        cfg.virtues_api_internal_secret.clone(),
    );
    let stripe = stripe_api::StripeClient::new(cfg.stripe_secret_key.clone());

    let state = routes::AppState {
        pool,
        virtues_api,
        stripe,
        stripe_webhook_secret: cfg.stripe_webhook_secret.clone(),
        stripe_price_id: cfg.stripe_price_id.clone(),
        public_url: cfg.public_url.clone(),
        voucher: routes::VoucherPolicy {
            renewal_micros: cfg.voucher_renewal_micros,
            auto_topup_micros: cfg.auto_topup_micros,
            topup_min_micros: cfg.topup_min_micros,
            topup_max_micros: cfg.topup_max_micros,
            unredeemed_days: cfg.voucher_unredeemed_days,
            min_interval_days: cfg.voucher_min_interval_days,
        },
        preorder: routes::PreorderPolicy {
            amount_cents: cfg.preorder_deposit_amount_cents,
            currency: cfg.preorder_deposit_currency.clone(),
            product_name: cfg.preorder_product_name.clone(),
            product_image: cfg.preorder_product_image.clone(),
            price_id: cfg.preorder_deposit_price_id.clone(),
            success_url: cfg.preorder_success_url.clone(),
            cancel_url: cfg.preorder_cancel_url.clone(),
            allowed_countries: cfg.preorder_allowed_countries.clone(),
            email_from: cfg.preorder_email_from.clone(),
            email_reply_to: cfg.preorder_email_reply_to.clone(),
        },
        resend_api_key: cfg.resend_api_key.clone(),
        allow_promotion_codes: cfg.allow_promotion_codes,
    };

    if cfg.allow_promotion_codes {
        tracing::warn!(
            "ATLAS_ALLOW_PROMOTION_CODES is ON — Checkout exposes promo-code field \
             and finalize accepts no_payment_required ($0 claims via 100%-off coupons). \
             Cap blast radius in Stripe: max_redemptions + restrict_first_time_transactions \
             + expires_at on every coupon."
        );
    }

    let app = Router::new()
        .merge(routes::router())
        .with_state(state);

    let addr = format!("0.0.0.0:{}", cfg.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("atlas listening on {}", addr);

    // `into_make_service_with_connect_info` so handlers (e.g. /diag/*) that
    // need the client's `SocketAddr` can extract it via `ConnectInfo`.
    // Other handlers ignore the type parameter and behave identically.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}

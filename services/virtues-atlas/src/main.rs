//! atlas - identity + funding for Virtues (linked prepaid model).
//!
//! Atlas owns the Stripe customer side. At link/claim it mints the box's
//! device `api_key` and registers it with virtues-api against an opaque
//! `account_id`; on subscription renewal (`invoice.paid`) and top-up it credits
//! that account's wallet via virtues-api `/internal/credit`. It never sees
//! usage — only identity, plan, and amounts.

use std::time::Duration;

use anyhow::Result;
use axum::Router;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;
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
    // Install the ring CryptoProvider as the process-wide default. Without
    // this, `reqwest::Client::new()` panics at first use because rustls
    // 0.23 (used by reqwest + tokio-rustls + hyper-rustls) requires the
    // provider to be installed before any TLS work. Mirrors what
    // virtues-core's main.rs does for the same reason.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("install rustls ring CryptoProvider");

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

    // Route 53 client for the per-box ACME DNS-01 TXT-writer. Built only when a
    // hosted-zone id is configured; otherwise the writer endpoint returns 503 and
    // boxes stay on the self-signed bootstrap cert. Pinned to the legacy ring TLS
    // stack (see Cargo.toml) so no aws-lc-sys/cmake is needed. Credentials come
    // from the standard chain (env vars or the EC2 instance role via IMDS).
    let route53 = if cfg.route53_zone_id.is_empty() {
        tracing::info!("VIRTUES_ROUTE53_ZONE_ID unset — ACME DNS-01 TXT-writer disabled");
        None
    } else {
        let aws_cfg =
            aws_config::defaults(aws_config::BehaviorVersion::latest()).load().await;
        tracing::info!(zone = %cfg.route53_zone_id, "Route 53 ACME TXT-writer enabled");
        Some(aws_sdk_route53::Client::new(&aws_cfg))
    };

    // Parse atlas's Ed25519 relay-token signing key once at startup. A malformed
    // key is a fatal misconfig (fail loud, don't silently disable minting); an
    // empty key means relay minting is intentionally off (→ 503 on /relay/config).
    let relay_signing_key = if cfg.relay_signing_key.is_empty() {
        tracing::info!("VIRTUES_RELAY_SIGNING_KEY unset — relay token minting disabled");
        None
    } else {
        match virtues_protocol::relay::parse_signing_key(&cfg.relay_signing_key) {
            Some(k) => {
                tracing::info!("relay token signing key loaded (Ed25519)");
                Some(k)
            }
            None => {
                eprintln!(
                    "FATAL: VIRTUES_RELAY_SIGNING_KEY is set but not a valid hex-encoded \
                     32-byte Ed25519 private key. Generate one and set it, or unset it to \
                     disable relay minting."
                );
                std::process::exit(1);
            }
        }
    };

    let state = routes::AppState {
        pool,
        virtues_api,
        stripe,
        stripe_webhook_secret: cfg.stripe_webhook_secret.clone(),
        stripe_price_id: cfg.stripe_price_id.clone(),
        public_url: cfg.public_url.clone(),
        credit: routes::CreditPolicy {
            renewal_micros: cfg.renewal_micros,
            auto_topup_micros: cfg.auto_topup_micros,
            topup_min_micros: cfg.topup_min_micros,
            topup_max_micros: cfg.topup_max_micros,
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
        relay: routes::RelayPolicy {
            signing_key: relay_signing_key,
            control_addr: cfg.relay_control_addr.clone(),
            base_domain: cfg.relay_base_domain.clone(),
            route53_zone_id: cfg.route53_zone_id.clone(),
            route53,
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

    // Global hardening middleware. Applied to every endpoint:
    //
    //   - 1 MB request body cap. Atlas endpoints all carry tiny JSON; this
    //     is two orders of magnitude above what any legitimate caller
    //     sends. Prevents accidental + malicious memory blowup.
    //
    //   - 30 s request timeout. Slow-loris and accidental hung clients
    //     stop holding connections open. Above the longest known atlas
    //     handler (Stripe Checkout creation can take ~5 s end-to-end).
    //
    // Per-IP rate limiting deliberately omitted at this stage: at our
    // scale there's no abuse to dampen, and AWS WAF (or Cloudflare) is the
    // right home for edge-level throttling when traffic grows.
    let app = Router::new()
        .merge(routes::router())
        .layer(RequestBodyLimitLayer::new(1_048_576))
        .layer(TimeoutLayer::new(Duration::from_secs(30)))
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

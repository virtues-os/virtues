//! Configuration loaded from environment.
//!
//! Production secrets (Stripe keys, Privacy Pass private keys, internal
//! secret with virtues-api) live in env vars / KMS, never in source.

use anyhow::{anyhow, Result};

pub struct Config {
    pub port: u16,
    /// Postgres URL for Atlas's own database (NOT shared with virtues-api).
    pub database_url: String,
    /// URL of the virtues-api instance Atlas pushes entitlement updates to.
    pub virtues_api_url: String,
    /// Shared secret authenticating Atlas → virtues-api internal calls.
    pub virtues_api_internal_secret: String,
    /// Stripe secret API key (`sk_test_...` or `sk_live_...`). Used to
    /// retrieve checkout sessions in the `/claim` flow.
    pub stripe_secret_key: String,
    /// Stripe webhook signing secret (`whsec_...`). Used to verify the
    /// `Stripe-Signature` header on every webhook delivery.
    pub stripe_webhook_secret: String,
    /// Stripe Price ID (`price_...`) of the $29/mo plan. Used to create
    /// Checkout sessions in the device-link flow.
    pub stripe_price_id: String,
    /// Public base URL where Atlas is reachable by a customer's browser
    /// (e.g. `https://atlas.virtues.com`). Used to build the device-link
    /// verification URL and the Stripe success/cancel URLs.
    pub public_url: String,

    /// Monthly renewal credit (micros USD). Default $20/mo (full sub value).
    pub renewal_micros: i64,
    /// Auto-top-up amount (micros USD). Default $10 fixed.
    pub auto_topup_micros: i64,
    /// Manual top-up min/max range (micros USD). Defaults $10–$50.
    pub topup_min_micros: i64,
    pub topup_max_micros: i64,
    /// Surface Stripe's "Add promotion code" field at Checkout AND accept
    /// `payment_status = "no_payment_required"` in `finalize_paid_session`
    /// (so 100%-off coupons settle without a charge). Default false. Cap
    /// abuse in Stripe (max_redemptions + restrict_first_time_transactions
    /// + expires_at on every coupon); Stripe is the billing source of truth
    /// and enforces those atomically. Gate the two halves together —
    /// accepting free claims without showing the field is pointless, and
    /// showing the field without accepting free claims confuses UX (Checkout
    /// completes, `/claim` 400s).
    pub allow_promotion_codes: bool,

    /// Pre-order deposit amount in the smallest currency unit (cents). Default
    /// 5000 ($50). Env: PREORDER_DEPOSIT_AMOUNT_CENTS.
    pub preorder_deposit_amount_cents: i64,
    /// Pre-order deposit currency. Default "usd". Env: PREORDER_DEPOSIT_CURRENCY.
    pub preorder_deposit_currency: String,
    /// Line-item name shown on the deposit Checkout page. Env: PREORDER_PRODUCT_NAME.
    pub preorder_product_name: String,
    /// Optional product image URL on the deposit Checkout page (empty → none).
    /// Env: PREORDER_PRODUCT_IMAGE.
    pub preorder_product_image: String,
    /// Optional Stripe Price ID for the deposit. Empty → defined inline, so no
    /// dashboard Price is required. Env: PREORDER_DEPOSIT_PRICE_ID.
    pub preorder_deposit_price_id: String,
    /// Where Stripe sends the browser after a completed / cancelled deposit.
    /// Point these at the marketing site. Env: PREORDER_SUCCESS_URL / PREORDER_CANCEL_URL.
    pub preorder_success_url: String,
    pub preorder_cancel_url: String,
    /// ISO-3166 alpha-2 countries Checkout will collect a shipping address for
    /// (and restrict the country dropdown to). For the first batch this is
    /// US-only — a non-US customer can't complete the deposit. Comma-separated.
    /// Env: PREORDER_ALLOWED_COUNTRIES. Default "US".
    pub preorder_allowed_countries: Vec<String>,

    /// Resend API key for the founder's pre-order thank-you email. Empty →
    /// email is skipped (deposit still records). Env: RESEND_API_KEY.
    pub resend_api_key: String,
    /// "From" header on the thank-you email — must be a Resend-verified sender.
    /// Env: PREORDER_EMAIL_FROM. Default Adam at virtues.com.
    pub preorder_email_from: String,
    /// Reply-to on the thank-you email so customer replies reach Adam directly.
    /// Env: PREORDER_EMAIL_REPLY_TO. Default adam@virtues.com.
    pub preorder_email_reply_to: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let port: u16 = std::env::var("VIRTUES_ATLAS_PORT")
            .unwrap_or_else(|_| "9100".to_string())
            .parse()
            .map_err(|e| anyhow!("invalid VIRTUES_ATLAS_PORT: {e}"))?;

        let database_url = std::env::var("VIRTUES_ATLAS_DATABASE_URL")
            .map_err(|_| anyhow!("VIRTUES_ATLAS_DATABASE_URL not set"))?;

        let virtues_api_url = std::env::var("VIRTUES_API_URL")
            .unwrap_or_else(|_| "http://localhost:9002".to_string());

        // Optional during early scaffolding. WS-7 makes this required once
        // Atlas actually starts pushing updates.
        let virtues_api_internal_secret =
            std::env::var("VIRTUES_API_INTERNAL_SECRET").unwrap_or_default();

        // Optional during scaffolding — required once a real Stripe
        // account is wired up. Webhook + /activate routes return 503 if
        // unset, leaving everything else working for local testing.
        let stripe_secret_key = std::env::var("STRIPE_SECRET_KEY").unwrap_or_default();
        let stripe_webhook_secret =
            std::env::var("STRIPE_WEBHOOK_SECRET").unwrap_or_default();
        // Optional during scaffolding — required for the device-link checkout
        // flow once a real Stripe account + plan are wired.
        let stripe_price_id = std::env::var("STRIPE_PRICE_ID").unwrap_or_default();
        let public_url = std::env::var("VIRTUES_ATLAS_PUBLIC_URL")
            .unwrap_or_else(|_| format!("http://localhost:{port}"));

        // Wallet economics — linked prepaid model.
        //   * Monthly renewal credit: $20 (overwrites wallet to the full
        //     subscription value — "all credit, no haircut"). Margin comes
        //     entirely from the 20% universal markup: a fully-burned $20 wallet
        //     is $20/1.2 = $16.67 of real upstream cost, so usage is always
        //     margin-positive and the per-user floor is ~$2-3 (much higher for
        //     anyone who doesn't burn the whole wallet). See entitlement.rs::
        //     apply_markup.
        //   * Auto-top-up: $10 fixed, fires when wallet hits 0.
        //   * Manual top-up: $10–$50 user choice (atlas validates band).
        //   * Top-ups (add) are bounded by monthly_cap_micros, not anti-stacking.
        let renewal_micros = env_i64("VOUCHER_RENEWAL_MICROS", 20_000_000);
        let auto_topup_micros = env_i64("AUTO_TOPUP_MICROS", 10_000_000);
        let topup_min_micros = env_i64("TOPUP_MIN_MICROS", 10_000_000);
        let topup_max_micros = env_i64("TOPUP_MAX_MICROS", 50_000_000);

        let allow_promotion_codes = std::env::var("ATLAS_ALLOW_PROMOTION_CODES")
            .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes"))
            .unwrap_or(false);

        // Pre-order deposit ($50, fully refundable, credited toward the unit).
        // The amount is defined inline unless PREORDER_DEPOSIT_PRICE_ID is set.
        // Success/cancel URLs default to the production marketing site (Stripe
        // redirects the browser there after checkout); override per-deployment
        // with PREORDER_SUCCESS_URL / PREORDER_CANCEL_URL (e.g. for staging).
        const PREORDER_SITE: &str = "https://virtues.com";
        let preorder_deposit_amount_cents = env_i64("PREORDER_DEPOSIT_AMOUNT_CENTS", 5000);
        let preorder_deposit_currency =
            std::env::var("PREORDER_DEPOSIT_CURRENCY").unwrap_or_else(|_| "usd".to_string());
        let preorder_product_name = std::env::var("PREORDER_PRODUCT_NAME")
            .unwrap_or_else(|_| "Virtues Server — Pre-Order Deposit".to_string());
        let preorder_product_image = std::env::var("PREORDER_PRODUCT_IMAGE")
            .unwrap_or_else(|_| format!("{PREORDER_SITE}/images/test_main_1.jpg"));
        let preorder_deposit_price_id =
            std::env::var("PREORDER_DEPOSIT_PRICE_ID").unwrap_or_default();
        // Append the session id placeholder so the success page can show the
        // buyer their actual order (deposit, position in line, confirmation
        // email). Stripe substitutes {CHECKOUT_SESSION_ID} at redirect time.
        let preorder_success_url = std::env::var("PREORDER_SUCCESS_URL")
            .unwrap_or_else(|_| format!("{PREORDER_SITE}/pre-order/success?session_id={{CHECKOUT_SESSION_ID}}"));
        let preorder_cancel_url = std::env::var("PREORDER_CANCEL_URL")
            .unwrap_or_else(|_| format!("{PREORDER_SITE}/pre-order"));

        // US-only for the first batch. Comma-separated ISO alpha-2 codes;
        // blanks trimmed and empties dropped so "US, CA" and "US,CA" both work.
        let preorder_allowed_countries = std::env::var("PREORDER_ALLOWED_COUNTRIES")
            .ok()
            .map(|v| {
                v.split(',')
                    .map(|s| s.trim().to_uppercase())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| vec!["US".to_string()]);

        // Founder thank-you email. Optional — unset RESEND_API_KEY just skips
        // the send (the deposit still records and Stripe's own receipt, if
        // enabled in the Dashboard, still goes out).
        let resend_api_key = std::env::var("RESEND_API_KEY").unwrap_or_default();
        let preorder_email_from = std::env::var("PREORDER_EMAIL_FROM")
            .unwrap_or_else(|_| "Adam at Virtues <adam@virtues.com>".to_string());
        let preorder_email_reply_to = std::env::var("PREORDER_EMAIL_REPLY_TO")
            .unwrap_or_else(|_| "adam@virtues.com".to_string());

        Ok(Self {
            port,
            database_url,
            virtues_api_url,
            virtues_api_internal_secret,
            stripe_secret_key,
            stripe_webhook_secret,
            stripe_price_id,
            public_url,
            renewal_micros,
            auto_topup_micros,
            topup_min_micros,
            topup_max_micros,
            allow_promotion_codes,
            preorder_deposit_amount_cents,
            preorder_deposit_currency,
            preorder_product_name,
            preorder_product_image,
            preorder_deposit_price_id,
            preorder_success_url,
            preorder_cancel_url,
            preorder_allowed_countries,
            resend_api_key,
            preorder_email_from,
            preorder_email_reply_to,
        })
    }
}

fn env_i64(key: &str, default: i64) -> i64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

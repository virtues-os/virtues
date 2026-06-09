//! Deployment / box-bringup CLI commands: `status` and `bringup`.
//!
//! This is the deployment *substrate*: the same bringup the appliance runs
//! headless at first boot, that a DIY self-hoster runs interactively, and that
//! the phone-app onboarding API mirrors. One code path, identical for the
//! Virtues hardware box and a BYO server.

use anyhow::Result;

use crate::api::box_status::BoxStatus;
use crate::search::model_cache::{self, ModelSource};
use crate::search::Embedder;
use crate::wireguard::{ca, pairing};
use crate::Virtues;

/// `virtues status` — a human-readable box health report and the DIY
/// onboarding dashboard. Renders the boot gates as a checklist, the inference
/// resolution (same as `virtues doctor`), and the single next action to take.
/// Shares `api::box_status::compute_status` with the HTTP endpoints so the CLI
/// and web can never disagree.
pub async fn handle_status(virtues: &Virtues) -> Result<()> {
    let s = crate::api::box_status::compute_status(virtues.database.pool()).await?;
    let yn = |b: bool| if b { "✓" } else { "—" };

    println!();
    println!("Virtues box status");
    println!("──────────────────");
    println!("  identity:");
    println!("    server CA            {}", yn(s.identity.server_ca));
    println!("    WG server keypair    {}", yn(s.identity.wg_server_keypair));
    if let Some(pk) = &s.identity.wg_public_key {
        println!("      public key         {pk}");
    }
    println!("    rendezvous identity  {}", yn(s.identity.rendezvous));
    if let Some(pid) = &s.identity.publish_id {
        println!("      publish_id         {pid}");
    }

    // Inference resolution (accelerator + per-model baked/download).
    let r = model_cache::resolution_report();
    println!("  inference:");
    println!("    accelerator          {} ({})", r.accelerator, r.precision);
    if r.accelerator == "cuda" && !r.cuda_compiled {
        println!("    note                 GPU present but CPU-only build — `--features cuda`");
    }
    let mut any_download = false;
    for m in &r.models {
        let src = match &m.source {
            ModelSource::Baked(_) => "baked",
            ModelSource::Download => {
                any_download = true;
                "download on first use"
            }
        };
        println!("    {:<8}             {}", m.name, src);
    }

    println!("  subscription:");
    println!("    billing token        {}", yn(s.subscription.billing_token));
    println!("    AI ready (bearer)    {}", yn(s.subscription.bearer));
    println!("  devices:");
    println!("    paired (WG)          {}", s.devices.paired_wg);
    println!();

    if any_download {
        println!("  tip: run `virtues warm-models` to pre-fetch models (else they");
        println!("       download on the first search/chat request).");
        println!();
    }
    println!("  next: {}", next_step(&s));
    println!();
    Ok(())
}

/// The single next action a DIY operator should take, derived from the boot
/// gates. Identity first (usually already auto-done on boot), then link the
/// subscription, then pair a device.
fn next_step(s: &BoxStatus) -> String {
    let url = access_url();
    if !s.ready {
        return "identity incomplete — run `virtues bringup`".to_string();
    }
    if !s.subscription.billing_token {
        return format!("link your Virtues subscription — open {url}");
    }
    if s.devices.paired_wg == 0 {
        return format!("pair a device — open {url} and scan the QR");
    }
    "ready — your box is set up and paired".to_string()
}

/// Best-effort access URL for the on-box web UI. Uses an explicit override if
/// set, otherwise a generic hint (the DIY operator knows their host since they
/// ran `compose up`).
fn access_url() -> String {
    for var in ["VIRTUES_PUBLIC_URL", "VIRTUES_CORE_URL", "AUTH_URL"] {
        if let Ok(v) = std::env::var(var) {
            if !v.is_empty() {
                return v.trim_end_matches('/').to_string();
            }
        }
    }
    "http://<this-box-ip>:8000".to_string()
}

/// `virtues bringup` — non-interactive first-boot: run migrations and ensure the
/// box's identity exists (CA, rendezvous identity, WG server keypair). Idempotent,
/// so it's safe to run on every boot. The appliance runs this headless; DIY runs
/// it too.
pub async fn handle_bringup(virtues: &Virtues) -> Result<()> {
    let pool = virtues.database.pool();

    println!("running migrations…");
    virtues.database.initialize().await?;

    println!("ensuring box identity…");
    ca::ensure_ca(pool).await?;
    pairing::ensure_rendezvous_identity(pool).await?;
    #[cfg(target_os = "linux")]
    crate::wireguard::reconcile::ensure_server_keypair(pool).await?;
    #[cfg(not(target_os = "linux"))]
    println!("  (WG server keypair skipped — Linux-only; generated on the appliance)");

    println!("✅ bringup complete");
    handle_status(virtues).await
}

/// `virtues setup` — the quick DIY onboarding runner: migrations + identity +
/// pre-fetch the inference models, then print the status dashboard with the
/// next step. Idempotent; equivalent to `bringup` + `warm-models` + `status`
/// in one command (the "CLI-first and quick" self-host path).
pub async fn handle_setup(virtues: &Virtues) -> Result<()> {
    let pool = virtues.database.pool();

    println!("running migrations…");
    virtues.database.initialize().await?;

    println!("ensuring box identity…");
    ca::ensure_ca(pool).await?;
    pairing::ensure_rendezvous_identity(pool).await?;
    #[cfg(target_os = "linux")]
    crate::wireguard::reconcile::ensure_server_keypair(pool).await?;

    println!("warming inference models (first run downloads ~hundreds of MB)…");
    let embedder = crate::search::get_embedder().await?;
    println!("  embedder ready (dim={})", embedder.dimension());
    let _ = crate::search::get_reranker().await?;
    println!("  reranker ready");

    handle_status(virtues).await
}

/// `virtues subscribe` — connect this box to a paid Virtues subscription via the
/// device-authorization flow.
///
/// Three onboarding paths printed at once — user picks whatever's easiest:
///
///   1. Phone scan      → QR code rendered in terminal (unicode half-blocks)
///   2. Browser open    → URL printed alongside
///   3. Manual paste    → if the user already subscribed elsewhere they can
///                        skip the whole flow by setting `VIRTUES_BILLING_TOKEN`
///                        before running (we detect it before starting a link)
///
/// All three converge on the same atlas /link/* device-authorization flow.
/// On success the billing token is stored sealed in the box vault and the
/// first bearer is minted.
pub async fn handle_subscribe(virtues: &Virtues) -> Result<()> {
    use crate::virtues_api::link::{self, LinkStatus};

    let pool = virtues.database.pool();
    let atlas_url =
        std::env::var("VIRTUES_ATLAS_URL").unwrap_or_else(|_| "http://localhost:9100".to_string());
    let api_url =
        std::env::var("VIRTUES_API_URL").unwrap_or_else(|_| "http://localhost:9002".to_string());
    let http = crate::http_client::virtues_api_client();

    print_welcome(&atlas_url);

    let start = link::start(pool, &http, &atlas_url).await?;

    println!();
    print_qr_block(&start.verification_uri_complete);
    println!();
    println!("  Subscribe via phone:  scan the code above");
    println!("  Or open in browser:   {}", start.verification_uri_complete);
    println!("  Or enter code at:     {}   →  code: {}",
             start.verification_uri, start.user_code);
    println!();
    println!("  Waiting for checkout to complete… (Ctrl-C to cancel)");

    let interval = std::time::Duration::from_secs(start.interval.max(2));
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_secs(start.expires_in.max(60) as u64);
    loop {
        tokio::time::sleep(interval).await;
        if std::time::Instant::now() > deadline {
            println!("  link expired — run `virtues subscribe` again.");
            return Ok(());
        }
        match link::poll(pool, &http, &atlas_url, &api_url).await {
            Ok(LinkStatus::Ready) => {
                println!();
                println!("  ✅ linked — subscription active.");
                // Eagerly fetch the first voucher + credit the wallet so the
                // user's very next chat call doesn't 402 with an empty wallet.
                // The periodic renew cron handles every subsequent cycle; this
                // closes the cold-start gap between "subscription created" and
                // "wallet has money in it."
                match crate::virtues_api::renew::renew(pool, &http, &atlas_url, &api_url).await {
                    Ok(_) => println!("  ✅ wallet credited — AI ready."),
                    Err(e) => {
                        // Non-fatal — subscription is active even if the
                        // first voucher redeem failed (network blip, atlas
                        // restart, etc.). The renew cron will retry; tell
                        // the user so they're not surprised by a 402 on
                        // their first chat.
                        tracing::warn!(error = %e, "post-subscribe renew failed");
                        println!("  ⚠  wallet not yet credited (network issue?).");
                        println!("     The renew cron will retry; if your first chat");
                        println!("     returns 402, run `virtues subscribe` again or wait a minute.");
                    }
                }
                return handle_status(virtues).await;
            }
            Ok(LinkStatus::Expired) => {
                println!("  link expired or denied — run `virtues subscribe` again.");
                return Ok(());
            }
            Ok(LinkStatus::None) => {
                println!("  no link in flight — run `virtues subscribe` again.");
                return Ok(());
            }
            Ok(LinkStatus::Pending) => { /* keep waiting */ }
            Err(e) => tracing::warn!("poll error (will retry): {e}"),
        }
    }
}

/// Magic-link login for `[1] Log in to existing Virtues account`.
///
/// Pairs with `handle_subscribe`: both mint a device_code via `link::start`
/// and both poll `link::poll` until ready. The login path differs in how
/// the device_link gets flipped to ready:
///   - subscribe: user completes Stripe Checkout → atlas finalizes
///   - login:     user clicks magic link in email → atlas finalizes
/// Same poll loop afterward; same eager renew for the first voucher.
pub async fn handle_login(virtues: &Virtues) -> Result<()> {
    use crate::virtues_api::link::{self, LinkStatus, LoginStart};

    let pool = virtues.database.pool();
    let atlas_url =
        std::env::var("VIRTUES_ATLAS_URL").unwrap_or_else(|_| "http://localhost:9100".to_string());
    let api_url =
        std::env::var("VIRTUES_API_URL").unwrap_or_else(|_| "http://localhost:9002".to_string());
    let http = crate::http_client::virtues_api_client();

    // Start a device_link so the atlas /init/login call has something to
    // bind to. The subscribe path uses this same start; the login path
    // never shows the user the user_code/QR — they get an email instead.
    let _start = link::start(pool, &http, &atlas_url).await?;

    // Prompt for email.
    let email: String = dialoguer::Input::with_theme(&dialoguer::theme::ColorfulTheme::default())
        .with_prompt("Email on your Virtues subscription")
        .validate_with(|s: &String| -> std::result::Result<(), &str> {
            if s.contains('@') && s.contains('.') {
                Ok(())
            } else {
                Err("that doesn't look like an email")
            }
        })
        .interact_text()
        .map_err(|e| anyhow::anyhow!("prompt failed: {e}"))?;

    println!();
    match link::login(pool, &http, &atlas_url, &email).await? {
        LoginStart::Sent => {
            println!("  📧 Sent — check {email} for the magic link.");
            println!("     (15 min, single-use. Click the link, then this CLI continues.)");
        }
        LoginStart::NoAccount => {
            println!(
                "  No Virtues subscription found on {email}. Re-run `virtues init`"
            );
            println!("  and pick [2] Create new account instead.");
            return Ok(());
        }
        LoginStart::RateLimited => {
            println!(
                "  Too many login attempts for {email} in the last hour."
            );
            println!("  Try again later, or use [2] Create new if you don't have an account.");
            return Ok(());
        }
    }

    println!();
    println!("  Waiting for you to click the link… (Ctrl-C to cancel)");

    // Same poll loop as handle_subscribe. The device_link flips to ready
    // when the user clicks the email magic link → atlas marks it ready
    // → next poll picks up the billing_token.
    let interval = std::time::Duration::from_secs(5);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15 * 60);
    loop {
        tokio::time::sleep(interval).await;
        if std::time::Instant::now() > deadline {
            println!("  link expired — run `virtues login` again.");
            return Ok(());
        }
        match link::poll(pool, &http, &atlas_url, &api_url).await {
            Ok(LinkStatus::Ready) => {
                println!();
                println!("  ✅ logged in — subscription attached.");
                match crate::virtues_api::renew::renew(pool, &http, &atlas_url, &api_url).await {
                    Ok(_) => println!("  ✅ wallet credited — AI ready."),
                    Err(e) => {
                        tracing::warn!(error = %e, "post-login renew failed");
                        println!("  ⚠  wallet not yet credited (network blip?). It'll retry shortly.");
                    }
                }
                return handle_status(virtues).await;
            }
            Ok(LinkStatus::Expired) => {
                println!("  link expired or denied — run `virtues login` again.");
                return Ok(());
            }
            Ok(LinkStatus::None) => {
                println!("  no link in flight — run `virtues login` again.");
                return Ok(());
            }
            Ok(LinkStatus::Pending) => { /* keep waiting */ }
            Err(e) => tracing::warn!("poll error (will retry): {e}"),
        }
    }
}

/// Welcome banner + honest privacy framing. The "passes through, never stored"
/// claim is the v3 marketing line — accurate to what virtues-api actually does
/// (in-memory proxy, no logging, no DB persistence; verifiable in source).
fn print_welcome(atlas_url: &str) {
    let is_staging = atlas_url.contains("staging") || atlas_url.contains("localhost");
    println!();
    println!("─────────────────────────────────────────────────────────");
    println!("  Welcome to Virtues.");
    println!();
    println!("  Subscription: $20/mo — what you get:");
    println!("    • AI in your apps (proxied through Virtues; we never");
    println!("      store your prompts or responses — pure in-memory");
    println!("      passthrough, open source, ZDR upstream)");
    println!("    • Integrations: Google, Notion, Strava, Plaid");
    println!("    • Remote access from your phone");
    println!("    • Your data lives on this box, not on our servers");
    if is_staging {
        println!();
        println!("  ⚠  staging environment");
    }
    println!("─────────────────────────────────────────────────────────");
}

/// Render a QR code for `data` using unicode half-block characters so each
/// row is two QR-modules tall — keeps the QR compact enough to scan on
/// reasonable terminals (~25 lines tall for typical link URLs).
fn print_qr_block(data: &str) {
    let qr = match qrcode::QrCode::new(data) {
        Ok(q) => q,
        Err(_) => {
            // Fallback — should never happen for plausible URLs.
            println!("  (QR generation failed; use the URL below)");
            return;
        }
    };
    // 2 modules vertically per row → halve terminal height.
    let width = qr.width();
    let modules: Vec<Vec<bool>> = (0..width)
        .map(|y| {
            (0..width)
                .map(|x| qr[(x, y)] == qrcode::Color::Dark)
                .collect()
        })
        .collect();

    // Border (quiet zone) — most scanners require at least 2 modules.
    const BORDER: usize = 2;
    let pad = |row: &Vec<bool>| -> Vec<bool> {
        let mut v = vec![false; BORDER];
        v.extend(row.iter().copied());
        v.extend(vec![false; BORDER]);
        v
    };
    let blank = vec![false; width + 2 * BORDER];
    let mut all_rows: Vec<Vec<bool>> = Vec::with_capacity(width + 2 * BORDER);
    for _ in 0..BORDER {
        all_rows.push(blank.clone());
    }
    for r in &modules {
        all_rows.push(pad(r));
    }
    for _ in 0..BORDER {
        all_rows.push(blank.clone());
    }

    // Two rows per terminal line, using upper/lower half blocks. Inverted
    // colors so DARK QR pixels are spaces (light background) — this is the
    // convention scanners expect on dark terminals; on light terminals it
    // still works because we set foreground space. White ground + black fg
    // (▀ U+2580) works in both light and dark terminals.
    for chunk in all_rows.chunks(2) {
        let top = &chunk[0];
        let bot_default = blank.clone();
        let bot = chunk.get(1).unwrap_or(&bot_default);
        print!("  "); // small indent
        for x in 0..top.len() {
            let ch = match (top[x], bot[x]) {
                // ▀ upper half block = top dark, bottom light
                // ▄ lower half block = top light, bottom dark
                // █ full block       = both dark
                // ' ' space          = both light
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
                (false, false) => ' ',
            };
            print!("{ch}");
        }
        println!();
    }
}

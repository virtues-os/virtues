//! Deployment / box-bringup CLI commands: `status` and `bringup`.
//!
//! This is the deployment *substrate*: the same bringup the appliance runs
//! headless at first boot, that a DIY self-hoster runs interactively, and that
//! the phone-app onboarding API mirrors. One code path, identical for the
//! Virtues hardware box and a BYO server.

use anyhow::Result;

use crate::api::box_status::BoxStatus;
use crate::inference_report::{self, ModelSource};
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
    println!("    TLS cert (ACME)      {}", yn(s.identity.tls_cert));

    // Inference resolution (sidecar engine + per-model on-disk/missing).
    let r = inference_report::resolution_report();
    println!("  inference:");
    println!("    accelerator          {} ({})", r.accelerator, r.precision);
    let mut any_download = false;
    for m in &r.models {
        let src = match &m.source {
            ModelSource::Baked(_) => "on disk",
            ModelSource::Download => {
                any_download = true;
                "missing — re-run installer"
            }
        };
        println!("    {:<8}             {}", m.name, src);
    }

    println!("  subscription:");
    println!("    linked (api key)     {}", yn(s.subscription.linked));
    println!("  devices:");
    println!("    paired (WG)          {}", s.devices.paired_wg);

    // Setup + next-wins checklists — the textual mirror of the panel/wizard
    // (one state machine, three renderers; see docs/onboarding.md).
    if let Ok(setup) = crate::api::box_status::compute_setup_state(virtues.database.pool()).await {
        println!("  setup:");
        for step in &setup.setup {
            print_step(step);
        }
        if setup.setup_complete {
            println!("  next wins:");
            for step in &setup.onboarding {
                print_step(step);
            }
        }
    }
    println!();

    if any_download {
        // GGUFs don't lazy-download — a missing one means the sidecar for it
        // can't be serving. Surface it next to the per-model lines above.
        println!("  note: missing model files — re-run the installer to fetch");
    }
    println!("  next: {}", next_step(&s));
    println!();
    Ok(())
}

/// Render one setup/onboarding step as a checklist line.
fn print_step(step: &crate::api::box_status::SetupStep) {
    let mark = if step.done { "✓" } else { "—" };
    match &step.detail {
        Some(d) => println!("    {:<20} {mark}  ({d})", step.title),
        None => println!("    {:<20} {mark}", step.title),
    }
}

/// The single next action a DIY operator should take, derived from the boot
/// gates. Identity first (usually already auto-done on boot), then link the
/// subscription, then pair a device.
fn next_step(s: &BoxStatus) -> String {
    let url = access_url();
    if !s.ready {
        return "identity incomplete — run `virtues bringup`".to_string();
    }
    if !s.subscription.linked {
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
/// box's identity exists (WG server keypair). Idempotent, so it's safe to run on
/// every boot. The appliance runs this headless; DIY runs it too.
pub async fn handle_bringup(virtues: &Virtues) -> Result<()> {
    println!("running migrations…");
    virtues.database.initialize().await?;

    println!("✅ bringup complete");
    handle_status(virtues).await
}

/// `virtues subscribe` — connect this box to a paid Virtues subscription via the
/// device-authorization flow.
///
/// Two onboarding paths printed at once — user picks whatever's easiest:
///
///   1. Phone scan      → QR code rendered in terminal (unicode half-blocks)
///   2. Browser open    → URL printed alongside
///
/// Both converge on the same atlas /init/* device-authorization flow. On
/// success the device `api_key` is stored in the box vault (atlas registers the
/// device + funds the wallet at link).
pub async fn handle_subscribe(virtues: &Virtues) -> Result<()> {
    use crate::virtues_api::link::{self, LinkStatus};

    let pool = virtues.database.pool();
    let atlas_url =
        crate::virtues_api::atlas_url();
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
        match link::poll(pool, &http, &atlas_url).await {
            Ok(LinkStatus::Ready) => {
                // link::poll stores the api_key; atlas funds the wallet at link.
                // Lazy renew on the first AI call handles any in-poll renew failure.
                println!();
                println!("  Subscribed. AI ready.");
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
/// Same poll loop afterward.
pub async fn handle_login(virtues: &Virtues) -> Result<()> {
    use crate::virtues_api::link::{self, LinkStatus, LoginStart};

    let pool = virtues.database.pool();
    let atlas_url =
        crate::virtues_api::atlas_url();
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
            println!("  Sent magic link to {email}. Waiting… (Ctrl-C to cancel)");
        }
        LoginStart::NoAccount => {
            println!("  No Virtues subscription on {email}. Re-run `virtues init` and pick [2] Create new.");
            return Ok(());
        }
        LoginStart::RateLimited => {
            println!("  Too many login attempts for {email}. Try again later.");
            return Ok(());
        }
    }

    // Same poll loop as handle_subscribe. The device_link flips to ready
    // when the user clicks the email magic link → atlas marks it ready
    // → next poll picks up the api_key.
    let interval = std::time::Duration::from_secs(5);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15 * 60);
    loop {
        tokio::time::sleep(interval).await;
        if std::time::Instant::now() > deadline {
            println!("  link expired — run `virtues account-login` again.");
            return Ok(());
        }
        match link::poll(pool, &http, &atlas_url).await {
            Ok(LinkStatus::Ready) => {
                // link::poll stores the api_key; atlas funds the wallet at link.
                println!();
                println!("  Logged in. AI ready.");
                return handle_status(virtues).await;
            }
            Ok(LinkStatus::Expired) => {
                println!("  link expired or denied — run `virtues account-login` again.");
                return Ok(());
            }
            Ok(LinkStatus::None) => {
                println!("  no link in flight — run `virtues account-login` again.");
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
fn print_welcome(_atlas_url: &str) {
    let is_staging = crate::virtues_api::is_nonprod_cloud();
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
///
/// Assumes a UTF-8 terminal — the installer provisions a UTF-8 system locale
/// (`ensure_utf8_locale`) precisely so box-side output never needs an ASCII
/// fallback path. A terminal that still mangles this has a client-side font
/// problem no box-side rendering choice can detect or fix.
pub fn print_qr_block(data: &str) {
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

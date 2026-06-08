//! Auto-top-up end-to-end smoke test against staging.
//!
//! Drives the full v3 flow:
//!   1. Read billing_token + bearer from local box vault (decrypted)
//!   2. /v1/whoami       → confirm initial wallet (post sub-renewal voucher)
//!   3. Repeatedly hit /v1/charge-test with high cost → drain wallet
//!   4. /v1/charge-test once more   → expect 402 insufficient_budget
//!   5. POST atlas /credits/auto-topup with billing_token
//!         → atlas charges sandbox card $10 off-session
//!         → mints {amount: $10, is_renewal: false} voucher
//!         → returns voucher_code
//!   6. POST api /v1/redeem with bearer + voucher_code
//!         → ADDS $10 to wallet (not overwrite)
//!   7. /v1/whoami → confirm wallet ~= $10 + remainder
//!
//! Run with:
//!   cargo run --example auto_topup_smoke

use anyhow::{Context, Result};

const ATLAS: &str = "https://atlas-staging.virtues.com";
const API: &str = "https://api-staging.virtues.com";

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    // Load .env so VIRTUES_ENCRYPTION_KEY + DATABASE_URL flow through.
    dotenv::dotenv().ok();

    let db = std::env::var("DATABASE_URL").context("DATABASE_URL not set")?;
    let pool = sqlx::PgPool::connect(&db).await?;

    // Use the vault helper directly — it handles the AES-GCM decrypt.
    let id_row: (String,) =
        sqlx::query_as("SELECT id FROM credentials WHERE source_id = 'virtues_api' LIMIT 1")
            .fetch_one(&pool)
            .await
            .context("find virtues_api credential")?;
    let secrets = virtues_helpers::auth::vault::read_credential_secrets(&pool, &id_row.0)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))
        .context("decrypt vault secrets")?;

    let billing_token = secrets["billing_token"]
        .as_str()
        .filter(|s| !s.is_empty())
        .context("billing_token missing from vault")?;
    let bearer = secrets["bearer"]
        .as_str()
        .filter(|s| !s.is_empty())
        .context("bearer missing from vault — claim first")?;

    println!("[1] credentials loaded from vault");
    println!("    billing_token: {}…", &billing_token[..16]);
    println!("    bearer:        {}…", &bearer[..16]);

    let http = reqwest::Client::new();

    // Whoami → initial wallet.
    let initial = whoami(&http, bearer).await?;
    println!("[2] initial wallet = ${:.4}", initial as f64 / 1_000_000.0);

    // Drain via charge-test. Use $4/call (under $5 per-call cap).
    println!("[3] draining wallet with $4/call charge-tests…");
    let mut drained = 0i64;
    for i in 1..=5 {
        let resp = http
            .post(format!("{API}/v1/charge-test?cost_micros=4000000"))
            .header("Authorization", format!("Bearer {bearer}"))
            .send()
            .await?;
        let status = resp.status();
        let body: serde_json::Value = resp.json().await.unwrap_or(serde_json::json!({}));
        if status.is_success() {
            drained += 4_000_000;
            let wallet = body["wallet_micros"].as_i64().unwrap_or(0);
            println!("    call {i}: charged $4 → wallet = ${:.4}", wallet as f64 / 1_000_000.0);
        } else {
            let code = body["error"]["code"].as_str().unwrap_or("?");
            println!("    call {i}: {} → {}", status, code);
            if code == "daily_cap_reached" {
                println!("    (hit daily ceiling $20 — that's the safety net working)");
                break;
            }
            if code == "insufficient_budget" || code == "wallet_empty" {
                break;
            }
        }
    }
    println!("    drained ${:.4} total", drained as f64 / 1_000_000.0);

    // Force a 402 by trying to spend $1 more — wallet should be < $1.
    println!("[4] forcing 402 insufficient_budget…");
    let resp = http
        .post(format!("{API}/v1/charge-test?cost_micros=1000000"))
        .header("Authorization", format!("Bearer {bearer}"))
        .send()
        .await?;
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap_or(serde_json::json!({}));
    let code = body["error"]["code"].as_str().unwrap_or("");
    println!("    status: {} | error.code: {}", status, code);

    // Auto-topup via atlas.
    println!("[5] POST atlas /credits/auto-topup with billing_token…");
    let resp = http
        .post(format!("{ATLAS}/credits/auto-topup"))
        .json(&serde_json::json!({ "billing_token": billing_token }))
        .send()
        .await?;
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap_or(serde_json::json!({}));
    if !status.is_success() {
        let code = body["error"]["code"].as_str().unwrap_or("?");
        let msg = body["error"]["message"].as_str().unwrap_or("");
        println!("    FAILED: {} — {} ({})", status, code, msg);
        if code == "card_declined" {
            let sc = body["error"]["stripe_code"].as_str().unwrap_or("?");
            println!("    stripe_code: {sc}");
        }
        anyhow::bail!("auto-topup failed; cannot continue");
    }
    let voucher_code = body["voucher_code"]
        .as_str()
        .context("no voucher_code in response")?
        .to_string();
    let amount = body["amount_micros"].as_i64().unwrap_or(0);
    println!("    Stripe sandbox charged ${:.2}, voucher minted: {}…",
             amount as f64 / 1_000_000.0, &voucher_code[..16]);

    // Redeem voucher onto same bearer.
    println!("[6] POST api /v1/redeem (top-up, is_renewal=false → ADDS to wallet)…");
    let resp = http
        .post(format!("{API}/v1/redeem"))
        .header("Authorization", format!("Bearer {bearer}"))
        .json(&serde_json::json!({ "voucher_code": voucher_code }))
        .send()
        .await?;
    let body: serde_json::Value = resp.json().await.unwrap_or(serde_json::json!({}));
    println!("    redeem response: {}", serde_json::to_string_pretty(&body).unwrap_or_default());

    // Final whoami.
    let after = whoami(&http, bearer).await?;
    println!("[7] post-topup wallet = ${:.4}", after as f64 / 1_000_000.0);

    println!();
    println!("✅ auto-top-up smoke complete.");
    println!("   Initial:      ${:.4}", initial as f64 / 1_000_000.0);
    println!("   Drained:      ${:.4}", drained as f64 / 1_000_000.0);
    println!("   After top-up: ${:.4}", after as f64 / 1_000_000.0);
    println!("   Net gain from top-up: ${:.4}", (after - initial + drained) as f64 / 1_000_000.0);

    Ok(())
}

async fn whoami(http: &reqwest::Client, bearer: &str) -> Result<i64> {
    let resp = http
        .get(format!("{API}/v1/whoami"))
        .header("Authorization", format!("Bearer {bearer}"))
        .send()
        .await?;
    let body: serde_json::Value = resp.json().await?;
    Ok(body["wallet_micros"].as_i64().unwrap_or(0))
}

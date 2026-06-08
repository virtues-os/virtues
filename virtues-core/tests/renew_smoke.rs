//! Manual smoke test for the virtues-api voucher renewal (WS-6b).
//!
//! Requires running services + databases. Run with:
//!
//!   DATABASE_URL=postgres://virtues:virtues@localhost:5432/virtues \
//!   VIRTUES_ENCRYPTION_KEY=<base64-32-bytes> \
//!   VIRTUES_ATLAS_URL=http://localhost:9100 \
//!   VIRTUES_API_URL=http://localhost:9002 \
//!   BILLING_TOKEN=<token-also-seeded-into-atlas> \
//!   cargo test -p virtues --test renew_smoke -- --ignored --nocapture
//!
//! The caller must seed Atlas with a customer + active subscription whose
//! billing_token_hash = sha256(BILLING_TOKEN) before running.

#[tokio::test]
#[ignore]
async fn renew_end_to_end() {
    let billing_token = std::env::var("BILLING_TOKEN").expect("BILLING_TOKEN");
    let atlas_url = std::env::var("VIRTUES_ATLAS_URL").expect("VIRTUES_ATLAS_URL");
    let api_url = std::env::var("VIRTUES_API_URL").expect("VIRTUES_API_URL");

    let pool = virtues_helpers::connect_from_env("renew-smoke")
        .await
        .expect("connect");

    // Seed the billing token into the local vault (the real claim store path).
    virtues::virtues_api::renew::store_billing_token(&pool, &billing_token)
        .await
        .expect("store_billing_token");

    // Before renewal there is no bearer.
    let before = virtues::virtues_api::renew::current_bearer(&pool)
        .await
        .expect("current_bearer");
    assert!(before.is_none(), "expected no bearer before renewal");

    let http = reqwest::Client::new();

    // Run the voucher dance.
    let res = virtues::virtues_api::renew::renew(&pool, &http, &atlas_url, &api_url)
        .await
        .expect("renew");
    println!("renewed: bearer_len={} expires_at={}", res.bearer.len(), res.expires_at);
    assert_eq!(res.bearer.len(), 64, "bearer should be 32 bytes hex");
    assert!(res.expires_at > chrono::Utc::now(), "expiry should be future");

    // After renewal the vault has the bearer.
    let after = virtues::virtues_api::renew::current_bearer(&pool)
        .await
        .expect("current_bearer after");
    let (bearer, expiry) = after.expect("expected a bearer after renewal");
    assert_eq!(bearer, res.bearer, "stored bearer should match returned");
    assert!(expiry.is_some(), "expiry should be recorded");
    println!("vault now holds bearer with expiry {:?}", expiry);
}

/// BearerClient should auto-renew when no bearer exists yet, then make a
/// real AI call. Requires the same seeded billing token + a virtues-api
/// with AI_GATEWAY_API_KEY configured.
#[tokio::test]
#[ignore]
async fn bearer_client_auto_renews_and_calls() {
    let billing_token = std::env::var("BILLING_TOKEN").expect("BILLING_TOKEN");

    let pool = virtues_helpers::connect_from_env("bearer-client-smoke")
        .await
        .expect("connect");

    // Seed billing token; no bearer minted yet.
    virtues::virtues_api::renew::store_billing_token(&pool, &billing_token)
        .await
        .expect("store_billing_token");

    let client = virtues::virtues_api::client::BearerClient::from_env(pool.clone());

    // First call: no bearer → BearerClient auto-renews, then makes the call.
    let resp = client
        .post_json(
            "/v1/ai/chat/completions",
            &serde_json::json!({
                "model": "google/gemini-3-flash",
                "messages": [{"role": "user", "content": "Say hi in one word."}],
                "max_tokens": 10
            }),
        )
        .await
        .expect("post_json");

    println!("status={} body_keys={:?}", resp.status, resp.body.as_object().map(|o| o.keys().collect::<Vec<_>>()));
    assert!(resp.is_success(), "expected 2xx, got {}: {}", resp.status, resp.body);
    assert!(
        resp.body["choices"][0]["message"]["content"].is_string(),
        "expected a chat completion"
    );

    // The bearer should now be present in the vault.
    let after = virtues::virtues_api::renew::current_bearer(&pool)
        .await
        .expect("current_bearer");
    assert!(after.is_some(), "bearer should be minted after auto-renew");
    println!("auto-renew + AI call succeeded; cost charged to device bearer");
}

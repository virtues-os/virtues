//! Manual smoke test for the linked prepaid model (device api_key).
//!
//! Requires running services + databases. Run with:
//!
//!   DATABASE_URL=postgres://virtues:virtues@localhost:5432/virtues \
//!   VIRTUES_ENCRYPTION_KEY=<base64-32-bytes> \
//!   VIRTUES_API_URL=http://localhost:9002 \
//!   VIRTUES_API_KEY=<key-registered-against-a-funded-account-in-virtues-api> \
//!   cargo test -p virtues --test renew_smoke -- --ignored --nocapture
//!
//! The caller must register the api_key with virtues-api (POST /internal/device)
//! against a funded account before running.

#[tokio::test]
#[ignore]
async fn api_key_store_roundtrip() {
    let api_key = std::env::var("VIRTUES_API_KEY").expect("VIRTUES_API_KEY");

    let pool = virtues_helpers::connect_from_env("api-key-smoke")
        .await
        .expect("connect");

    // Before storing, no key is linked.
    assert!(
        !virtues::virtues_api::renew::has_api_key(&pool)
            .await
            .expect("has_api_key"),
        "expected no api_key before store"
    );

    // Store the key into the vault (the real link store path).
    virtues::virtues_api::renew::store_api_key(&pool, &api_key)
        .await
        .expect("store_api_key");

    let read = virtues::virtues_api::renew::read_api_key(&pool)
        .await
        .expect("read_api_key")
        .expect("expected a stored api_key");
    assert_eq!(read, api_key, "stored api_key should round-trip");
}

/// BearerClient should attach the stored api_key and make a real AI call (the
/// wallet behind the key must be funded in virtues-api). No renewal happens —
/// the key is stable.
#[tokio::test]
#[ignore]
async fn bearer_client_calls_with_api_key() {
    let api_key = std::env::var("VIRTUES_API_KEY").expect("VIRTUES_API_KEY");

    let pool = virtues_helpers::connect_from_env("bearer-client-smoke")
        .await
        .expect("connect");

    virtues::virtues_api::renew::store_api_key(&pool, &api_key)
        .await
        .expect("store_api_key");

    let client = virtues::virtues_api::client::BearerClient::from_env(pool.clone());

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

    println!(
        "status={} body_keys={:?}",
        resp.status,
        resp.body.as_object().map(|o| o.keys().collect::<Vec<_>>())
    );
    assert!(resp.is_success(), "expected 2xx, got {}: {}", resp.status, resp.body);
    assert!(
        resp.body["choices"][0]["message"]["content"].is_string(),
        "expected a chat completion"
    );
    println!("AI call succeeded; cost charged to the account wallet via the device api_key");
}

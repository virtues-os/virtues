//! Registration auth (#5): with a relay secret set, a box may register only the
//! SNI its token was minted for. A wrong token, or a valid token for a *different*
//! SNI, is rejected — closing the cross-tenant hijack a flat shared bearer left.

use std::time::Duration;
use tokio::net::TcpListener;
use virtues_protocol::relay::derive_token;
use virtues_relay::config::Config;
use virtues_relay::state::AppState;
use virtues_relay_client::{serve_once, RelayClientConfig};

/// Start a relay with the given per-SNI `secret`; return (state, control_addr).
/// The returned state is a clone sharing the same registry, so the test can
/// observe registrations.
async fn start_relay(secret: Option<String>) -> (AppState, String) {
    let client_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client_addr = client_listener.local_addr().unwrap().to_string();
    let control_addr = control_listener.local_addr().unwrap().to_string();

    let state = AppState::new(Config {
        client_addr,
        control_addr: control_addr.clone(),
        secret,
        token: "shared-unused".into(),
    });
    let observe = state.clone();
    tokio::spawn(virtues_relay::serve(state, client_listener, control_listener));
    (observe, control_addr)
}

fn cfg(addr: &str, sni: &str, token: &str, read_timeout: Duration) -> RelayClientConfig {
    RelayClientConfig {
        relay_addr: addr.into(),
        sni: sni.into(),
        token: token.into(),
        local_addr: "127.0.0.1:1".into(), // unused; no OpenConn is sent
        read_timeout: Some(read_timeout),
        registered: None,
        token_cell: None,
    }
}

async fn wait_until(mut cond: impl FnMut() -> bool, max: Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < max {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    cond()
}

#[tokio::test]
async fn hmac_secret_gates_registration_by_sni() {
    let secret = "relay-secret";
    let sni = "abc.boxes.virtues.com";
    let (state, addr) = start_relay(Some(secret.into())).await;

    // 1. Wrong token → rejected; nothing registered.
    let short = Duration::from_millis(200);
    let r = serve_once(&cfg(&addr, sni, "totally-wrong", short)).await;
    assert!(r.is_err(), "relay accepted a bad token");
    assert!(
        state.registry.lookup(sni).is_none(),
        "a rejected box must not appear in the registry"
    );

    // 2. A token minted for a DIFFERENT SNI must not register `sni` (the hijack
    //    case): a box holding its own valid token can't claim another's name.
    let bucket = virtues_protocol::relay::current_bucket();
    let foreign = derive_token(secret, "victim.boxes.virtues.com", bucket);
    let r = serve_once(&cfg(&addr, sni, &foreign, short)).await;
    assert!(r.is_err(), "relay accepted a token minted for a different SNI");
    assert!(state.registry.lookup(sni).is_none());

    // 3. The correct per-SNI token registers. serve_once blocks after Register,
    //    so drive it detached (long read timeout so the entry persists) and poll.
    let good = derive_token(secret, sni, bucket);
    let good_cfg = cfg(&addr, sni, &good, Duration::from_secs(30));
    tokio::spawn(async move {
        let _ = serve_once(&good_cfg).await;
    });
    let registered =
        wait_until(|| state.registry.lookup(sni).is_some(), Duration::from_secs(5)).await;
    assert!(registered, "relay rejected the correct per-SNI token");
}

#[tokio::test]
async fn token_expires_outside_current_or_previous_bucket() {
    // Revocation mechanism: the relay accepts the current and previous bucket
    // (±1, for clock skew / day boundary) but rejects an older token. atlas stops
    // minting for a revoked box, so its token falls out of this window.
    let secret = "relay-secret";
    let sni = "bucketed.boxes.virtues.com";
    let (state, addr) = start_relay(Some(secret.into())).await;
    let now = virtues_protocol::relay::current_bucket();

    // Two buckets old → rejected (expired).
    let stale = derive_token(secret, sni, now - 2);
    let r = serve_once(&cfg(&addr, sni, &stale, Duration::from_millis(200))).await;
    assert!(r.is_err(), "relay accepted a two-bucket-old token");
    assert!(state.registry.lookup(sni).is_none());

    // Previous bucket → still accepted (the ±1 grace window).
    let prev = derive_token(secret, sni, now - 1);
    let good_cfg = cfg(&addr, sni, &prev, Duration::from_secs(30));
    tokio::spawn(async move {
        let _ = serve_once(&good_cfg).await;
    });
    let registered =
        wait_until(|| state.registry.lookup(sni).is_some(), Duration::from_secs(5)).await;
    assert!(registered, "relay rejected a previous-bucket token");
}

#[tokio::test]
async fn shared_bearer_path_when_no_secret() {
    // With no secret configured the relay falls back to the shared bearer.
    let (state, addr) = start_relay(None).await;
    let sni = "lan.boxes.virtues.com";

    // Wrong shared token → rejected.
    let short = Duration::from_millis(200);
    let r = serve_once(&cfg(&addr, sni, "nope", short)).await;
    assert!(r.is_err(), "relay accepted a wrong shared bearer");

    // Correct shared token ("shared-unused") → registers.
    let good_cfg = cfg(&addr, sni, "shared-unused", Duration::from_secs(30));
    tokio::spawn(async move {
        let _ = serve_once(&good_cfg).await;
    });
    let registered =
        wait_until(|| state.registry.lookup(sni).is_some(), Duration::from_secs(5)).await;
    assert!(registered, "relay rejected the correct shared bearer");
}

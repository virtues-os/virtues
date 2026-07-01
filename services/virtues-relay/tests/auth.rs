//! Registration auth (#5/#6'): the relay verifies an **atlas-signed** Ed25519
//! token, so a box may register only the SNI its token was signed for. A wrong
//! token, a token for a *different* SNI, or a token signed by a *different* key is
//! rejected — and the relay holds only the public key, so it can never mint.

use ed25519_dalek::{SigningKey, VerifyingKey};
use std::time::Duration;
use tokio::net::TcpListener;
use virtues_protocol::relay::sign_token;
use virtues_relay::config::Config;
use virtues_relay::state::AppState;
use virtues_relay_client::{serve_once, RelayClientConfig};

/// Deterministic Ed25519 keypair for tests (atlas = signer, relay = verifier).
fn keypair(seed: u8) -> (SigningKey, VerifyingKey) {
    let sk = SigningKey::from_bytes(&[seed; 32]);
    let vk = sk.verifying_key();
    (sk, vk)
}

/// Start a relay holding `public_key` (atlas's verifying key); return (state,
/// control_addr). The returned state clones the shared registry so the test can
/// observe registrations.
async fn start_relay(public_key: Option<VerifyingKey>) -> (AppState, String) {
    start_relay_rotating(public_key, None).await
}

/// Like [`start_relay`] but also configures a previous public key (key rotation).
async fn start_relay_rotating(
    public_key: Option<VerifyingKey>,
    public_key_prev: Option<VerifyingKey>,
) -> (AppState, String) {
    let client_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client_addr = client_listener.local_addr().unwrap().to_string();
    let control_addr = control_listener.local_addr().unwrap().to_string();

    let state = AppState::new(Config {
        client_addr,
        control_addr: control_addr.clone(),
        public_key,
        public_key_prev,
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
async fn signed_token_gates_registration_by_sni() {
    let (sk, vk) = keypair(1);
    let sni = "abc.boxes.virtues.com";
    let (state, addr) = start_relay(Some(vk)).await;
    let bucket = virtues_protocol::relay::current_bucket();

    // 1. Garbage token → rejected; nothing registered.
    let short = Duration::from_millis(200);
    let r = serve_once(&cfg(&addr, sni, "totally-wrong", short)).await;
    assert!(r.is_err(), "relay accepted a bad token");
    assert!(
        state.registry.lookup(sni).is_none(),
        "a rejected box must not appear in the registry"
    );

    // 2. A token signed for a DIFFERENT SNI must not register `sni` (the hijack
    //    case): the signature is over "<sni>:<bucket>", so it won't verify here.
    let foreign = sign_token(&sk, "victim.boxes.virtues.com", bucket);
    let r = serve_once(&cfg(&addr, sni, &foreign, short)).await;
    assert!(r.is_err(), "relay accepted a token signed for a different SNI");
    assert!(state.registry.lookup(sni).is_none());

    // 3. The correct per-SNI signed token registers. serve_once blocks after
    //    Register, so drive it detached (long read timeout) and poll.
    let good = sign_token(&sk, sni, bucket);
    let good_cfg = cfg(&addr, sni, &good, Duration::from_secs(30));
    tokio::spawn(async move {
        let _ = serve_once(&good_cfg).await;
    });
    let registered =
        wait_until(|| state.registry.lookup(sni).is_some(), Duration::from_secs(5)).await;
    assert!(registered, "relay rejected the correct per-SNI signed token");
}

#[tokio::test]
async fn token_expires_outside_current_or_previous_bucket() {
    // Revocation mechanism: the relay accepts the current and previous bucket
    // (±1, for clock skew / day boundary) but rejects an older token. atlas stops
    // signing for a revoked box, so its token falls out of this window.
    let (sk, vk) = keypair(2);
    let sni = "bucketed.boxes.virtues.com";
    let (state, addr) = start_relay(Some(vk)).await;
    let now = virtues_protocol::relay::current_bucket();

    // Two buckets old → rejected (expired).
    let stale = sign_token(&sk, sni, now - 2);
    let r = serve_once(&cfg(&addr, sni, &stale, Duration::from_millis(200))).await;
    assert!(r.is_err(), "relay accepted a two-bucket-old token");
    assert!(state.registry.lookup(sni).is_none());

    // Previous bucket → still accepted (the ±1 grace window).
    let prev = sign_token(&sk, sni, now - 1);
    let good_cfg = cfg(&addr, sni, &prev, Duration::from_secs(30));
    tokio::spawn(async move {
        let _ = serve_once(&good_cfg).await;
    });
    let registered =
        wait_until(|| state.registry.lookup(sni).is_some(), Duration::from_secs(5)).await;
    assert!(registered, "relay rejected a previous-bucket token");
}

#[tokio::test]
async fn key_rotation_accepts_old_and_new_signing_key() {
    // Zero-downtime KEY rotation: the relay runs with the NEW public key as
    // primary and the OLD public key as `public_key_prev`. A box still presenting
    // a token signed by the old key must keep registering (it re-fetches a
    // new-key token on its next refresh); a token signed by an unknown key fails.
    let (old_sk, old_vk) = keypair(3);
    let (_new_sk, new_vk) = keypair(4);
    let sni = "rotating.boxes.virtues.com";
    let (state, addr) = start_relay_rotating(Some(new_vk), Some(old_vk)).await;
    let bucket = virtues_protocol::relay::current_bucket();

    // Token signed under the OLD key → still accepted during the window.
    let old_tok = sign_token(&old_sk, sni, bucket);
    let old_cfg = cfg(&addr, sni, &old_tok, Duration::from_secs(30));
    tokio::spawn(async move {
        let _ = serve_once(&old_cfg).await;
    });
    assert!(
        wait_until(|| state.registry.lookup(sni).is_some(), Duration::from_secs(5)).await,
        "relay rejected an old-key token during rotation"
    );

    // A token signed by neither key is still rejected (rotation isn't a free pass).
    let (rogue_sk, _rogue_vk) = keypair(9);
    let bad = sign_token(&rogue_sk, "other.boxes.virtues.com", bucket);
    let r = serve_once(&cfg(&addr, "other.boxes.virtues.com", &bad, Duration::from_millis(200))).await;
    assert!(r.is_err(), "relay accepted a token signed by an unknown key");
}

#[tokio::test]
async fn shared_bearer_path_when_no_public_key() {
    // With no public key configured the relay falls back to the shared bearer.
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

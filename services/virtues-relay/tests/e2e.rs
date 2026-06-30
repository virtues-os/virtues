//! End-to-end: a real box client registers with the relay; a mock browser opens
//! a TLS-shaped connection (ClientHello carrying the box's SNI) plus a payload;
//! the bytes must round-trip through relay → box-client → the box's local service
//! and back — proving SNI routing, the OpenConn/work-conn dance, the ClientHello
//! buffer-replay, and the bidirectional splice. The relay never decrypts.

use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use virtues_relay::config::Config;
use virtues_relay::state::AppState;

/// Minimal valid TLS 1.2 ClientHello record carrying `sni` (mirrors the unit-test
/// helper; kept local so the test is self-contained).
fn client_hello_with_sni(sni: &str) -> Vec<u8> {
    let host = sni.as_bytes();
    let mut server_name = vec![0u8]; // host_name
    server_name.extend_from_slice(&(host.len() as u16).to_be_bytes());
    server_name.extend_from_slice(host);

    let mut sni_list = (server_name.len() as u16).to_be_bytes().to_vec();
    sni_list.extend_from_slice(&server_name);

    let mut sni_ext = 0u16.to_be_bytes().to_vec(); // ext type 0 = server_name
    sni_ext.extend_from_slice(&(sni_list.len() as u16).to_be_bytes());
    sni_ext.extend_from_slice(&sni_list);

    let mut extensions = (sni_ext.len() as u16).to_be_bytes().to_vec();
    extensions.extend_from_slice(&sni_ext);

    let mut body = vec![0x03, 0x03]; // client_version
    body.extend_from_slice(&[0u8; 32]); // random
    body.push(0u8); // session_id len
    body.extend_from_slice(&2u16.to_be_bytes()); // cipher suites len
    body.extend_from_slice(&[0x13, 0x01]);
    body.push(1u8); // compression methods len
    body.push(0u8);
    body.extend_from_slice(&extensions);

    let len = body.len();
    let mut handshake = vec![1u8, (len >> 16) as u8, (len >> 8) as u8, len as u8];
    handshake.extend_from_slice(&body);

    let mut record = vec![22u8, 0x03, 0x01];
    record.extend_from_slice(&(handshake.len() as u16).to_be_bytes());
    record.extend_from_slice(&handshake);
    record
}

/// A trivial echo server standing in for the box's local TLS service. The relay
/// is L4, so it doesn't care that this isn't real TLS — it only moves bytes.
async fn spawn_echo() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    tokio::spawn(async move {
        loop {
            let (mut sock, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let (mut r, mut w) = sock.split();
                let _ = tokio::io::copy(&mut r, &mut w).await;
            });
        }
    });
    addr
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn browser_reaches_box_through_relay() {
    let _ = tracing_subscriber::fmt::try_init();

    let sni = "box1.boxes.virtues.com";
    let token = "test-token";

    // --- relay ---
    let client_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client_addr = client_listener.local_addr().unwrap();
    let control_addr = control_listener.local_addr().unwrap();

    let state = AppState::new(Config {
        client_addr: client_addr.to_string(),
        control_addr: control_addr.to_string(),
        secret: None, // shared-bearer path; per-SNI HMAC is covered by a unit test
        token: token.to_string(),
    });
    let state_probe = state.clone();
    tokio::spawn(virtues_relay::serve(state, client_listener, control_listener));

    // --- box: local echo service + relay client ---
    let echo_addr = spawn_echo().await;
    let cfg = virtues_relay_client::RelayClientConfig {
        relay_addr: control_addr.to_string(),
        sni: sni.to_string(),
        token: token.to_string(),
        local_addr: echo_addr,
        read_timeout: None,
        registered: None,
    };
    tokio::spawn(async move {
        // One lifecycle is enough for the test (no splay/backoff).
        let _ = virtues_relay_client::serve_once(&cfg).await;
    });

    // Wait for the box to register (confirm-by-state, not a fixed sleep).
    let registered = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if state_probe.registry.lookup(sni).is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(registered.is_ok(), "box did not register in time");

    // --- mock browser ---
    let payload = b"hello-virtues-relay-e2e";
    let mut sent = client_hello_with_sni(sni);
    sent.extend_from_slice(payload);

    let result = tokio::time::timeout(Duration::from_secs(5), async {
        let mut browser = TcpStream::connect(client_addr).await.unwrap();
        browser.write_all(&sent).await.unwrap();
        browser.flush().await.unwrap();
        let mut got = vec![0u8; sent.len()];
        browser.read_exact(&mut got).await.unwrap();
        got
    })
    .await
    .expect("round-trip timed out");

    // The echo service mirrors everything it received — which is the replayed
    // ClientHello followed by the payload — so the browser gets back exactly what
    // it sent. This proves the full path incl. buffer-replay.
    assert_eq!(result, sent, "bytes did not round-trip intact through the relay");
}

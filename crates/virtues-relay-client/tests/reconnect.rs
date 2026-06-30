//! Liveness (#1): a box must detect a *silently* dead relay — one that ack'd the
//! registration then stopped sending pings without ever closing the TCP socket
//! (kernel panic / blackhole / NAT idle-drop). Without the control-loop read
//! timeout the box would block in `read` forever and never reconnect.

use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use virtues_relay_client::{run, serve_once, RelayClientConfig};

/// Read one newline-delimited line (the box's `Register`) and discard it.
async fn read_line(sock: &mut TcpStream) {
    let mut byte = [0u8; 1];
    loop {
        match sock.read(&mut byte).await {
            Ok(0) | Err(_) => return,
            Ok(_) => {
                if byte[0] == b'\n' {
                    return;
                }
            }
        }
    }
}

const REGISTERED: &[u8] = b"{\"type\":\"registered\"}\n";

fn cfg(addr: String) -> RelayClientConfig {
    RelayClientConfig {
        relay_addr: addr,
        sni: "box.test".into(),
        token: "t".into(),
        local_addr: "127.0.0.1:1".into(), // unused; no OpenConn is ever sent
        read_timeout: Some(Duration::from_millis(150)),
        registered: None,
        token_cell: None,
    }
}

/// The keystone, fully deterministic: against a relay that acks then goes silent,
/// `serve_once` must *return an error promptly* (via the read timeout) rather
/// than hang. Before the fix this blocked forever.
#[tokio::test]
async fn serve_once_errors_when_relay_goes_silent() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    tokio::spawn(async move {
        if let Ok((mut sock, _)) = listener.accept().await {
            read_line(&mut sock).await;
            let _ = sock.write_all(REGISTERED).await;
            // Hold the socket open but never ping and never close — the silent
            // relay. A box with no read timeout would block here indefinitely.
            std::future::pending::<()>().await;
        }
    });

    let start = std::time::Instant::now();
    let r = serve_once(&cfg(addr)).await;
    assert!(r.is_err(), "serve_once must error when the relay goes silent");
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "serve_once did not time out promptly (took {:?})",
        start.elapsed()
    );
}

/// #10: the `registered` flag is `true` only while a control connection is live.
/// It flips true once `Register` is acked, and the drop guard clears it the moment
/// `serve_once` returns (here, when the silent relay trips the read timeout).
#[tokio::test]
async fn registered_flag_tracks_control_connection() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    tokio::spawn(async move {
        if let Ok((mut sock, _)) = listener.accept().await {
            read_line(&mut sock).await;
            let _ = sock.write_all(REGISTERED).await;
            std::future::pending::<()>().await; // ack, then go silent
        }
    });

    let flag = Arc::new(AtomicBool::new(false));
    let mut c = cfg(addr);
    c.registered = Some(flag.clone());
    let handle = tokio::spawn(async move { serve_once(&c).await });

    // Becomes true shortly after the Register ack.
    let mut saw_registered = false;
    for _ in 0..100 {
        if flag.load(Ordering::Relaxed) {
            saw_registered = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(saw_registered, "flag must be true while registered");

    // The 150ms read timeout fires (silent relay) → serve_once returns → guard
    // clears the flag.
    let _ = handle.await.unwrap();
    assert!(
        !flag.load(Ordering::Relaxed),
        "flag must clear when the control link drops"
    );
}

/// End to end: `run()` keeps reconnecting — after the silent relay trips the read
/// timeout, the box backs off and re-registers. Paused clock so the startup splay
/// and backoff sleeps don't cost real seconds.
#[tokio::test(start_paused = true)]
async fn run_reconnects_after_silent_relay() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();

    tokio::spawn(async move {
        loop {
            let (mut sock, _) = match listener.accept().await {
                Ok(v) => v,
                Err(_) => break,
            };
            let tx = tx.clone();
            tokio::spawn(async move {
                read_line(&mut sock).await;
                let _ = sock.write_all(REGISTERED).await;
                let _ = tx.send(()); // a registration happened
                std::future::pending::<()>().await; // then go silent
            });
        }
    });

    tokio::spawn(run(cfg(addr)));

    // Initial registration (after the startup splay), then a SECOND one after the
    // silent relay trips the timeout and backoff elapses — i.e. it reconnected.
    assert!(rx.recv().await.is_some(), "box never registered initially");
    assert!(
        rx.recv().await.is_some(),
        "box did not reconnect after the relay went silent"
    );
}

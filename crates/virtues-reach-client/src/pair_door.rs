//! The pairing door — a time-boxed LAN opening onto the box's pair endpoint.
//!
//! Pairing is structurally LAN-only: a device can't use iroh until its
//! EndpointId is allowlisted, allowlisting happens at pairing, and pairing
//! therefore has to be plain HTTP straight to the box. So a phone standing in
//! a café cannot enroll against a box sitting at home, however reachable that
//! box is to everything already paired.
//!
//! An already-paired laptop is the way out. It reaches the box from anywhere
//! (LAN-direct → hole-punched → relay) and it is standing next to the phone.
//! This module lets it hold a door open on its own LAN address for the length
//! of one "Add device" window: the phone types the laptop's address into the
//! pairing screen it already has, and `POST /api/pair/consume` — whose reply
//! already carries `box_node_id`, `relay_url` and the direct addrs — completes
//! the enrollment in a single round trip. Nothing new is exchanged, nothing
//! comes back out of band, and the phone's secret is still minted on the phone.
//!
//! # What it needs from the network, and where that bites
//!
//! The phone must be able to reach the laptop directly. That is trivially true
//! on a home or office LAN and NOT true on much of the guest wifi this feature
//! exists to serve: coworking and hotel networks routinely enable client
//! isolation, which lets every device reach the internet and none of them reach
//! each other. Verified on WeWork wifi 2026-08-27 — the laptop reached the box
//! at home through this door (a real 422 came back from the box's consume) and
//! the phone could not open a socket to the laptop three feet away, failing
//! with `tcp connect error: Host is down`.
//!
//! The reliable answer there is the phone's own hotspot: put the laptop on it,
//! and the two share a private network with no isolation while the laptop keeps
//! its route to the box. Worth saying in the UI rather than leaving to be
//! rediscovered — the failure names the laptop, so it reads as a broken door.
//!
//! # Why this is not `proxy::serve_on`
//!
//! The loopback proxy raw-splices a TCP connection onto an iroh bi-stream, and
//! the box authenticates that stream as *this laptop* (`ProvenPeer`, stamped
//! post-handshake in `virtues-iroh::server`). Splicing that onto a LAN would
//! hand every peer on a café network the laptop's full owner authority. So
//! this terminates HTTP itself, admits exactly one route, and rebuilds the
//! request from parsed parts rather than forwarding what it was handed.
//!
//! # Why the door carries its own rate limit
//!
//! The box rate-limits `consume` on the socket peer to defend the 6-digit code
//! space — and explicitly exempts loopback. Requests arriving over iroh carry
//! no `ConnectInfo` at all (the server stamps `ProvenPeer` and nothing else),
//! so **the box's limiter does not run on anything this door forwards**. That
//! is harmless for the loopback proxy, where the only callers are already
//! paired; it would be a full-speed brute force here, and the prize is a
//! permanent allowlisted device reachable from anywhere via the relay. The
//! budget below is therefore not defence in depth — it is the only limit in
//! the path.

use anyhow::{bail, Context, Result};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use virtues_iroh::VirtuesIrohClient;

/// The one route the door forwards. Anything else is refused before a single
/// byte reaches the box.
const ALLOWED_REQUEST_LINE: &str = "POST /api/pair/consume ";

/// Caps on what an unauthenticated LAN peer can make us buffer.
const MAX_HEAD: usize = 8 * 1024;
const MAX_BODY: usize = 64 * 1024;

/// A slow-loris on a café network shouldn't hold a connection open.
const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// Total `consume` attempts a single door may forward in its lifetime. The
/// window is minutes and the honest path needs one attempt (a mistyped code
/// costs a second), so ten is generous for a human and useless for a search
/// of a 10^6 space.
const MAX_ATTEMPTS: u32 = 10;

/// How an admitted request reaches the box. Concrete callers pass the warm
/// iroh client's `request`; tests pass a stub, which is what lets the refusal
/// rules below be verified over a real socket without a box on the other end.
pub trait Forwarder: Send + Sync + 'static {
    fn forward(
        &self,
        raw: Vec<u8>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>>> + Send>>;
}

impl Forwarder for Arc<VirtuesIrohClient> {
    fn forward(
        &self,
        raw: Vec<u8>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>>> + Send>> {
        let client = self.clone();
        Box::pin(async move { client.request(&raw).await })
    }
}

/// Serve the pairing door until `shutdown` resolves.
///
/// `attempts` is shared with the caller so the host can report the remaining
/// budget and close early once it is spent.
pub async fn serve_pair_door<F>(
    listener: TcpListener,
    client: Arc<dyn Forwarder>,
    attempts: Arc<AtomicU32>,
    shutdown: F,
) -> Result<()>
where
    F: std::future::Future<Output = ()> + Send,
{
    tracing::info!(addr = ?listener.local_addr().ok(), "pair door: open");
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            accepted = listener.accept() => {
                let (stream, peer) = match accepted {
                    Ok(x) => x,
                    Err(e) => {
                        tracing::debug!(error = %e, "pair door: accept error");
                        continue;
                    }
                };
                let client = client.clone();
                let attempts = attempts.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle(stream, client, attempts).await {
                        // Debug, not warn: a closed browser tab and a port
                        // scanner both land here, and neither is an event.
                        tracing::debug!(peer = %peer, error = %format!("{e:#}"), "pair door: connection ended");
                    }
                });
            }
        }
    }
    tracing::info!("pair door: closed");
    Ok(())
}

async fn handle(
    mut stream: TcpStream,
    client: Arc<dyn Forwarder>,
    attempts: Arc<AtomicU32>,
) -> Result<()> {
    let (head, leftover) = match read_head(&mut stream).await {
        Ok(v) => v,
        Err(e) => {
            let _ = respond(&mut stream, 400, "bad_request").await;
            return Err(e);
        }
    };

    // One route, matched on the literal request line. No prefix matching, no
    // path normalization to get wrong, no query string: the box's consume
    // takes everything in its JSON body.
    if !head.starts_with(ALLOWED_REQUEST_LINE) {
        respond(&mut stream, 403, "pair_door_closed_to_this_route").await?;
        return Ok(());
    }

    // Spend budget before touching the box, so a refused attempt still costs.
    let used = attempts.fetch_add(1, Ordering::SeqCst);
    if used >= MAX_ATTEMPTS {
        respond(&mut stream, 429, "too_many_attempts").await?;
        return Ok(());
    }

    let len = content_length(&head)?;
    if len > MAX_BODY {
        respond(&mut stream, 413, "body_too_large").await?;
        return Ok(());
    }
    let body = read_body(&mut stream, leftover, len).await?;

    // REBUILT, never forwarded. The client's own headers are dropped on the
    // floor here — `X-Forwarded-For` in particular, which the box's limiter
    // was taught not to trust after exactly this attack, plus anything else a
    // café peer might try to smuggle down an authenticated stream.
    let mut raw = Vec::with_capacity(body.len() + 160);
    raw.extend_from_slice(b"POST /api/pair/consume HTTP/1.1\r\n");
    raw.extend_from_slice(b"Host: virtues\r\n");
    raw.extend_from_slice(b"Content-Type: application/json\r\n");
    raw.extend_from_slice(format!("Content-Length: {}\r\n", body.len()).as_bytes());
    raw.extend_from_slice(b"Connection: close\r\n\r\n");
    raw.extend_from_slice(&body);

    match client.forward(raw).await {
        Ok(resp) => {
            stream.write_all(&resp).await.context("write response")?;
        }
        Err(e) => {
            tracing::warn!(error = %format!("{e:#}"), "pair door: box request failed");
            respond(&mut stream, 502, "box_unreachable").await?;
        }
    }
    let _ = stream.shutdown().await;
    Ok(())
}

/// Read up to the end of the request head, returning it plus whatever body
/// bytes arrived in the same read.
async fn read_head(stream: &mut TcpStream) -> Result<(String, Vec<u8>)> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    loop {
        let n = tokio::time::timeout(READ_TIMEOUT, stream.read(&mut chunk))
            .await
            .context("read timeout")?
            .context("read")?;
        if n == 0 {
            bail!("connection closed before request head");
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = find_head_end(&buf) {
            let head = String::from_utf8_lossy(&buf[..pos]).to_string();
            return Ok((head, buf[pos + 4..].to_vec()));
        }
        if buf.len() > MAX_HEAD {
            bail!("request head too large");
        }
    }
}

fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

fn content_length(head: &str) -> Result<usize> {
    for line in head.lines().skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("content-length") {
            return value
                .trim()
                .parse::<usize>()
                .context("parse content-length");
        }
    }
    Ok(0)
}

async fn read_body(stream: &mut TcpStream, mut have: Vec<u8>, want: usize) -> Result<Vec<u8>> {
    while have.len() < want {
        let mut chunk = vec![0u8; (want - have.len()).min(8 * 1024)];
        let n = tokio::time::timeout(READ_TIMEOUT, stream.read(&mut chunk))
            .await
            .context("body read timeout")?
            .context("body read")?;
        if n == 0 {
            bail!("connection closed mid-body");
        }
        have.extend_from_slice(&chunk[..n]);
    }
    have.truncate(want);
    Ok(have)
}

/// A minimal JSON error, in the shape the box's own pair errors use so the
/// pairing screen renders a door refusal the same way it renders a box one.
async fn respond(stream: &mut TcpStream, status: u16, error: &str) -> Result<()> {
    let reason = match status {
        400 => "Bad Request",
        403 => "Forbidden",
        413 => "Payload Too Large",
        429 => "Too Many Requests",
        502 => "Bad Gateway",
        _ => "Error",
    };
    let body = format!("{{\"error\":\"{error}\"}}");
    let resp = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(resp.as_bytes())
        .await
        .context("write error response")?;
    let _ = stream.shutdown().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_consume_route_is_admitted() {
        // The guard is a literal prefix match on the request line — these are
        // the shapes a scanner or a curious peer actually sends.
        assert!("POST /api/pair/consume HTTP/1.1".starts_with(ALLOWED_REQUEST_LINE));
        for denied in [
            "GET /api/chats HTTP/1.1",
            "POST /api/pair/consumex HTTP/1.1",
            "POST /api/pair/mint HTTP/1.1",
            "GET / HTTP/1.1",
            "POST /api/devices/enroll-peer HTTP/1.1",
        ] {
            assert!(
                !denied.starts_with(ALLOWED_REQUEST_LINE),
                "must not admit {denied}"
            );
        }
    }

    #[test]
    fn content_length_is_case_insensitive_and_absent_means_zero() {
        let head = "POST /api/pair/consume HTTP/1.1\r\nHost: x\r\ncontent-length: 42";
        assert_eq!(content_length(head).unwrap(), 42);
        let none = "POST /api/pair/consume HTTP/1.1\r\nHost: x";
        assert_eq!(content_length(none).unwrap(), 0);
    }

    #[test]
    fn head_end_is_found_across_chunk_boundaries() {
        assert_eq!(find_head_end(b"AB\r\n\r\nbody"), Some(2));
        assert_eq!(find_head_end(b"no terminator"), None);
    }

    // ── Socket-level: the refusal rules, proven against a real listener with a
    // stub forwarder that RECORDS whether the box was ever reached. ──────────

    struct SpyForwarder {
        reached: Arc<AtomicU32>,
    }

    impl Forwarder for SpyForwarder {
        fn forward(
            &self,
            _raw: Vec<u8>,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>>> + Send>> {
            self.reached.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                Ok(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}".to_vec())
            })
        }
    }

    /// Open a door on loopback, run `body`, return (response, times the box was reached).
    async fn with_door<F, Fut>(requests: Vec<&'static str>, body: F) -> (Vec<String>, u32)
    where
        F: FnOnce(std::net::SocketAddr, Vec<&'static str>) -> Fut,
        Fut: std::future::Future<Output = Vec<String>>,
    {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let reached = Arc::new(AtomicU32::new(0));
        let fwd: Arc<dyn Forwarder> = Arc::new(SpyForwarder { reached: reached.clone() });
        let attempts = Arc::new(AtomicU32::new(0));
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let task = tokio::spawn(serve_pair_door(listener, fwd, attempts, async move {
            let _ = rx.await;
        }));
        let out = body(addr, requests).await;
        drop(tx);
        let _ = tokio::time::timeout(Duration::from_secs(1), task).await;
        (out, reached.load(Ordering::SeqCst))
    }

    async fn send(addr: std::net::SocketAddr, raw: &str) -> String {
        let mut s = TcpStream::connect(addr).await.unwrap();
        s.write_all(raw.as_bytes()).await.unwrap();
        let mut buf = Vec::new();
        let _ = tokio::time::timeout(Duration::from_secs(2), s.read_to_end(&mut buf)).await;
        String::from_utf8_lossy(&buf).to_string()
    }

    #[tokio::test]
    async fn other_routes_are_refused_without_ever_reaching_the_box() {
        // The door forwards over an iroh stream the box authenticates as THIS
        // machine, so a route escaping the allowlist would hand a LAN peer the
        // owner's authority. The spy proves nothing was forwarded at all.
        let (responses, reached) = with_door(vec![], |addr, _| async move {
            let mut out = Vec::new();
            for raw in [
                "GET /api/chats HTTP/1.1\r\nHost: x\r\n\r\n",
                "POST /api/devices/enroll-peer HTTP/1.1\r\nHost: x\r\nContent-Length: 2\r\n\r\n{}",
                "GET / HTTP/1.1\r\nHost: x\r\n\r\n",
                "POST /api/pair/consumex HTTP/1.1\r\nHost: x\r\n\r\n",
            ] {
                out.push(send(addr, raw).await);
            }
            out
        })
        .await;
        assert_eq!(reached, 0, "no refused route may reach the box");
        for r in &responses {
            assert!(r.starts_with("HTTP/1.1 403"), "expected 403, got: {r}");
        }
    }

    #[tokio::test]
    async fn the_consume_route_is_forwarded() {
        let (responses, reached) = with_door(vec![], |addr, _| async move {
            vec![
                send(
                    addr,
                    "POST /api/pair/consume HTTP/1.1\r\nHost: x\r\nContent-Length: 2\r\n\r\n{}",
                )
                .await,
            ]
        })
        .await;
        assert_eq!(reached, 1);
        assert!(responses[0].starts_with("HTTP/1.1 200"), "{}", responses[0]);
    }

    #[tokio::test]
    async fn the_attempt_budget_is_the_only_limit_in_the_path() {
        // Requests arriving over iroh carry no ConnectInfo, so the box's own
        // per-IP pair limiter never runs on anything forwarded here. If this
        // budget stops working, a 10^6 code space is open at full speed.
        let (responses, reached) = with_door(vec![], |addr, _| async move {
            let mut out = Vec::new();
            for _ in 0..(MAX_ATTEMPTS + 3) {
                out.push(
                    send(
                        addr,
                        "POST /api/pair/consume HTTP/1.1\r\nHost: x\r\nContent-Length: 2\r\n\r\n{}",
                    )
                    .await,
                );
            }
            out
        })
        .await;
        assert_eq!(reached, MAX_ATTEMPTS, "budget must cap what reaches the box");
        let refused = responses
            .iter()
            .filter(|r| r.starts_with("HTTP/1.1 429"))
            .count();
        assert_eq!(refused, 3, "attempts past the budget must be refused");
    }

    #[tokio::test]
    async fn client_headers_are_not_forwarded() {
        // The box's limiter was taught not to trust X-Forwarded-For after a LAN
        // attacker used it to mint a fresh budget per request. Nothing the peer
        // sends may survive into the request the box sees.
        struct CapturingForwarder(std::sync::Mutex<Vec<u8>>, Arc<AtomicU32>);
        impl Forwarder for CapturingForwarder {
            fn forward(
                &self,
                raw: Vec<u8>,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>>> + Send>>
            {
                *self.0.lock().unwrap() = raw;
                self.1.fetch_add(1, Ordering::SeqCst);
                Box::pin(async move { Ok(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".to_vec()) })
            }
        }

        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let captured = Arc::new(CapturingForwarder(
            std::sync::Mutex::new(Vec::new()),
            Arc::new(AtomicU32::new(0)),
        ));
        let fwd: Arc<dyn Forwarder> = captured.clone();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let task = tokio::spawn(serve_pair_door(
            listener,
            fwd,
            Arc::new(AtomicU32::new(0)),
            async move {
                let _ = rx.await;
            },
        ));

        send(
            addr,
            "POST /api/pair/consume HTTP/1.1\r\nHost: evil\r\nX-Forwarded-For: 1.2.3.4\r\nCookie: a=b\r\nContent-Length: 9\r\n\r\n{\"t\":\"1\"}",
        )
        .await;
        drop(tx);
        let _ = tokio::time::timeout(Duration::from_secs(1), task).await;

        let sent = String::from_utf8_lossy(&captured.0.lock().unwrap().clone()).to_string();
        assert!(!sent.contains("X-Forwarded-For"), "smuggled header survived: {sent}");
        assert!(!sent.contains("Cookie"), "smuggled cookie survived: {sent}");
        assert!(!sent.contains("evil"), "peer Host survived: {sent}");
        // The body is the one thing that must pass through intact.
        assert!(sent.ends_with("{\"t\":\"1\"}"), "body not forwarded: {sent}");
    }
}

//! Box-side client for the blind relay.
//!
//! The box **dials out** to the relay (so it needs no public inbound port and
//! works behind CGNAT), `Register`s its SNI, and holds a control connection. On
//! each `OpenConn` it dials a fresh **work connection** to the relay and splices
//! it to the box's own local TLS service — so the box terminates TLS with its own
//! cert and the relay only ever moves ciphertext.
//!
//! Reconnects with **Full-Jitter** backoff (and an initial splay) so a fleet
//! severed by a relay restart doesn't thunder back in lockstep.

mod wire;

use anyhow::{anyhow, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::net::TcpStream;
use virtues_protocol::relay::{BoxHello, BoxMsg, RelayMsg};

use crate::wire::{read_msg, write_msg};

/// Full-Jitter backoff bounds (AWS "Exponential Backoff And Jitter").
const BACKOFF_BASE: Duration = Duration::from_secs(1);
const BACKOFF_CAP: Duration = Duration::from_secs(60);
/// Initial random splay before the *first* connect, so boxes that boot/restart
/// together don't all dial at the same instant.
const STARTUP_SPLAY: Duration = Duration::from_secs(30);
/// If the relay sends nothing (no `Ping`, no `OpenConn`) within this window, we
/// declare the control link dead and reconnect. The relay pings every 25s, so
/// 60s tolerates jitter while still detecting a silently-dead relay (kernel
/// panic / blackhole / NAT idle-drop) that emits no TCP FIN — otherwise a bare
/// `read` would block forever and the reconnect loop would never run.
const READ_TIMEOUT: Duration = Duration::from_secs(60);
/// TCP keepalive backstop (heartbeat is primary). Probe after 30s idle, then
/// every 10s; `TCP_USER_TIMEOUT` kills a connection gone dark mid-stream.
const KEEPALIVE_IDLE: Duration = Duration::from_secs(30);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(10);
const USER_TIMEOUT: Duration = Duration::from_secs(90);

/// Apply TCP keepalive + user-timeout to a connected stream. Best-effort: a
/// failure here is logged, not fatal (the app-level read timeout still applies).
fn set_keepalive(stream: &TcpStream) {
    let sock = socket2::SockRef::from(stream);
    let ka = socket2::TcpKeepalive::new()
        .with_time(KEEPALIVE_IDLE)
        .with_interval(KEEPALIVE_INTERVAL);
    if let Err(e) = sock.set_tcp_keepalive(&ka) {
        tracing::debug!(error = %e, "set_tcp_keepalive failed");
    }
    #[cfg(target_os = "linux")]
    if let Err(e) = sock.set_tcp_user_timeout(Some(USER_TIMEOUT)) {
        tracing::debug!(error = %e, "set_tcp_user_timeout failed");
    }
    #[cfg(not(target_os = "linux"))]
    let _ = USER_TIMEOUT;
}

#[derive(Clone, Debug)]
pub struct RelayClientConfig {
    /// Relay control address the box dials out to (host:port).
    pub relay_addr: String,
    /// This box's SNI, e.g. `abc123.boxes.virtues.com`.
    pub sni: String,
    /// Token presented at `Register` when [`Self::token_cell`] is unset (tests,
    /// dev/env path). The atlas-provisioned box uses `token_cell` instead so it
    /// can present a freshly-rotated (current-bucket) token on each reconnect.
    pub token: String,
    /// Live token cell: read at each (re)connect so token rotation (revocation
    /// bucketing) takes effect without restarting the client. `None` → use
    /// [`Self::token`].
    pub token_cell: Option<Arc<RwLock<String>>>,
    /// The box's own local TLS service to forward work connections to,
    /// e.g. `127.0.0.1:8443`. The box terminates TLS here with its own cert.
    pub local_addr: String,
    /// Control-loop read timeout. `None` uses the production [`READ_TIMEOUT`];
    /// tests set a short value to exercise the silent-relay reconnect path
    /// deterministically.
    pub read_timeout: Option<Duration>,
    /// Live-registration signal: set `true` once `Register` is acked and back to
    /// `false` whenever the control connection ends. Lets the box advertise its
    /// relay `box_url` only while it's actually reachable (review #10). `None`
    /// disables the signal.
    pub registered: Option<Arc<AtomicBool>>,
}

/// Run the relay client forever: connect, serve, reconnect with jittered backoff.
/// Intended to be `tokio::spawn`ed alongside the box's HTTP/TLS server.
pub async fn run(cfg: RelayClientConfig) {
    // Initial splay so a fleet booting/restarting together doesn't dial in lockstep.
    tokio::time::sleep(splay()).await;

    let mut attempt: u32 = 0;
    loop {
        match serve_once(&cfg).await {
            Ok(()) => {
                tracing::info!(sni = %cfg.sni, "relay control connection closed");
                attempt = 0;
            }
            Err(e) => tracing::warn!(sni = %cfg.sni, error = %e, "relay control connection failed"),
        }
        let delay = backoff_delay(attempt);
        attempt = attempt.saturating_add(1);
        tracing::debug!(sni = %cfg.sni, ?delay, attempt, "reconnecting to relay");
        tokio::time::sleep(delay).await;
    }
}

/// Clears the live-registration flag when dropped — i.e. when `serve_once`
/// returns by any path — so a dead control link is never reported as registered.
struct RegisteredGuard(Option<Arc<AtomicBool>>);
impl Drop for RegisteredGuard {
    fn drop(&mut self) {
        if let Some(flag) = &self.0 {
            flag.store(false, Ordering::Relaxed);
        }
    }
}

/// One control-connection lifecycle: connect, register, then serve `OpenConn` /
/// `Ping` until the connection closes or errors. No splay or backoff (the caller
/// owns reconnection) — also the entry point integration tests drive directly.
pub async fn serve_once(cfg: &RelayClientConfig) -> Result<()> {
    let stream = TcpStream::connect(&cfg.relay_addr).await?;
    set_keepalive(&stream);
    let (mut rd, mut wr) = stream.into_split();

    // Clears the live-registration flag on *any* exit from this function (error,
    // silent-relay timeout, clean close) so the box stops advertising box_url the
    // moment the control link drops.
    let _reg = RegisteredGuard(cfg.registered.clone());

    // Resolve the token fresh at connect time: the cell (if present) holds the
    // current-bucket token the refresh task keeps up to date, so a reconnect
    // after rotation presents the new token without restarting the client.
    let token = match &cfg.token_cell {
        Some(cell) => cell
            .read()
            .map(|g| g.clone())
            .unwrap_or_else(|_| cfg.token.clone()),
        None => cfg.token.clone(),
    };
    write_msg(
        &mut wr,
        &BoxHello::Register {
            sni: cfg.sni.clone(),
            token,
        },
    )
    .await?;

    match read_msg::<_, RelayMsg>(&mut rd).await? {
        RelayMsg::Registered => {
            tracing::info!(sni = %cfg.sni, "registered with relay");
            if let Some(flag) = &cfg.registered {
                flag.store(true, Ordering::Relaxed);
            }
        }
        RelayMsg::Rejected { reason } => return Err(anyhow!("relay rejected register: {reason}")),
        other => return Err(anyhow!("unexpected pre-register message: {other:?}")),
    }

    // Control loop: respond to OpenConn (dial a work conn) and Ping (Pong).
    // The read is bounded by `read_timeout` so a silently-dead relay (no FIN,
    // pings stopped) surfaces as an error here and the caller reconnects.
    let read_timeout = cfg.read_timeout.unwrap_or(READ_TIMEOUT);
    loop {
        let msg = match tokio::time::timeout(read_timeout, read_msg::<_, RelayMsg>(&mut rd)).await {
            Ok(r) => r?,
            Err(_) => {
                return Err(anyhow!(
                    "relay went silent (no message within {read_timeout:?})"
                ))
            }
        };
        match msg {
            RelayMsg::OpenConn { conn_id } => {
                let cfg = cfg.clone();
                tokio::spawn(async move {
                    if let Err(e) = serve_work(&cfg, conn_id).await {
                        tracing::debug!(error = %e, "work connection ended");
                    }
                });
            }
            RelayMsg::Ping => write_msg(&mut wr, &BoxMsg::Pong).await?,
            RelayMsg::Registered => {} // tolerate duplicates
            RelayMsg::Rejected { reason } => {
                return Err(anyhow!("relay rejected mid-session: {reason}"))
            }
        }
    }
}

/// Dial a work connection for `conn_id` and splice it to the box's local service.
async fn serve_work(cfg: &RelayClientConfig, conn_id: String) -> Result<()> {
    let mut work = TcpStream::connect(&cfg.relay_addr).await?;
    set_keepalive(&work);
    write_msg(&mut work, &BoxHello::Work { conn_id }).await?;

    // The relay will replay the client's ClientHello and then stream ciphertext;
    // our local service terminates TLS with the box's own cert. Idle-reaped so a
    // half-open client (vanished with no FIN) can't pin this task + two sockets.
    let local = TcpStream::connect(&cfg.local_addr).await?;
    let _ = splice(work, local, SPLICE_IDLE).await;
    Ok(())
}

/// Idle timeout for a spliced connection (see [`splice`]). Resets on any byte
/// movement; the app-level heartbeat keeps healthy streams well within it.
pub const SPLICE_IDLE: Duration = Duration::from_secs(600);

/// Bidirectional copy with an **idle** timeout, returning `(a→b, b→a)` byte
/// totals no matter how it ends (clean EOF, error, or idle). The idle timer
/// resets whenever either direction moves bytes, so a long-lived stream that
/// carries periodic heartbeats is never cut — but a half-open connection (peer
/// gone with no FIN, e.g. a NAT/middlebox blackhole) is reaped after `idle`
/// instead of pinning a task and two sockets forever.
pub async fn splice<A, B>(a: A, b: B, idle: Duration) -> (u64, u64)
where
    A: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    B: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    let (ar, aw) = tokio::io::split(a);
    let (br, bw) = tokio::io::split(b);
    let a2b = Arc::new(AtomicU64::new(0));
    let b2a = Arc::new(AtomicU64::new(0));

    let mut f1 = Box::pin(pump(ar, bw, a2b.clone()));
    let mut f2 = Box::pin(pump(br, aw, b2a.clone()));
    let (mut done1, mut done2) = (false, false);
    let mut last = (0u64, 0u64);

    while !(done1 && done2) {
        tokio::select! {
            _ = &mut f1, if !done1 => done1 = true,
            _ = &mut f2, if !done2 => done2 = true,
            // No select event for a full `idle` window means neither half closed;
            // if the byte counters also didn't move, the connection is dead — reap.
            _ = tokio::time::sleep(idle) => {
                let now = (a2b.load(Ordering::Relaxed), b2a.load(Ordering::Relaxed));
                if now == last {
                    break;
                }
                last = now;
            }
        }
    }
    (a2b.load(Ordering::Relaxed), b2a.load(Ordering::Relaxed))
}

/// One-direction copy, tallying bytes into `counter` as they move so a caller can
/// read the partial total even if a later read/write errors. Propagates the
/// half-close: a clean EOF shuts the writer down.
async fn pump<R, W>(mut r: R, mut w: W, counter: std::sync::Arc<std::sync::atomic::AtomicU64>)
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut buf = vec![0u8; 16 * 1024];
    loop {
        match r.read(&mut buf).await {
            Ok(0) | Err(_) => {
                let _ = w.shutdown().await;
                return;
            }
            Ok(n) => {
                if w.write_all(&buf[..n]).await.is_err() {
                    return;
                }
                counter.fetch_add(n as u64, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }
}

/// Full-Jitter delay: `random(0, min(cap, base * 2^attempt))`.
fn backoff_delay(attempt: u32) -> Duration {
    use rand::Rng;
    let exp_ms = BACKOFF_BASE
        .as_millis()
        .saturating_mul(1u128 << attempt.min(20))
        .min(BACKOFF_CAP.as_millis()) as u64;
    let jittered = rand::thread_rng().gen_range(0..=exp_ms);
    Duration::from_millis(jittered)
}

fn splay() -> Duration {
    use rand::Rng;
    let ms = rand::thread_rng().gen_range(0..=STARTUP_SPLAY.as_millis() as u64);
    Duration::from_millis(ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_is_bounded_by_cap() {
        for attempt in 0..40 {
            let d = backoff_delay(attempt);
            assert!(d <= BACKOFF_CAP, "attempt {attempt} exceeded cap: {d:?}");
        }
    }

    #[test]
    fn backoff_grows_then_caps() {
        // The *upper bound* grows with attempt until the cap. We can't assert on
        // a single random draw, so check the cap math by sampling the max many
        // times at a low attempt vs a high attempt.
        let low_max = (0..1000).map(|_| backoff_delay(0)).max().unwrap();
        let high_max = (0..1000).map(|_| backoff_delay(10)).max().unwrap();
        assert!(low_max <= Duration::from_secs(1));
        assert!(high_max > Duration::from_secs(1));
        assert!(high_max <= BACKOFF_CAP);
    }

    #[test]
    fn splay_is_bounded() {
        for _ in 0..1000 {
            assert!(splay() <= STARTUP_SPLAY);
        }
    }
}

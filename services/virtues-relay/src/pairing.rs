//! Pairing an inbound client with its box: ask the box (over control) to dial a
//! work connection, wait for it, **replay the buffered ClientHello**, then splice
//! ciphertext both ways. The relay never decrypts.

use anyhow::Result;
use tokio::net::TcpStream;
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::config::{
    MAX_INFLIGHT_PER_SNI, RATE_BURST_PER_SNI, RATE_REFILL_PER_SEC, SPLICE_IDLE, WORK_DEADLINE,
};
use crate::state::AppState;

/// Route a peeked client to its box. `buffered` is the exact ClientHello bytes
/// consumed during the SNI peek — they MUST be replayed to the box first or the
/// TLS handshake hangs.
pub async fn route_client(
    client: TcpStream,
    sni: String,
    buffered: Vec<u8>,
    state: AppState,
) -> Result<()> {
    let Some(handle) = state.registry.lookup(&sni) else {
        tracing::debug!(%sni, registered = state.registry.len(), "no box for SNI; closing");
        return Ok(());
    };

    // Per-SNI abuse floor (checked only after the SNI resolves to a registered
    // box, so the limiter's keyset stays fleet-bounded). Rate first (cheap), then
    // reserve a concurrent-connection slot held for the life of the splice. Both
    // are keyed on SNI, not source IP — CGNAT-safe. On trip we just close the
    // client; the box is never asked to dial a work connection.
    if !state
        .limits
        .allow_rate(&sni, RATE_BURST_PER_SNI, RATE_REFILL_PER_SEC)
    {
        tracing::debug!(%sni, "per-SNI connect rate exceeded; closing");
        return Ok(());
    }
    let _slot = match state.limits.try_acquire(&sni, MAX_INFLIGHT_PER_SNI) {
        Some(g) => g,
        None => {
            tracing::debug!(%sni, cap = MAX_INFLIGHT_PER_SNI, "per-SNI connection cap reached; closing");
            return Ok(());
        }
    };

    let conn_id = Uuid::new_v4();
    let (tx, rx) = oneshot::channel::<TcpStream>();
    state.pending.insert(conn_id, tx);

    // Ask the box to dial a work connection for this client.
    if handle.work_tx.send(conn_id).await.is_err() {
        state.pending.remove(&conn_id);
        tracing::debug!(%sni, "box control gone before OpenConn; closing");
        return Ok(());
    }

    // Wait for the box's work connection (handed over by the control handler).
    let mut work = match tokio::time::timeout(WORK_DEADLINE, rx).await {
        Ok(Ok(w)) => w,
        _ => {
            state.pending.remove(&conn_id);
            tracing::debug!(%sni, %conn_id, "box did not dial work conn in time; closing");
            return Ok(());
        }
    };

    // Replay the ClientHello, then splice ciphertext both directions.
    use tokio::io::AsyncWriteExt;
    work.write_all(&buffered).await?;
    work.flush().await?;
    // `buffered` was the client's first flight; count it in the client→box tally.
    let replayed = buffered.len() as u64;
    // `splice` returns the byte counts no matter how the connection ends, so an
    // abnormal close (mid-stream RST) is still metered — a plain
    // `copy_bidirectional` drops its counts on error, under-billing every
    // abnormally-closed connection. It is also idle-reaped (SPLICE_IDLE) so a
    // half-open client can't pin a relay task + two sockets forever.
    let (c2b, b2c) = splice(client, work, SPLICE_IDLE).await;
    state.meter.add(&sni, replayed + c2b + b2c);
    Ok(())
}

/// Bidirectional copy with an **idle** timeout, returning `(a→b, b→a)` byte
/// totals no matter how it ends (clean EOF, error, or idle). The idle timer
/// resets on any byte movement, so a heartbeat-carrying stream lives
/// indefinitely while a half-open (no-FIN) connection is reaped after `idle`.
///
/// Kept local to the relay so its data plane carries no extra crate dependency
/// (mirrors the box-side `virtues_relay_client::splice`).
async fn splice<A, B>(a: A, b: B, idle: std::time::Duration) -> (u64, u64)
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

/// One-direction copy, tallying bytes into `counter` as they move so the caller
/// reads an accurate partial total even if a later read/write errors. Propagates
/// the half-close: a clean EOF shuts the writer down.
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

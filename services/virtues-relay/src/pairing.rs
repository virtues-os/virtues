//! Pairing an inbound client with its box: ask the box (over control) to dial a
//! work connection, wait for it, **replay the buffered ClientHello**, then splice
//! ciphertext both ways. The relay never decrypts.

use anyhow::Result;
use tokio::net::TcpStream;
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::config::WORK_DEADLINE;
use crate::state::AppState;

/// Route a peeked client to its box. `buffered` is the exact ClientHello bytes
/// consumed during the SNI peek — they MUST be replayed to the box first or the
/// TLS handshake hangs.
pub async fn route_client(
    mut client: TcpStream,
    sni: String,
    buffered: Vec<u8>,
    state: AppState,
) -> Result<()> {
    let Some(handle) = state.registry.lookup(&sni) else {
        tracing::debug!(%sni, registered = state.registry.len(), "no box for SNI; closing");
        return Ok(());
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
    // copy_bidirectional reports bytes each way; meter the aggregate per box
    // (volume only — no per-connection record). `buffered` was the client's
    // first flight, so include it in the client→box tally.
    let replayed = buffered.len() as u64;
    if let Ok((c2b, b2c)) = tokio::io::copy_bidirectional(&mut client, &mut work).await {
        state.meter.add(&sni, replayed + c2b + b2c);
    } else {
        state.meter.add(&sni, replayed);
    }
    Ok(())
}

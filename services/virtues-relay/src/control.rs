//! Box-facing connection handling: control connections (persistent, registered
//! per SNI, keepalive + `OpenConn` signaling) and work connections (ephemeral,
//! one per inbound client, handed to the waiting client task to splice).

use anyhow::Result;
use subtle::ConstantTimeEq;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::interval;
use uuid::Uuid;
use virtues_protocol::relay::{BoxHello, BoxMsg, RelayMsg};

use crate::config::{HELLO_TIMEOUT, KEEPALIVE, MAX_CONN_AGE, PONG_DEADLINE};
use crate::state::AppState;
use crate::wire::{read_msg, write_msg};

/// Dispatch a freshly-accepted box connection by its hello line.
pub async fn handle_box_conn(mut stream: TcpStream, state: AppState) -> Result<()> {
    // Read the hello byte-by-byte so a *work* connection's post-hello bytes stay
    // intact for splicing — bounded by HELLO_TIMEOUT so a connect-then-stall peer
    // can't pin a task + socket waiting for a hello that never completes.
    let hello: BoxHello = match tokio::time::timeout(HELLO_TIMEOUT, read_msg(&mut stream)).await {
        Ok(r) => r?,
        Err(_) => {
            tracing::debug!("box hello timed out before completing; dropping");
            return Ok(());
        }
    };
    match hello {
        BoxHello::Register { sni, token } => handle_control(stream, sni, token, state).await,
        BoxHello::Work { conn_id } => {
            let id = Uuid::parse_str(&conn_id)?;
            // Hand the raw stream to the waiting client task. If the pending entry
            // is gone (expired / unknown), just drop the connection.
            if let Some((_, tx)) = state.pending.remove(&id) {
                let _ = tx.send(stream);
            } else {
                tracing::debug!(%id, "work conn for unknown/expired conn_id; dropping");
            }
            Ok(())
        }
    }
}

/// Run a registered control connection: confirm registration, then concurrently
/// forward `OpenConn` requests + keepalive (writer) and watch for liveness
/// (reader). Evicts the registry entry on teardown (generation-guarded).
async fn handle_control(
    mut stream: TcpStream,
    sni: String,
    token: String,
    state: AppState,
) -> Result<()> {
    // Authorize the registration. With a per-SNI secret, accept the token for the
    // current OR previous bucket — the ±1 window absorbs clock skew and the day
    // boundary, and accepting only these two buckets is what expires a revoked
    // box's token (atlas stops re-minting → it falls out of the window). Derived
    // on the fly from the one secret, so the relay holds no per-box state. Both
    // candidates are compared in constant time (no short-circuit).
    let authorized = match &state.config.secret {
        Some(secret) => {
            let now = virtues_protocol::relay::current_bucket();
            let cur = virtues_protocol::relay::derive_token(secret, &sni, now);
            let prev = virtues_protocol::relay::derive_token(secret, &sni, now.saturating_sub(1));
            bool::from(
                token.as_bytes().ct_eq(cur.as_bytes()) | token.as_bytes().ct_eq(prev.as_bytes()),
            )
        }
        None => bool::from(token.as_bytes().ct_eq(state.config.token.as_bytes())),
    };
    if !authorized {
        write_msg(&mut stream, &RelayMsg::Rejected { reason: "bad token".into() }).await?;
        tracing::warn!(%sni, "register rejected: bad token");
        return Ok(());
    }

    let (work_tx, mut work_rx) = mpsc::channel::<Uuid>(64);
    let gen = state.registry.register(sni.clone(), work_tx);
    write_msg(&mut stream, &RelayMsg::Registered).await?;
    tracing::info!(%sni, registered = state.registry.len(), "box registered");

    let (mut rd, mut wr) = stream.into_split();

    // Writer: forward OpenConn requests + periodic keepalive Ping.
    let writer = tokio::spawn(async move {
        let mut ping = interval(KEEPALIVE);
        ping.tick().await; // consume the immediate first tick
        loop {
            tokio::select! {
                maybe = work_rx.recv() => match maybe {
                    Some(conn_id) => {
                        let msg = RelayMsg::OpenConn { conn_id: conn_id.to_string() };
                        if write_msg(&mut wr, &msg).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                },
                _ = ping.tick() => {
                    if write_msg(&mut wr, &RelayMsg::Ping).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    // Reader: a box must produce traffic (Pong) within PONG_DEADLINE or it's dead.
    // The whole session is also capped at MAX_CONN_AGE: on expiry we tear down so
    // the box re-registers, which re-checks its token against the current bucket —
    // this is what makes token-revocation actually bite on long-lived connections.
    let _ = tokio::time::timeout(MAX_CONN_AGE, async {
        loop {
            match tokio::time::timeout(PONG_DEADLINE, read_msg::<_, BoxMsg>(&mut rd)).await {
                Ok(Ok(BoxMsg::Pong)) => continue,
                Ok(Err(e)) => {
                    tracing::debug!(%sni, error = %e, "control reader ended");
                    break;
                }
                Err(_) => {
                    tracing::debug!(%sni, "control liveness timeout; evicting");
                    break;
                }
            }
        }
    })
    .await;

    writer.abort();
    state.registry.unregister_if(&sni, gen);
    tracing::info!(%sni, "box unregistered");
    Ok(())
}

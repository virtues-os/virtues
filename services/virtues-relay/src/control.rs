//! Box-facing connection handling: control connections (persistent, registered
//! per SNI, keepalive + `OpenConn` signaling) and work connections (ephemeral,
//! one per inbound client, handed to the waiting client task to splice).

use anyhow::Result;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::interval;
use uuid::Uuid;
use virtues_protocol::relay::{BoxHello, BoxMsg, RelayMsg};

use crate::config::{KEEPALIVE, PONG_DEADLINE};
use crate::state::AppState;
use crate::wire::{read_msg, write_msg};

/// Dispatch a freshly-accepted box connection by its hello line.
pub async fn handle_box_conn(mut stream: TcpStream, state: AppState) -> Result<()> {
    // Read the hello byte-by-byte so a *work* connection's post-hello bytes stay
    // intact for splicing.
    let hello: BoxHello = read_msg(&mut stream).await?;
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
    if token != state.config.token {
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

    writer.abort();
    state.registry.unregister_if(&sni, gen);
    tracing::info!(%sni, "box unregistered");
    Ok(())
}

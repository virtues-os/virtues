//! virtues-relay — blind L4 SNI-passthrough relay (library).
//!
//! Boxes dial OUT to this relay (no public inbound port; CGNAT-safe). Browsers
//! reach a box at `<boxhash>.boxes.virtues.com`; the relay peeks the cleartext
//! **SNI**, finds the registered box, asks it (over its control connection) to
//! dial a work connection, then splices the **still-encrypted** bytes. It never
//! terminates TLS (the box holds its own cert), so it only ever sees ciphertext
//! — there is deliberately no rustls/openssl dependency.
//!
//! The `main.rs` binary is a thin wrapper around [`serve`]; the logic lives here
//! so it can be integration-tested end-to-end.

pub mod config;
pub mod control;
pub mod pairing;
pub mod registry;
pub mod sni;
pub mod state;
pub mod wire;

use anyhow::{Context, Result};
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};

use crate::state::AppState;

/// Run the relay: a box-facing accept loop (control + work connections) and a
/// browser-facing accept loop. Never returns under normal operation.
pub async fn serve(
    state: AppState,
    client_listener: TcpListener,
    control_listener: TcpListener,
) -> Result<()> {
    // Box-facing accept loop.
    {
        let state = state.clone();
        tokio::spawn(async move {
            loop {
                match control_listener.accept().await {
                    Ok((stream, peer)) => {
                        let state = state.clone();
                        tokio::spawn(async move {
                            if let Err(e) = control::handle_box_conn(stream, state).await {
                                tracing::debug!(%peer, error = %e, "box connection ended");
                            }
                        });
                    }
                    Err(e) => tracing::warn!(error = %e, "control accept failed"),
                }
            }
        });
    }

    // Browser-facing accept loop.
    loop {
        let (stream, peer) = client_listener
            .accept()
            .await
            .context("client accept failed")?;
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_client(stream, peer, state).await {
                tracing::debug!(%peer, error = %e, "client connection ended");
            }
        });
    }
}

/// Peek the SNI, then route the (still-encrypted) client to its box.
pub async fn handle_client(mut stream: TcpStream, peer: SocketAddr, state: AppState) -> Result<()> {
    let (sni, buffered) = sni::peek_sni(&mut stream).await.context("peek sni")?;
    tracing::debug!(%peer, %sni, "peeked SNI");
    pairing::route_client(stream, sni, buffered, state).await
}

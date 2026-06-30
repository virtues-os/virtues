//! e2e box harness: the box's relay subsystem, minus Postgres/atlas/ACME.
//!
//! Terminates TLS on a self-signed box-held cert, dials out to the relay, and
//! splices inbound work connections to a trivial HTTP responder. This is the
//! exact reach + TLS + liveness path a real box runs; it's deliberately tiny so
//! the harness needs no database. (Real ACME issuance is validated separately —
//! see the README — because it needs virtues-core + a Pebble-CA trust seam.)
//!
//! Env:
//! - `VIRTUES_RELAY_ADDR`  (req) — relay control addr to dial out to.
//! - `VIRTUES_RELAY_SNI`   (req) — this box's name (the cert/SNI).
//! - `VIRTUES_RELAY_TOKEN` (req in secret mode) — registration token.
//! - `VIRTUES_RELAY_TLS_FRONT` (opt, default `0.0.0.0:8443`) — TLS-front bind.

use anyhow::{Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use virtues_helpers::transport::tls;
use virtues_relay_client::{run, splice, RelayClientConfig, SPLICE_IDLE};

const UPSTREAM: &str = "127.0.0.1:9000";

#[tokio::main]
async fn main() -> Result<()> {
    let relay_addr = env("VIRTUES_RELAY_ADDR")?;
    let sni = env("VIRTUES_RELAY_SNI")?;
    let token = std::env::var("VIRTUES_RELAY_TOKEN").unwrap_or_default();
    let tls_front =
        std::env::var("VIRTUES_RELAY_TLS_FRONT").unwrap_or_else(|_| "0.0.0.0:8443".to_string());

    // 1. Trivial upstream the TLS-front splices decrypted HTTP to.
    spawn_http_responder(UPSTREAM).await?;

    // 2. Box-held cert (self-signed bootstrap — the same path a real box serves
    //    on before its ACME cert lands).
    let (cert, key) = tls::self_signed(vec![sni.clone()])?;
    let server_config = tls::server_config_from_pem(&cert, &key)?;
    eprintln!("[box] self-signed box cert for {sni}");

    // 3. TLS-front: terminate TLS (box-held cert), splice to the upstream.
    let listener = TcpListener::bind(&tls_front)
        .await
        .with_context(|| format!("bind TLS-front {tls_front}"))?;
    let tls_listener = tls::TlsListener::new(listener, server_config);
    eprintln!("[box] TLS-front up on {tls_front}");
    tokio::spawn(async move {
        loop {
            match tls_listener.accept_raw().await {
                Ok((tcp, _peer, cfg)) => {
                    tokio::spawn(async move {
                        let Ok(tls_stream) = tls::TlsListener::handshake(cfg, tcp).await else {
                            return;
                        };
                        if let Ok(http) = TcpStream::connect(UPSTREAM).await {
                            let _ = splice(tls_stream, http, SPLICE_IDLE).await;
                        }
                    });
                }
                Err(e) => eprintln!("[box] accept error: {e}"),
            }
        }
    });

    // 4. Dial out to the relay and serve forever (reconnects internally).
    eprintln!("[box] registering {sni} at relay {relay_addr}");
    run(RelayClientConfig {
        relay_addr,
        sni,
        token,
        token_cell: None,
        local_addr: tls_front,
        read_timeout: None,
        registered: None,
    })
    .await;
    Ok(())
}

fn env(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("{key} must be set"))
}

/// A trivial HTTP/1.1 responder: reads (and discards) the request, returns 200.
/// Stands in for the box's real local HTTP server.
async fn spawn_http_responder(addr: &str) -> Result<()> {
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind upstream {addr}"))?;
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                continue;
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                let _ = sock.read(&mut buf).await; // drain the request line(s)
                let body = "ok: reached the box through the blind relay\n";
                let resp = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = sock.write_all(resp.as_bytes()).await;
                let _ = sock.flush().await;
                let _ = sock.shutdown().await;
            });
        }
    });
    Ok(())
}

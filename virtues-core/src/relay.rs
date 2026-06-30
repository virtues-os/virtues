//! Box-side relay integration.
//!
//! Makes the box reachable from any browser via the blind relay without exposing
//! any public inbound port:
//!
//! 1. A **TLS-front** terminates TLS with the box's own cert and splices the
//!    decrypted HTTP to the existing plain-HTTP server on localhost. (The relay
//!    only ever sees ciphertext because TLS terminates *here*, on the box.)
//! 2. The **relay-client** dials out to the relay, registers the box's SNI, and
//!    forwards each inbound client to the TLS-front.
//!
//! Enabled only when `VIRTUES_RELAY_ADDR` is set. The bootstrap cert is
//! self-signed; ACME/DNS-01 (a browser-trusted per-box cert) replaces it next.
//!
//! Env:
//! - `VIRTUES_RELAY_ADDR`  — relay control address to dial out to (host:port).
//! - `VIRTUES_RELAY_SNI`   — this box's name, e.g. `abc.boxes.virtues.com`.
//! - `VIRTUES_RELAY_TOKEN` — shared bearer for `Register` (v1 auth).
//! - `VIRTUES_RELAY_TLS_FRONT` — local TLS-front bind addr (default `127.0.0.1:8443`).

use anyhow::{Context, Result};
use tokio::net::{TcpListener, TcpStream};
use virtues_helpers::transport::tls;

/// Spawn the relay subsystem if configured. No-op (with a debug log) otherwise.
pub fn maybe_spawn(http_port: u16) {
    let relay_addr = match std::env::var("VIRTUES_RELAY_ADDR") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            tracing::debug!("VIRTUES_RELAY_ADDR unset — relay reachability disabled");
            return;
        }
    };
    let sni = std::env::var("VIRTUES_RELAY_SNI").unwrap_or_default();
    if sni.is_empty() {
        tracing::warn!("VIRTUES_RELAY_ADDR set but VIRTUES_RELAY_SNI is empty — relay disabled");
        return;
    }
    let token = std::env::var("VIRTUES_RELAY_TOKEN").unwrap_or_default();
    let tls_front =
        std::env::var("VIRTUES_RELAY_TLS_FRONT").unwrap_or_else(|_| "127.0.0.1:8443".to_string());
    let upstream = format!("127.0.0.1:{http_port}");

    tokio::spawn(async move {
        if let Err(e) = run(relay_addr, sni, token, tls_front, upstream).await {
            tracing::error!(error = %e, "relay subsystem exited");
        }
    });
}

async fn run(
    relay_addr: String,
    sni: String,
    token: String,
    tls_front: String,
    upstream: String,
) -> Result<()> {
    // Prefer a browser-trusted ACME cert (box holds the key); fall back to a
    // self-signed bootstrap when ACME/DNS authority isn't configured yet.
    let (cert_pem, key_pem) = match (crate::acme::AcmeConfig::from_env(), crate::acme::HttpDnsPublisher::from_env()) {
        (Some(acme_cfg), Some(publisher)) => match crate::acme::ensure_cert(&acme_cfg, &publisher).await {
            Ok(m) => {
                tracing::info!(%sni, "using ACME cert (box-held key)");
                (m.cert_pem, m.key_pem)
            }
            Err(e) => {
                tracing::warn!(error = %e, "ACME cert failed; falling back to self-signed bootstrap");
                tls::self_signed(vec![sni.clone()]).context("generate bootstrap cert")?
            }
        },
        _ => {
            tracing::info!(%sni, "ACME not configured; using self-signed bootstrap cert");
            tls::self_signed(vec![sni.clone()]).context("generate bootstrap cert")?
        }
    };
    let server_config =
        tls::server_config_from_pem(&cert_pem, &key_pem).context("build TLS server config")?;

    let listener = TcpListener::bind(&tls_front)
        .await
        .with_context(|| format!("bind TLS-front {tls_front}"))?;
    let tls_listener = tls::TlsListener::new(listener, server_config);
    tracing::info!(%tls_front, %upstream, %sni, "relay TLS-front up; box reachable via relay");

    // TLS-front accept loop: terminate TLS, splice decrypted HTTP to the local
    // plain-HTTP server. Runs for the life of the process.
    tokio::spawn(async move {
        loop {
            match tls_listener.accept().await {
                Ok((mut tls_stream, _peer)) => {
                    let upstream = upstream.clone();
                    tokio::spawn(async move {
                        match TcpStream::connect(&upstream).await {
                            Ok(mut http) => {
                                let _ =
                                    tokio::io::copy_bidirectional(&mut tls_stream, &mut http).await;
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, %upstream, "TLS-front upstream connect failed")
                            }
                        }
                    });
                }
                Err(e) => tracing::debug!(error = %e, "TLS-front accept error"),
            }
        }
    });

    // Dial out to the relay and serve forever (reconnects internally).
    virtues_relay_client::run(virtues_relay_client::RelayClientConfig {
        relay_addr,
        sni,
        token,
        local_addr: tls_front,
    })
    .await;

    Ok(())
}

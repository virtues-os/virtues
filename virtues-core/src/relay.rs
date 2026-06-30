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
//! Enabled when the box has been provisioned with relay config (atlas mints it
//! at link, stored in `box_secrets`; see [`crate::virtues_api::relay`]) — or,
//! for dev/manual setups, when `VIRTUES_RELAY_ADDR` is set in the environment.
//! The bootstrap cert is self-signed; ACME/DNS-01 (a browser-trusted per-box
//! cert) replaces it next.
//!
//! Env fallback (dev/manual; box_secrets takes precedence):
//! - `VIRTUES_RELAY_ADDR`  — relay control address to dial out to (host:port).
//! - `VIRTUES_RELAY_SNI`   — this box's name, e.g. `abc.virtues.ch`.
//! - `VIRTUES_RELAY_TOKEN` — registration token for `Register`.
//! - `VIRTUES_RELAY_TLS_FRONT` — local TLS-front bind addr (default `127.0.0.1:8443`).

use anyhow::{Context, Result};
use sqlx::PgPool;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use virtues_helpers::transport::tls;

/// How often the background task checks whether the ACME cert needs renewing.
const RENEW_CHECK_INTERVAL: Duration = Duration::from_secs(12 * 3600);

/// Process-wide live-registration flag: `true` only while the box currently
/// holds a registered control connection to the relay. Set by the relay-client,
/// read by pairing ([`crate::api`]) to advertise `box_url` only when the box is
/// actually reachable (review #10).
static RELAY_REGISTERED: OnceLock<Arc<AtomicBool>> = OnceLock::new();

fn registered_flag() -> Arc<AtomicBool> {
    RELAY_REGISTERED
        .get_or_init(|| Arc::new(AtomicBool::new(false)))
        .clone()
}

/// Whether the box currently holds a live, registered relay control connection.
pub fn is_relay_registered() -> bool {
    RELAY_REGISTERED
        .get()
        .map(|f| f.load(Ordering::Relaxed))
        .unwrap_or(false)
}

/// Spawn the relay subsystem if configured. Prefers the atlas-provisioned config
/// in `box_secrets`; falls back to env vars (dev/manual). No-op otherwise.
pub fn maybe_spawn(db: PgPool, http_port: u16) {
    tokio::spawn(async move {
        let Some((relay_addr, sni, token)) = load_runtime_config(&db).await else {
            tracing::debug!("relay not provisioned (no box_secrets config, VIRTUES_RELAY_ADDR unset) — reachability disabled");
            return;
        };
        let tls_front = std::env::var("VIRTUES_RELAY_TLS_FRONT")
            .unwrap_or_else(|_| "127.0.0.1:8443".to_string());
        let upstream = format!("127.0.0.1:{http_port}");
        if let Err(e) = run(relay_addr, sni, token, tls_front, upstream).await {
            tracing::error!(error = %e, "relay subsystem exited");
        }
    });
}

/// Resolve the relay runtime config: prefer the atlas-provisioned `box_secrets`
/// entry, else the env-var fallback (dev/manual). `None` when neither is set.
async fn load_runtime_config(db: &PgPool) -> Option<(String, String, String)> {
    if let Ok(Some(rc)) = crate::virtues_api::relay::load(db).await {
        tracing::info!(sni = %rc.sni, "relay config loaded from box_secrets (atlas-provisioned)");
        return Some((rc.relay_addr, rc.sni, rc.token));
    }
    let relay_addr = std::env::var("VIRTUES_RELAY_ADDR")
        .ok()
        .filter(|s| !s.is_empty())?;
    let sni = std::env::var("VIRTUES_RELAY_SNI").unwrap_or_default();
    if sni.is_empty() {
        tracing::warn!("VIRTUES_RELAY_ADDR set but VIRTUES_RELAY_SNI is empty — relay disabled");
        return None;
    }
    let token = std::env::var("VIRTUES_RELAY_TOKEN").unwrap_or_default();
    tracing::info!(%sni, "relay config loaded from environment (dev/manual)");
    Some((relay_addr, sni, token))
}

async fn run(
    relay_addr: String,
    sni: String,
    token: String,
    tls_front: String,
    upstream: String,
) -> Result<()> {
    // Bind the TLS-front *immediately* with a self-signed bootstrap cert so the
    // box is reachable via the relay on cold start without waiting on ACME (which
    // can take 15s+ for DNS propagation). A browser-trusted ACME cert is obtained
    // concurrently below and hot-swapped in when ready.
    let (cert_pem, key_pem) =
        tls::self_signed(vec![sni.clone()]).context("generate bootstrap cert")?;
    let server_config =
        tls::server_config_from_pem(&cert_pem, &key_pem).context("build TLS server config")?;

    let listener = TcpListener::bind(&tls_front)
        .await
        .with_context(|| format!("bind TLS-front {tls_front}"))?;
    let tls_listener = tls::TlsListener::new(listener, server_config);
    let reloader = tls_listener.reloader();
    tracing::info!(%tls_front, %upstream, %sni, "relay TLS-front up (bootstrap cert); box reachable via relay");

    // Concurrently obtain a browser-trusted ACME cert and keep it renewed,
    // hot-swapping it into the live listener — only when ACME + a DNS authority
    // are configured. Otherwise we stay on the self-signed bootstrap.
    match (
        crate::acme::AcmeConfig::from_env(),
        crate::acme::HttpDnsPublisher::from_env(),
    ) {
        (Some(acme_cfg), Some(publisher)) => {
            tokio::spawn(cert_task(acme_cfg, publisher, reloader));
        }
        _ => tracing::info!(%sni, "ACME not configured; staying on self-signed bootstrap cert"),
    }

    // TLS-front accept loop: terminate TLS, splice decrypted HTTP to the local
    // plain-HTTP server. Runs for the life of the process. The handshake runs in
    // the spawned task (via accept_raw + handshake), never inline in the loop, so
    // one slow/stalled client handshake can't block every other visitor.
    tokio::spawn(async move {
        loop {
            match tls_listener.accept_raw().await {
                Ok((tcp, _peer, tls_config)) => {
                    let upstream = upstream.clone();
                    tokio::spawn(async move {
                        let tls_stream = match tls::TlsListener::handshake(tls_config, tcp).await {
                            Ok(s) => s,
                            Err(e) => {
                                tracing::debug!(error = %e, "TLS-front handshake failed");
                                return;
                            }
                        };
                        match TcpStream::connect(&upstream).await {
                            Ok(http) => {
                                // Idle-reaped so a half-open remote client can't pin
                                // a task + an upstream socket on the box forever.
                                let _ = virtues_relay_client::splice(
                                    tls_stream,
                                    http,
                                    virtues_relay_client::SPLICE_IDLE,
                                )
                                .await;
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
        read_timeout: None,
        registered: Some(registered_flag()),
    })
    .await;

    Ok(())
}

/// Obtain the initial ACME cert (hot-swapping it over the bootstrap cert) and
/// then renew it before expiry, hot-swapping each renewal in. Never returns; a
/// failed issuance/renewal is logged and retried on the next tick (the listener
/// keeps serving the last good cert in the meantime).
async fn cert_task(
    cfg: crate::acme::AcmeConfig,
    publisher: crate::acme::HttpDnsPublisher,
    reloader: tls::CertReloader,
) {
    // Initial issuance (load cached-if-fresh, else obtain).
    match crate::acme::ensure_cert(&cfg, &publisher).await {
        Ok(m) => match tls::server_config_from_pem(&m.cert_pem, &m.key_pem) {
            Ok(c) => {
                reloader.reload(c);
                tracing::info!("ACME cert active (box-held key)");
            }
            Err(e) => tracing::warn!(error = %e, "issued cert failed to load; staying on bootstrap"),
        },
        Err(e) => {
            tracing::warn!(error = %e, "initial ACME issuance failed; staying on bootstrap cert")
        }
    }

    // Renewal loop.
    loop {
        tokio::time::sleep(RENEW_CHECK_INTERVAL).await;
        if !crate::acme::cert_stale(&cfg.cert_dir) {
            continue;
        }
        match crate::acme::obtain(&cfg, &publisher).await {
            Ok(m) => {
                if let Err(e) = crate::acme::save_to_disk(&cfg.cert_dir, &m).await {
                    tracing::warn!(error = %e, "failed to cache renewed cert (continuing in-memory)");
                }
                match tls::server_config_from_pem(&m.cert_pem, &m.key_pem) {
                    Ok(c) => {
                        reloader.reload(c);
                        tracing::info!("ACME cert renewed and hot-swapped");
                    }
                    Err(e) => tracing::warn!(error = %e, "renewed cert failed to load"),
                }
            }
            Err(e) => tracing::warn!(error = %e, "ACME renewal failed; will retry"),
        }
    }
}

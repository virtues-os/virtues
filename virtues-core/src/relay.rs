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
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use virtues_helpers::transport::tls;

/// How often the background task checks whether the ACME cert needs renewing.
const RENEW_CHECK_INTERVAL: Duration = Duration::from_secs(12 * 3600);
/// How often the box re-fetches its relay token from atlas. Must be shorter than
/// the token bucket (24h) so the presented token is always within the relay's
/// current-or-previous window; a revoked box stops getting fresh tokens here and
/// falls out of that window within ~2 buckets. See `docs/relay-control-plane.md`.
const TOKEN_REFRESH_INTERVAL: Duration = Duration::from_secs(12 * 3600);
/// Initial-issuance retry backoff. Until the first browser-trusted cert lands the
/// box serves the self-signed bootstrap, which browsers reject — so retry fast
/// (not at the 12h renewal cadence), backing off to a ceiling on repeated failure.
const INITIAL_ISSUE_BACKOFF: Duration = Duration::from_secs(30);
const INITIAL_ISSUE_BACKOFF_MAX: Duration = Duration::from_secs(15 * 60);

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
        let resolved = match load_runtime_config(&db).await {
            Some(c) => Some(c),
            // Lazy-provision: a box linked before provisioning existed (or whose
            // provisioning failed) has an api_key but no stored config. Fetch it.
            None => lazy_provision(&db).await,
        };
        let Some((relay_addr, sni, token)) = resolved else {
            tracing::debug!("relay not provisioned (no box_secrets/env config, not lazily provisionable) — reachability disabled");
            return;
        };
        // Live token cell: the refresh task keeps it on the current bucket so each
        // reconnect presents a fresh token (and a revoked box stops getting one).
        let token_cell = Arc::new(RwLock::new(token));
        spawn_token_refresh(db.clone(), token_cell.clone());

        let tls_front = std::env::var("VIRTUES_RELAY_TLS_FRONT")
            .unwrap_or_else(|_| "127.0.0.1:8443".to_string());
        let upstream = format!("127.0.0.1:{http_port}");
        if let Err(e) = run(relay_addr, sni, token_cell, tls_front, upstream).await {
            tracing::error!(error = %e, "relay subsystem exited");
        }
    });
}

/// Periodically re-fetch this box's relay token from atlas and update the live
/// cell. atlas mints only the current bucket for an active, non-revoked account,
/// so this both rotates the token forward each bucket and is the point at which a
/// revoked/lapsed box stops receiving valid tokens. Best-effort: a failed refresh
/// keeps the current token (still valid for up to ~2 buckets).
fn spawn_token_refresh(db: PgPool, cell: Arc<RwLock<String>>) {
    tokio::spawn(async move {
        loop {
            // Refresh immediately on spawn (a box offline > 1 bucket has a stale
            // stored token), then once per interval.
            if let Ok(Some(api_key)) = crate::virtues_api::renew::read_api_key(&db).await {
                let http = crate::http_client::virtues_api_client();
                let atlas = crate::virtues_api::atlas_url();
                match crate::virtues_api::relay::fetch_and_store(&db, &http, &atlas, &api_key).await {
                    Ok(()) => {
                        if let Ok(Some(rc)) = crate::virtues_api::relay::load(&db).await {
                            if let Ok(mut g) = cell.write() {
                                *g = rc.token;
                            }
                            tracing::debug!("relay token refreshed");
                        }
                    }
                    Err(e) => tracing::debug!(error = %e, "relay token refresh skipped"),
                }
            }
            tokio::time::sleep(TOKEN_REFRESH_INTERVAL).await;
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

/// Fetch + persist relay config from atlas for a linked box that has no stored
/// config yet (linked before provisioning existed, or an earlier provision
/// failed). Retries transient failures with backoff; gives up — leaving the box
/// LAN-only — if the box isn't linked or the deployment has no relay (atlas 503).
async fn lazy_provision(db: &PgPool) -> Option<(String, String, String)> {
    let api_key = match crate::virtues_api::renew::read_api_key(db).await {
        Ok(Some(k)) => k,
        _ => return None, // not linked → nothing to provision against
    };
    let http = crate::http_client::virtues_api_client();
    let atlas = crate::virtues_api::atlas_url();
    let mut backoff = Duration::from_secs(30);
    loop {
        match crate::virtues_api::relay::fetch_and_store(db, &http, &atlas, &api_key).await {
            Ok(()) => {
                tracing::info!("relay config lazily provisioned from atlas");
                return crate::virtues_api::relay::load(db)
                    .await
                    .ok()
                    .flatten()
                    .map(|c| (c.relay_addr, c.sni, c.token));
            }
            Err(e) => {
                let msg = e.to_string();
                // 503 = this deployment has no relay configured; don't spin.
                if msg.contains("503") {
                    tracing::info!("relay not enabled on this deployment — LAN-only reach");
                    return None;
                }
                tracing::warn!(error = %msg, ?backoff, "lazy relay provisioning failed; retrying");
            }
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(INITIAL_ISSUE_BACKOFF_MAX);
    }
}

async fn run(
    relay_addr: String,
    sni: String,
    token_cell: Arc<RwLock<String>>,
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
    let token_fallback = token_cell.read().map(|g| g.clone()).unwrap_or_default();
    virtues_relay_client::run(virtues_relay_client::RelayClientConfig {
        relay_addr,
        sni,
        token: token_fallback,
        token_cell: Some(token_cell),
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
    // Initial issuance (load cached-if-fresh, else obtain). Retry aggressively
    // until the first browser-trusted cert lands — until then the box serves the
    // self-signed bootstrap, which every browser rejects, so the box is
    // effectively unreachable. Waiting the 12h renewal cadence to retry would
    // mean a transient ACME/DNS hiccup leaves the box dark for half a day.
    let mut backoff = INITIAL_ISSUE_BACKOFF;
    loop {
        match crate::acme::ensure_cert(&cfg, &publisher).await {
            Ok(m) => match tls::server_config_from_pem(&m.cert_pem, &m.key_pem) {
                Ok(c) => {
                    reloader.reload(c);
                    tracing::info!("ACME cert active (box-held key)");
                    break;
                }
                Err(e) => tracing::warn!(error = %e, "issued cert failed to load; retrying"),
            },
            Err(e) => {
                tracing::warn!(error = %e, ?backoff, "initial ACME issuance failed; retrying (on self-signed bootstrap until then)")
            }
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(INITIAL_ISSUE_BACKOFF_MAX);
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

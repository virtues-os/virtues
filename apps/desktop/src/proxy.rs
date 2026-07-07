//! The `:7117` localhost helper — serves the box over iroh to the local browser.
//!
//! On `virtues-client up`, this binds `127.0.0.1:7117` and, for each inbound
//! browser TCP connection, opens an iroh bi-stream to the box and raw-splices the
//! two. The box serves each bi-stream as a hyper HTTP/1 connection, so keep-alive
//! works end-to-end. The browser talks plain loopback HTTP — same-origin
//! cookies/CSRF are untouched — while the transport to the box is iroh
//! (LAN-direct → hole-punched → relay fallback).

use anyhow::{anyhow, bail, Context, Result};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use virtues_iroh::{
    build_direct_endpoint, build_endpoint, EndpointAddr, EndpointId, RelayUrl, SecretKey,
    VirtuesIrohClient,
};

use crate::keychain;

const BIND: &str = "127.0.0.1:7117";

pub async fn run() -> Result<()> {
    let rec = keychain::load_box()
        .context("load paired box")?
        .ok_or_else(|| anyhow!("not paired — run `virtues-client pair <url>` first"))?;

    let (Some(node_id_hex), Some(secret_hex)) = (rec.box_node_id, rec.device_secret_hex) else {
        bail!("this pairing is missing the box's iroh identity — re-pair to fix it.");
    };
    let box_id: EndpointId = node_id_hex.parse().context("parse box EndpointId")?;

    // Reach paths, in preference order: direct LAN/VPN addresses (no relay, no
    // discovery, no third party) and/or the relay (remote). iroh negotiates the
    // best path from whatever we supply.
    let direct: Vec<SocketAddr> =
        rec.box_direct_addrs.iter().filter_map(|s| s.parse().ok()).collect();
    let relay: Option<RelayUrl> = rec.relay_url.and_then(|s| s.parse().ok());
    if direct.is_empty() && relay.is_none() {
        bail!(
            "this pairing has no way to reach the box — no direct addresses and no relay. \
             Re-pair on the same network as the box, or claim it for remote access."
        );
    }

    let seed: [u8; 32] = hex::decode(secret_hex.trim())
        .context("decode device secret")?
        .as_slice()
        .try_into()
        .map_err(|_| anyhow!("device secret is not 32 bytes"))?;
    let secret = SecretKey::from_bytes(&seed);

    // With a relay, build the relay-capable endpoint (it still upgrades to direct
    // when reachable). LAN-only: a direct-only endpoint — zero third parties.
    let endpoint = match &relay {
        Some(r) => build_endpoint(secret, Some(r.clone())).await.context("bind iroh endpoint")?,
        None => build_direct_endpoint(secret).await.context("bind direct iroh endpoint")?,
    };
    let mut addr = EndpointAddr::new(box_id);
    for a in &direct {
        addr = addr.with_ip_addr(*a);
    }
    if let Some(r) = relay {
        addr = addr.with_relay_url(r);
    }
    let client = Arc::new(VirtuesIrohClient::new(endpoint, addr));

    let listener = TcpListener::bind(BIND)
        .await
        .with_context(|| format!("bind {BIND}"))?;
    eprintln!("virtues helper: serving your box at http://{BIND}  (Ctrl+C to stop)");

    loop {
        let (mut tcp, _peer) = match listener.accept().await {
            Ok(x) => x,
            Err(e) => {
                tracing::debug!(error = %e, "accept error");
                continue;
            }
        };
        let client = client.clone();
        tokio::spawn(async move {
            if let Err(e) = client.proxy_stream(&mut tcp).await {
                tracing::debug!(error = %format!("{e:#}"), "proxy stream ended");
            }
        });
    }
}

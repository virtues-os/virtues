//! The loopback reach helper — serve the box over iroh to a local HTTP client.
//!
//! Desktop binds `127.0.0.1:7117`; mobile binds an in-process loopback port. For
//! each inbound TCP connection we open an iroh bi-stream to the box and raw-splice
//! the two. The box serves each bi-stream as a hyper HTTP/1 connection, so
//! keep-alive works end-to-end. The local client talks plain loopback HTTP —
//! same-origin cookies/CSRF untouched — while the transport is iroh (LAN-direct →
//! hole-punched → relay).
//!
//! [`build_client`] is split out so a single warm [`VirtuesIrohClient`] can be
//! shared between the loopback and the upload path (`client.request()`).

use anyhow::{anyhow, bail, Context, Result};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use virtues_iroh::{
    build_endpoint, iroh_port, EndpointAddr, EndpointId, RelayUrl, SecretKey, VirtuesIrohClient,
};

use crate::model::PairedBox;

/// Build a warm iroh client to the box from a paired record. Re-resolves the
/// box's LAN host so reach is DHCP-proof (only the NodeId is frozen). Reusable
/// for both the loopback and direct `request()` uploads.
pub async fn build_client(rec: &PairedBox) -> Result<Arc<VirtuesIrohClient>> {
    let node_id_hex = rec
        .box_node_id
        .clone()
        .ok_or_else(|| anyhow!("this pairing is missing the box's iroh identity — re-pair to fix it."))?;
    let secret_hex = rec
        .device_secret_hex
        .clone()
        .ok_or_else(|| anyhow!("this pairing is missing the device secret — re-pair to fix it."))?;
    let box_id: EndpointId = node_id_hex.parse().context("parse box EndpointId")?;

    // Reach paths, in preference order: direct LAN/VPN addrs (no relay, no
    // discovery, no third party) and/or the relay (remote). iroh negotiates the
    // best path from whatever we supply.
    let relay: Option<RelayUrl> = rec.relay_url.as_deref().and_then(|s| s.parse().ok());

    let mut direct: Vec<SocketAddr> =
        rec.box_direct_addrs.iter().filter_map(|s| s.parse().ok()).collect();
    for a in resolve_box_lan(&rec.box_url).await {
        if !direct.contains(&a) {
            direct.push(a);
        }
    }
    if direct.is_empty() && relay.is_none() {
        bail!(
            "no way to reach the box — couldn't resolve it on this network and no relay. \
             Reach it on the same network as the box, or claim it for remote access."
        );
    }

    let seed: [u8; 32] = hex::decode(secret_hex.trim())
        .context("decode device secret")?
        .as_slice()
        .try_into()
        .map_err(|_| anyhow!("device secret is not 32 bytes"))?;
    let secret = SecretKey::from_bytes(&seed);

    // `Some(relay)` → our relay (remote, upgrades to direct when reachable);
    // `None` → relay disabled (LAN-direct only). The client binds an ephemeral
    // port (only the box pins one).
    let endpoint = build_endpoint(secret, relay.clone(), None)
        .await
        .context("bind iroh endpoint")?;
    let mut addr = EndpointAddr::new(box_id);
    for a in &direct {
        addr = addr.with_ip_addr(*a);
    }
    if let Some(r) = relay {
        addr = addr.with_relay_url(r);
    }
    Ok(Arc::new(VirtuesIrohClient::new(endpoint, addr)))
}

/// Bind `bind` and splice each inbound TCP connection to the box over iroh.
/// Loops until the listener errors unrecoverably. Callers spawn this on a task.
pub async fn serve_loopback(client: Arc<VirtuesIrohClient>, bind: SocketAddr) -> Result<()> {
    let listener = TcpListener::bind(bind)
        .await
        .with_context(|| format!("bind {bind}"))?;
    serve_on(listener, client).await
}

/// Splice inbound connections on an already-bound listener. Split out so a host
/// can bind the port *synchronously* (before pointing a webview at it, so the
/// first request queues rather than gets refused) and then serve on a task.
pub async fn serve_on(listener: TcpListener, client: Arc<VirtuesIrohClient>) -> Result<()> {
    tracing::info!(addr = ?listener.local_addr().ok(), "reach helper: serving box over iroh");
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

/// Like [`serve_on`] but fetches the *current* client from `provider` per inbound
/// connection, so a rebuilt client (after a network-change recovery that swaps the
/// process-global warm client) is picked up **without** restarting the listener.
/// `provider` returns `None` while unpaired / mid-rebuild — we just drop that
/// connection (the local HTTP client retries).
pub async fn serve_on_provider<F>(listener: TcpListener, provider: F) -> Result<()>
where
    F: Fn() -> Option<Arc<VirtuesIrohClient>> + Send + Sync + 'static,
{
    tracing::info!(addr = ?listener.local_addr().ok(), "reach helper: serving box over iroh (live client)");
    loop {
        let (mut tcp, _peer) = match listener.accept().await {
            Ok(x) => x,
            Err(e) => {
                tracing::debug!(error = %e, "accept error");
                continue;
            }
        };
        let Some(client) = provider() else {
            // No client right now (rebuilding / unpaired) — drop; caller retries.
            continue;
        };
        tokio::spawn(async move {
            if let Err(e) = client.proxy_stream(&mut tcp).await {
                tracing::debug!(error = %format!("{e:#}"), "proxy stream ended");
            }
        });
    }
}

/// Resolve the box's LAN host (from the paired `box_url`) to `IP:iroh_port`
/// socket addresses to dial by NodeId. Re-resolved on each build, so a DHCP
/// lease change never strands us. Best-effort: empty when the host can't be
/// resolved (off-LAN, or the OS can't resolve `.local`), in which case reach
/// falls back to the relay if one is present.
pub async fn resolve_box_lan(box_url: &str) -> Vec<SocketAddr> {
    let host = match url::Url::parse(box_url).ok().and_then(|u| u.host_str().map(str::to_owned)) {
        Some(h) => h,
        None => return Vec::new(),
    };
    // Bind the result before matching so the borrow of `host` ends at the await.
    let resolved = tokio::net::lookup_host((host.as_str(), iroh_port())).await;
    match resolved {
        Ok(iter) => iter.collect(),
        Err(e) => {
            tracing::debug!(host, error = %e, "could not resolve box LAN host");
            Vec::new()
        }
    }
}

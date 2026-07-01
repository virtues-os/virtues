//! The `:7117` localhost helper — serves the box over iroh to the local browser.
//!
//! On `virtues-client up`, this binds `127.0.0.1:7117` and, for each inbound
//! browser TCP connection, opens an iroh bi-stream to the box and raw-splices the
//! two. The box serves each bi-stream as a hyper HTTP/1 connection, so keep-alive
//! works end-to-end. The browser talks plain loopback HTTP — same-origin
//! cookies/CSRF are untouched — while the transport to the box is iroh
//! (LAN-direct → hole-punched → relay fallback).

use anyhow::{anyhow, bail, Context, Result};
use std::sync::Arc;
use tokio::net::TcpListener;
use virtues_iroh::{build_endpoint, EndpointId, RelayUrl, SecretKey, VirtuesIrohClient};

use crate::keychain;

const BIND: &str = "127.0.0.1:7117";

pub async fn run() -> Result<()> {
    let rec = keychain::load_box()
        .context("load paired box")?
        .ok_or_else(|| anyhow!("not paired — run `virtues-client pair <url>` first"))?;

    let (Some(node_id_hex), Some(relay_url_str), Some(secret_hex)) =
        (rec.box_node_id, rec.relay_url, rec.device_secret_hex)
    else {
        bail!(
            "this pairing has no iroh reach ticket (LAN-only). Re-pair against a \
             relay-enabled box, or open the box directly on your LAN."
        );
    };

    let box_id: EndpointId = node_id_hex.parse().context("parse box EndpointId")?;
    let relay_url: RelayUrl = relay_url_str.parse().context("parse relay url")?;
    let seed: [u8; 32] = hex::decode(secret_hex.trim())
        .context("decode device secret")?
        .as_slice()
        .try_into()
        .map_err(|_| anyhow!("device secret is not 32 bytes"))?;
    let secret = SecretKey::from_bytes(&seed);

    let endpoint = build_endpoint(secret, Some(relay_url.clone()))
        .await
        .context("bind iroh endpoint")?;
    let client = Arc::new(VirtuesIrohClient::from_relay(endpoint, box_id, relay_url));

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

use anyhow::{Context, Result};
use iroh::endpoint::{presets, BindOpts};
use iroh::{Endpoint, RelayMode, RelayUrl, SecretKey};
use std::net::{Ipv4Addr, Ipv6Addr};

/// ALPN for Virtues HTTP-over-iroh. Version in the string; a wire break bumps it.
pub const VIRTUES_ALPN: &[u8] = b"virtues/http/1";

/// Default pinned UDP port for the box's iroh endpoint (overridable via
/// `VIRTUES_IROH_PORT`). Pinned rather than OS-assigned so a restart keeps the
/// same reachable port: a LAN peer resolves the box's current IP (mDNS) and dials
/// `IP:PORT` by NodeId, with nothing frozen but the identity.
pub const DEFAULT_IROH_PORT: u16 = 51820;

/// The box's pinned iroh UDP port: `VIRTUES_IROH_PORT` or [`DEFAULT_IROH_PORT`].
pub fn iroh_port() -> u16 {
    std::env::var("VIRTUES_IROH_PORT")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(DEFAULT_IROH_PORT)
}

/// Build the node's iroh `Endpoint` on the `Minimal` preset (ring crypto, no n0
/// discovery, no n0 relay) — one transport, no third parties.
///
/// - `relay_url`: `Some` → our relay (`RelayMode::Custom`) for remote reach;
///   `None` → `RelayMode::Disabled` (LAN-direct only — a peer reaches this node
///   by its explicit direct addresses, nobody in the loop).
/// - `bind_port`: `Some(port)` → bind that fixed UDP port. Use this for the
///   **box** so its `IP:port` is stable and dialable by NodeId. `None` → an
///   OS-assigned ephemeral port, for a **dialing client** (which needs no fixed
///   port).
pub async fn build_endpoint(
    secret: SecretKey,
    relay_url: Option<RelayUrl>,
    bind_port: Option<u16>,
) -> Result<Endpoint> {
    // iroh pins its own QUIC TLS to ring (presets::Minimal), but its relay HTTP
    // client (reqwest built with `rustls-no-provider`) resolves the *process
    // default* CryptoProvider and panics if none is installed. Install ring as
    // that default here — the one choke point every consumer passes through.
    // Err = someone already installed a provider, which is exactly as good.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let mut builder = Endpoint::builder(presets::Minimal).secret_key(secret);
    builder = match relay_url {
        Some(url) => builder.relay_mode(RelayMode::Custom(url.into())),
        None => builder.relay_mode(RelayMode::Disabled),
    };
    if let Some(port) = bind_port {
        // The builder pre-binds ephemeral `0.0.0.0:0` + `[::]:0`; replace those
        // with the pinned port. IPv4 is required (fail loudly if the port is
        // taken); IPv6 is best-effort — mirroring iroh's own default, where the
        // v6 bind is allowed to fail on hosts without IPv6.
        builder = builder
            .clear_ip_transports()
            .bind_addr((Ipv4Addr::UNSPECIFIED, port))
            .context("bind iroh IPv4 port")?
            .bind_addr_with_opts(
                (Ipv6Addr::UNSPECIFIED, port),
                BindOpts::default().set_is_required(false),
            )
            .context("bind iroh IPv6 port")?;
    }
    builder.bind().await.context("bind iroh endpoint")
}

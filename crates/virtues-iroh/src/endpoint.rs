use anyhow::{Context, Result};
use iroh::endpoint::presets;
use iroh::{Endpoint, RelayMode, RelayUrl, SecretKey};

/// ALPN for Virtues HTTP-over-iroh. Version in the string; a wire break bumps it.
pub const VIRTUES_ALPN: &[u8] = b"virtues/http/1";

/// Build the node's iroh `Endpoint`.
///
/// - **prod**: pass `Some(relay_url)` → `Minimal` preset (ring crypto, no n0
///   discovery/relay) + our relay as the only relay. A peer reaches this node
///   with just its `EndpointId` + our relay URL; direct paths are then
///   negotiated over the relay and upgraded to hole-punched.
/// - **dev/spike**: pass `None` → `N0` preset (n0 relays + n0 DNS discovery).
pub async fn build_endpoint(secret: SecretKey, relay_url: Option<RelayUrl>) -> Result<Endpoint> {
    let builder = match relay_url {
        Some(url) => Endpoint::builder(presets::Minimal)
            .secret_key(secret)
            .relay_mode(RelayMode::Custom(url.into())),
        None => Endpoint::builder(presets::N0).secret_key(secret),
    };
    builder.bind().await.context("bind iroh endpoint")
}

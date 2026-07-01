//! The shared iroh 1.x reach layer for Virtues.
//!
//! - [`build_endpoint`] constructs the node's iroh `Endpoint` (box or client),
//!   pinned to the `ring` crypto provider.
//! - [`serve`] runs the box's **existing** axum `Router` over iroh: a
//!   `ProtocolHandler` for [`VIRTUES_ALPN`] accepts bi-streams and drives each
//!   through hyper against the axum service (iroh streams implement tokio
//!   `AsyncRead`/`AsyncWrite`, so no HTTP re-framing is needed).
//! - [`VirtuesIrohClient`] dials the box by `EndpointId` + relay URL and holds a
//!   warm connection, opening a cheap bi-stream per HTTP request.
//!
//! Identity is the node's Ed25519 `EndpointId` (mutual-key auth, no CA). The box
//! enforces an [`AllowPolicy`] over paired-device EndpointIds; app-layer
//! bearer/cookie auth remains the authorization keystone on top.

mod client;
mod endpoint;
mod server;

pub use client::VirtuesIrohClient;
pub use endpoint::{build_endpoint, VIRTUES_ALPN};
pub use server::{serve, AllowPolicy, StaticAllow};

// Re-export the iroh types callers need so they don't depend on iroh directly.
pub use iroh::{Endpoint, EndpointAddr, EndpointId, RelayUrl, SecretKey};

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::get, Router as AxumRouter};
    use iroh::endpoint::presets;
    use iroh::{Endpoint, EndpointAddr, RelayMode};
    use std::sync::Arc;

    /// Serve a real axum route over iroh and fetch it through the client — no
    /// relay, no discovery, no network (two in-process endpoints connect over a
    /// direct localhost QUIC path). Proves the serve()+client() HTTP path.
    #[tokio::test]
    async fn axum_over_iroh_roundtrip() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter("virtues_iroh=debug,iroh=warn")
            .try_init();
        let server_ep = Endpoint::builder(presets::Minimal)
            .relay_mode(RelayMode::Disabled)
            .bind()
            .await
            .expect("bind server");
        let client_ep = Endpoint::builder(presets::Minimal)
            .relay_mode(RelayMode::Disabled)
            .bind()
            .await
            .expect("bind client");

        let server_id = server_ep.id();
        let client_id = client_ep.id();
        let socks = server_ep.bound_sockets();
        let port = socks
            .iter()
            .find(|s| s.is_ipv4())
            .or_else(|| socks.first())
            .map(|s| s.port())
            .expect("server bound port");

        let allow: Arc<dyn AllowPolicy> = Arc::new(StaticAllow::new([client_id]));
        let app = AxumRouter::new().route("/hello", get(|| async { "world" }));
        let _iroh_router = serve(server_ep, app, allow);

        let addr = EndpointAddr::new(server_id)
            .with_ip_addr(format!("127.0.0.1:{port}").parse().unwrap());
        let client = VirtuesIrohClient::new(client_ep, addr);

        let req = b"GET /hello HTTP/1.1\r\nHost: box\r\nConnection: close\r\n\r\n";
        let resp = client.request(req).await.expect("request");
        let text = String::from_utf8_lossy(&resp);
        assert!(text.starts_with("HTTP/1.1 200"), "unexpected status: {text}");
        assert!(text.contains("world"), "missing body: {text}");
    }
}

//! The door: what a stranger holding a link can reach on a box.
//!
//! The rule it exists to keep: **the door serves only what the owner
//! published, and can reach nothing else.** So it is a separate process with
//! its own iroh key (it cannot speak as the box), no database credentials,
//! and one input: a directory of bundles the core writes.
//!
//! - [`token`]: the secret part of a link, and what counts as one.
//! - [`bundle`]: the directory layout, shared with the core, which writes it.
//! - [`protocol`]: one request per stream, a JSON line in, a JSON line and a
//!   body out.
//! - [`core`]: the one line back to the core, for live pages' approved
//!   queries and open counts.
//! - [`serve`]: the iroh handler that ties them together.

pub mod bundle;
pub mod core;
pub mod protocol;
pub mod token;

use std::sync::Arc;

use iroh::endpoint::Connection;
use iroh::protocol::{AcceptError, ProtocolHandler, Router};
use iroh::Endpoint;

/// The publish protocol. A wire break bumps the number.
pub const PUBLISH_ALPN: &[u8] = b"virtues/publish/1";

/// Requests one connection may make before the door closes it. A page load is
/// one; a live page adds a few queries. A visitor who needs more reconnects.
const MAX_REQUESTS_PER_CONNECTION: usize = 64;

#[derive(Debug, Clone)]
struct Door {
    store: Arc<bundle::Store>,
    core: core::Core,
}

impl ProtocolHandler for Door {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        // Any EndpointId may connect: a browser's key is new on every visit.
        // What it can reach is decided per request, by token.
        let remote = conn.remote_id();
        for _ in 0..MAX_REQUESTS_PER_CONNECTION {
            let Ok((send, recv)) = conn.accept_bi().await else {
                break;
            };
            let store = self.store.clone();
            let core = self.core.clone();
            tokio::spawn(async move {
                if let Err(e) = protocol::handle(&store, &core, send, recv).await {
                    tracing::debug!(%remote, error = %e, "request failed");
                }
            });
        }
        Ok(())
    }
}

/// Serve `store` on `endpoint` until the returned router is shut down.
/// `core` is where approved queries and open counts go.
pub fn serve(endpoint: Endpoint, store: bundle::Store, core: core::Core) -> Router {
    Router::builder(endpoint)
        .accept(PUBLISH_ALPN, Door { store: Arc::new(store), core })
        .spawn()
}

#[cfg(test)]
mod tests {
    use super::*;
    use iroh::endpoint::presets;
    use iroh::{EndpointAddr, RelayMode};

    /// A page published into the store comes back over a real iroh
    /// connection, and an unknown token gets the same answer as a revoked one.
    #[tokio::test]
    async fn a_published_page_is_served_over_iroh() {
        let dir = tempfile::tempdir().unwrap();
        let token = token::generate();
        bundle::write(dir.path(), &token, b"<h1>Rome</h1>", &bundle::Meta::default()).unwrap();

        let server = Endpoint::builder(presets::Minimal)
            .relay_mode(RelayMode::Disabled)
            .bind()
            .await
            .unwrap();
        let client = Endpoint::builder(presets::Minimal)
            .relay_mode(RelayMode::Disabled)
            .bind()
            .await
            .unwrap();
        let port = server
            .bound_sockets()
            .iter()
            .find(|s| s.is_ipv4())
            .map(|s| s.port())
            .unwrap();
        let addr = EndpointAddr::new(server.id())
            .with_ip_addr(format!("127.0.0.1:{port}").parse().unwrap());
        let _router = serve(server, bundle::Store::new(dir.path()), core::Core::default());

        let conn = client.connect(addr, PUBLISH_ALPN).await.unwrap();
        let ask = |line: String| {
            let conn = conn.clone();
            async move {
                let (mut send, mut recv) = conn.open_bi().await.unwrap();
                send.write_all(line.as_bytes()).await.unwrap();
                send.finish().unwrap();
                recv.read_to_end(1 << 20).await.unwrap()
            }
        };

        let found = ask(format!("{{\"op\":\"page\",\"token\":\"{token}\"}}\n")).await;
        let text = String::from_utf8(found).unwrap();
        let (head, body) = text.split_once('\n').unwrap();
        assert!(head.contains("\"ok\""), "{head}");
        assert_eq!(body, "<h1>Rome</h1>");

        let missing = ask(format!("{{\"op\":\"page\",\"token\":\"{}\"}}\n", token::generate())).await;
        assert_eq!(String::from_utf8(missing).unwrap(), "{\"status\":\"not_found\"}\n");
    }
}

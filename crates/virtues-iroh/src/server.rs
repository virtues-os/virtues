use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use hyper_util::service::TowerToHyperService;
use iroh::endpoint::Connection;
use iroh::protocol::{AcceptError, ProtocolHandler, Router};
use iroh::{Endpoint, EndpointId};
use tower::ServiceExt; // for Router::oneshot

use crate::endpoint::VIRTUES_ALPN;

/// Decides whether a remote `EndpointId` may connect. On the box this is the set
/// of non-revoked paired-device EndpointIds — a transport-level ACL below the
/// app-layer bearer/cookie authorization.
pub trait AllowPolicy: Send + Sync + 'static {
    fn is_allowed(&self, remote: EndpointId) -> bool;
}

/// A simple in-memory allowlist that the box can hot-swap as devices pair/revoke.
#[derive(Clone, Default)]
pub struct StaticAllow(Arc<RwLock<HashSet<EndpointId>>>);

impl StaticAllow {
    pub fn new(ids: impl IntoIterator<Item = EndpointId>) -> Self {
        Self(Arc::new(RwLock::new(ids.into_iter().collect())))
    }
    /// Replace the whole allowlist (called after a pairing/revocation change).
    pub fn replace(&self, ids: impl IntoIterator<Item = EndpointId>) {
        *self.0.write().unwrap() = ids.into_iter().collect();
    }
}

impl AllowPolicy for StaticAllow {
    fn is_allowed(&self, remote: EndpointId) -> bool {
        self.0.read().unwrap().contains(&remote)
    }
}

/// Serve `app` (the box's existing axum `Router`) over iroh. Returns the iroh
/// `Router` handle — call `.shutdown().await` to stop gracefully.
pub fn serve(endpoint: Endpoint, app: axum::Router, allow: Arc<dyn AllowPolicy>) -> Router {
    Router::builder(endpoint)
        .accept(VIRTUES_ALPN, HttpHandler { app, allow })
        .spawn()
}

#[derive(Clone)]
struct HttpHandler {
    app: axum::Router,
    allow: Arc<dyn AllowPolicy>,
}

impl std::fmt::Debug for HttpHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HttpHandler")
    }
}

impl ProtocolHandler for HttpHandler {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        let remote = conn.remote_id();
        if !self.allow.is_allowed(remote) {
            tracing::warn!(%remote, "iroh: rejecting connection — not allowlisted");
            conn.close(1u32.into(), b"not allowlisted");
            return Ok(());
        }
        // One HTTP/1 connection per bi-stream; the client opens a stream per
        // request. QUIC streams are cheap and independent.
        loop {
            let (send, recv) = match conn.accept_bi().await {
                Ok(pair) => pair,
                Err(_) => break, // peer closed the connection
            };
            let app = self.app.clone();
            tokio::spawn(async move {
                let io = TokioIo::new(tokio::io::join(recv, send));
                // Map hyper's Incoming body → axum's Body, then run the router.
                let svc = TowerToHyperService::new(tower::service_fn(
                    move |req: hyper::Request<hyper::body::Incoming>| {
                        let app = app.clone();
                        async move {
                            let req = req.map(axum::body::Body::new);
                            app.oneshot(req).await // Router error is Infallible
                        }
                    },
                ));
                if let Err(e) = http1::Builder::new()
                    .serve_connection(io, svc)
                    .with_upgrades() // WebSocket/SSE upgrades ride the same stream
                    .await
                {
                    tracing::debug!(error = %e, "iroh http connection ended");
                }
            });
        }
        Ok(())
    }
}

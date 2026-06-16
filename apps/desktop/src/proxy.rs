//! Local HTTP reverse proxy on `127.0.0.1:LOCAL_PROXY_PORT` (7117).
//!
//! Browser → daemon (this) → over WG tunnel → box. The browser sees an
//! `http://localhost:7117` origin (Secure Context per W3C, no TLS, no cert
//! warnings, full Fetch / Service Workers / cookies). The daemon forwards
//! upstream to the box at `http://virtues.internal:8000` over the WG tunnel.
//!
//! Note the two ports are deliberately different: the box keeps `INTERNAL_PORT`
//! (8000), but the local listener uses [`LOCAL_PROXY_PORT`] so it doesn't squat
//! 8000 — a port developers reach for constantly (Django, uvicorn, http.server).
//!
//! ## Capabilities
//!
//! - Full HTTP/1.1 streaming (request + response bodies are piped, never buffered)
//! - HTTP/1.1 protocol upgrades (WebSocket, SSE, any `Connection: Upgrade`)
//! - Per-connection upstream connection with proper Host header rewriting
//! - Surface upstream failures as 502 Bad Gateway with a triage message
//! - Hop-by-hop header stripping per RFC 7230 §6.1
//!
//! ## Lifecycle
//!
//! [`run`] binds the listener and loops accepting connections. Each accepted
//! connection runs in its own tokio task; per-request handler does the
//! upstream dial. On Ctrl-C / `virtues-client` shutdown, the task is dropped
//! and existing connections drain naturally (hyper's `serve_connection` exits
//! when the client closes).

use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use anyhow::{Context, Result};
use bytes::Bytes;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::body::Incoming;
use hyper::header::{HeaderMap, HeaderValue, CONNECTION, HOST, UPGRADE};
use hyper::server::conn::http1 as server_http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::{TcpListener, TcpStream};
use virtues_protocol::PairingBundle;

/// Port the local reverse proxy listens on (`127.0.0.1:7117`). This is the
/// origin the browser / Tauri app opens. It is intentionally NOT the box's
/// `INTERNAL_PORT` (8000): the proxy must not permanently squat 8000 on the
/// user's machine. The box's own HTTP port stays 8000 (carried in the pairing
/// bundle's `http_port`); only this local listener moves.
///
/// 7117 is a quiet registered-range port (no ephemeral-socket collisions) and
/// `localhost` is a Secure Context at any port. Keep in sync with the literal
/// `7117` in the Tauri app (`apps/web/src-tauri/src/main.rs`, `tauri.conf.json`,
/// `ui/pair.html`).
pub const LOCAL_PROXY_PORT: u16 = 7117;

/// Body type returned by every proxy handler. Lets us mix streamed responses
/// (passed through from upstream) and synthetic byte responses (502s) under
/// one type.
type ResponseBody = BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>;

/// Settings derived from the pairing bundle. Reconstructed once at startup.
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    /// Upstream address (the box's internal WG address + HTTP port). Reached
    /// via the WG tunnel; from this process's POV it's just a TCP address.
    pub upstream_addr: SocketAddr,
    /// Value to send in the upstream Host header. Daemons set this to
    /// `virtues.internal` so the box's server-side URL minting stays stable
    /// regardless of how the daemon got there.
    pub upstream_host: String,
    /// Port the local proxy binds. Defaults to [`LOCAL_PROXY_PORT`] (7117) —
    /// distinct from the box's upstream port so the proxy doesn't squat 8000.
    pub bind_port: u16,
}

impl ProxyConfig {
    pub fn from_bundle(bundle: &PairingBundle) -> Result<Self> {
        let ip: IpAddr = bundle
            .internal_ip
            .parse()
            .with_context(|| format!("parse internal_ip `{}`", bundle.internal_ip))?;
        Ok(Self {
            upstream_addr: SocketAddr::new(ip, bundle.http_port),
            upstream_host: bundle.internal_host.clone(),
            bind_port: LOCAL_PROXY_PORT,
        })
    }

    /// Like [`from_bundle`] but forwards to an explicit upstream `addr:port`
    /// instead of the box's WG-internal address — used by `virtues-client up
    /// --upstream` to reach the box over a BYO transport (Tailscale/VPS/direct
    /// IPv6). Accepts `host:port` and `[v6]:port`. Host header still uses the
    /// bundle's `internal_host` so the box's server-side URL minting is stable.
    pub fn from_bundle_with_upstream(bundle: &PairingBundle, upstream: &str) -> Result<Self> {
        let upstream_addr: SocketAddr = upstream
            .parse()
            .with_context(|| format!("`{upstream}` is not a valid host:port (try `100.64.0.2:8000` or `[2606:4700::1]:8000`)"))?;
        Ok(Self {
            upstream_addr,
            upstream_host: bundle.internal_host.clone(),
            bind_port: LOCAL_PROXY_PORT,
        })
    }
}

/// Bind the listener and serve forever. Returns only on listener error (which
/// means the kernel refused the bind; we don't fall back to another port).
pub async fn run(cfg: ProxyConfig) -> Result<()> {
    let bind = SocketAddr::from(([127, 0, 0, 1], cfg.bind_port));
    let listener = TcpListener::bind(bind)
        .await
        .with_context(|| format!("bind {bind}"))?;
    let local = listener.local_addr()?;

    eprintln!("✓ proxy listening on http://localhost:{}", local.port());
    eprintln!("  → upstream {} (Host: {})", cfg.upstream_addr, cfg.upstream_host);
    tracing::info!(
        bind = %local,
        upstream = %cfg.upstream_addr,
        upstream_host = %cfg.upstream_host,
        "proxy ready"
    );

    let cfg = Arc::new(cfg);

    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(pair) => pair,
            Err(e) => {
                tracing::warn!("accept failed: {e}");
                continue;
            }
        };
        let cfg = Arc::clone(&cfg);
        tokio::spawn(async move {
            if let Err(e) = serve_connection(stream, peer, cfg).await {
                tracing::debug!(peer = %peer, "connection ended: {e:#}");
            }
        });
    }
}

async fn serve_connection(
    stream: TcpStream,
    peer: SocketAddr,
    cfg: Arc<ProxyConfig>,
) -> Result<()> {
    let _ = stream.set_nodelay(true);
    let io = TokioIo::new(stream);
    let service = service_fn(move |req: Request<Incoming>| {
        let cfg = Arc::clone(&cfg);
        async move {
            let resp = proxy_request(req, cfg, peer).await;
            Ok::<_, Infallible>(resp)
        }
    });

    server_http1::Builder::new()
        .preserve_header_case(true)
        .title_case_headers(false)
        .serve_connection(io, service)
        .with_upgrades()
        .await
        .map_err(|e| anyhow::anyhow!("serve_connection: {e}"))
}

async fn proxy_request(
    mut req: Request<Incoming>,
    cfg: Arc<ProxyConfig>,
    peer: SocketAddr,
) -> Response<ResponseBody> {
    let method = req.method().clone();
    let uri_path = req
        .uri()
        .path_and_query()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| req.uri().path().to_string());
    let is_upgrade = is_upgrade_headers(req.headers());

    tracing::debug!(
        peer = %peer, method = %method, path = %uri_path, upgrade = is_upgrade,
        "proxy request"
    );

    // CONNECT semantics aren't part of our model — browsers never CONNECT to
    // their own origin, only via explicit HTTP proxy config. Reject cleanly.
    if method == Method::CONNECT {
        return synthetic(StatusCode::METHOD_NOT_ALLOWED, "CONNECT not supported");
    }

    // Take the inbound upgrade future BEFORE we forward the request. Once
    // `send_request` consumes the request, the inbound upgrade machinery is
    // gone, so it must come first.
    let client_upgrade = if is_upgrade {
        Some(hyper::upgrade::on(&mut req))
    } else {
        None
    };

    rewrite_request_headers(&mut req, &cfg, is_upgrade);

    // Open an upstream TCP connection + HTTP/1.1 handshake. We don't pool
    // connections in v0.2 — each request gets its own dial. Pooling adds
    // correctness risk (state leaks across requests on a long-lived tunnel)
    // and we'll measure before adding it.
    let upstream_stream = match TcpStream::connect(cfg.upstream_addr).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(peer = %peer, "upstream dial {}: {e}", cfg.upstream_addr);
            return upstream_unreachable(&cfg, e.to_string());
        }
    };
    let _ = upstream_stream.set_nodelay(true);
    let upstream_io = TokioIo::new(upstream_stream);

    let (mut sender, conn) =
        match hyper::client::conn::http1::Builder::new()
            .preserve_header_case(true)
            .title_case_headers(false)
            .handshake::<_, Incoming>(upstream_io)
            .await
        {
            Ok(pair) => pair,
            Err(e) => {
                tracing::warn!(peer = %peer, "upstream handshake: {e}");
                return upstream_unreachable(&cfg, e.to_string());
            }
        };

    // Drive the upstream connection task. For an upgrade request we use
    // `with_upgrades` so the connection stays alive after the 101 hand-off.
    if is_upgrade {
        tokio::spawn(async move {
            if let Err(e) = conn.with_upgrades().await {
                tracing::debug!("upstream conn (with-upgrades) ended: {e}");
            }
        });
    } else {
        tokio::spawn(async move {
            if let Err(e) = conn.await {
                tracing::debug!("upstream conn ended: {e}");
            }
        });
    }

    // Forward the request and await response headers (body streams below).
    let mut upstream_resp: Response<Incoming> = match sender.send_request(req).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(peer = %peer, "upstream send_request: {e}");
            return upstream_unreachable(&cfg, e.to_string());
        }
    };

    rewrite_response_headers(&mut upstream_resp);

    // If both sides agreed to switch protocols, capture the upstream upgrade
    // future BEFORE we destructure the response (otherwise it's gone).
    let upstream_upgrade = if is_upgrade
        && upstream_resp.status() == StatusCode::SWITCHING_PROTOCOLS
    {
        Some(hyper::upgrade::on(&mut upstream_resp))
    } else {
        None
    };

    // Stream the upstream body through to the client.
    let (parts, body) = upstream_resp.into_parts();
    let response_body: ResponseBody = body
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })
        .boxed();
    let resp = Response::from_parts(parts, response_body);

    // Bridge the two upgraded sockets if we got 101.
    if let (Some(client_fut), Some(upstream_fut)) = (client_upgrade, upstream_upgrade) {
        tokio::spawn(async move {
            match tokio::try_join!(client_fut, upstream_fut) {
                Ok((client_up, upstream_up)) => {
                    let mut client = TokioIo::new(client_up);
                    let mut upstream = TokioIo::new(upstream_up);
                    if let Err(e) =
                        tokio::io::copy_bidirectional(&mut client, &mut upstream).await
                    {
                        tracing::debug!("upgrade bridge ended: {e}");
                    }
                }
                Err(e) => {
                    tracing::warn!("upgrade future failed: {e}");
                }
            }
        });
    }

    resp
}

/// Rewrite incoming request headers before they go upstream.
///
/// - Force `Host` to the upstream hostname (the box's `virtues.internal`) so
///   the box's server-side absolute-URL minting (OAuth redirects, etc.) stays
///   stable regardless of whether the client typed `localhost` or `127.0.0.1`.
/// - Drop hop-by-hop headers per RFC 7230 §6.1, EXCEPT when forwarding an
///   upgrade request — in that case `Upgrade` + `Connection: upgrade` are the
///   end-to-end semantic and must reach upstream verbatim, or upstream replies
///   200 instead of 101 and the WebSocket / SSE bridge never opens.
/// - Leave the body and other headers alone so streaming works.
fn rewrite_request_headers(req: &mut Request<Incoming>, cfg: &ProxyConfig, is_upgrade: bool) {
    let host_value = format!("{}:{}", cfg.upstream_host, cfg.upstream_addr.port());
    if let Ok(hv) = HeaderValue::from_str(&host_value) {
        req.headers_mut().insert(HOST, hv);
    }

    if is_upgrade {
        // Capture the Upgrade value before the strip wipes it; re-insert
        // after along with a clean `Connection: upgrade`.
        let upgrade_val = req.headers().get(UPGRADE).cloned();
        strip_hop_by_hop(req.headers_mut());
        if let Some(uv) = upgrade_val {
            req.headers_mut().insert(UPGRADE, uv);
            req.headers_mut().insert(CONNECTION, HeaderValue::from_static("upgrade"));
        }
    } else {
        strip_hop_by_hop(req.headers_mut());
    }
}

/// Strip hop-by-hop headers from an upstream response. Same rules as the
/// request side: when the response is a 101 (protocol switch agreed), keep
/// `Upgrade` + `Connection: upgrade` so the browser sees the same dance.
fn rewrite_response_headers(resp: &mut Response<Incoming>) {
    let is_switch = resp.status() == StatusCode::SWITCHING_PROTOCOLS;
    if is_switch {
        let upgrade_val = resp.headers().get(UPGRADE).cloned();
        strip_hop_by_hop(resp.headers_mut());
        if let Some(uv) = upgrade_val {
            resp.headers_mut().insert(UPGRADE, uv);
            resp.headers_mut()
                .insert(CONNECTION, HeaderValue::from_static("upgrade"));
        }
    } else {
        strip_hop_by_hop(resp.headers_mut());
    }
}

/// Per RFC 7230 §6.1: headers naming the connection mechanism, not the
/// end-to-end message. Intermediaries MUST remove them; otherwise they leak
/// the proxy's connection state to the next hop.
fn strip_hop_by_hop(headers: &mut HeaderMap) {
    // Names that appear in any value of `Connection` are *also* hop-by-hop.
    let conn_listed: Vec<String> = headers
        .get_all(CONNECTION)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|s| s.split(',').map(|p| p.trim().to_ascii_lowercase()))
        .collect();

    const FIXED: &[&str] = &[
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
    ];

    let to_drop: Vec<hyper::header::HeaderName> = headers
        .keys()
        .filter(|name| {
            let n = name.as_str().to_ascii_lowercase();
            FIXED.contains(&n.as_str()) || conn_listed.contains(&n)
        })
        .cloned()
        .collect();
    for name in to_drop {
        headers.remove(name);
    }
}

fn is_upgrade_headers(headers: &HeaderMap) -> bool {
    headers
        .get(CONNECTION)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| s.split(',').any(|p| p.trim().eq_ignore_ascii_case("upgrade")))
        && headers.contains_key(UPGRADE)
}

// ─────────────────────────────────────────────────────────────────────────────
// Synthetic responses
// ─────────────────────────────────────────────────────────────────────────────

fn synthetic(status: StatusCode, msg: &str) -> Response<ResponseBody> {
    let body: ResponseBody = Full::new(Bytes::from(msg.to_string()))
        .map_err(|never: Infallible| -> Box<dyn std::error::Error + Send + Sync> {
            match never {}
        })
        .boxed();
    Response::builder()
        .status(status)
        .header(hyper::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(body)
        .expect("static synthetic response always builds")
}

/// Human-facing 502 with enough context to triage. Lands in the browser when
/// the tunnel is down or the box's HTTP server isn't accepting.
fn upstream_unreachable(cfg: &ProxyConfig, detail: String) -> Response<ResponseBody> {
    let msg = format!(
        "virtues-client: could not reach the box at {} ({}).\n\n\
         The tunnel may be down. Run `virtues-client status` to check tunnel state.\n",
        cfg.upstream_addr, detail
    );
    synthetic(StatusCode::BAD_GATEWAY, &msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::header::{HeaderName, HeaderValue};
    use virtues_protocol::{RendezvousParams, WgParams};

    fn fixture_bundle() -> PairingBundle {
        PairingBundle {
            bearer: "BEARER".into(),
            wg: WgParams {
                server_public_key: "spk".into(),
                server_endpoint: "[2001:db8::1]:51820".into(),
                preshared_key: "psk".into(),
                client_address: "fd00:5654::2".into(),
                server_address: "fd00:5654::1".into(),
                allowed_ips: vec!["fd00:5654::1/128".into()],
            },
            internal_host: "virtues.internal".into(),
            internal_ip: "fd00:5654::1".into(),
            http_port: 8000,
            rendezvous: RendezvousParams {
                publish_id: "abc".into(),
                key: "k".into(),
                url: "https://api/v1/rendezvous/abc".into(),
            },
        }
    }

    fn hmap(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.append(
                HeaderName::from_bytes(k.as_bytes()).unwrap(),
                HeaderValue::from_str(v).unwrap(),
            );
        }
        h
    }

    #[test]
    fn config_from_bundle_parses_ipv6() {
        let cfg = ProxyConfig::from_bundle(&fixture_bundle()).unwrap();
        assert_eq!(cfg.upstream_addr.port(), 8000); // box upstream stays 8000
        assert_eq!(cfg.upstream_host, "virtues.internal");
        assert_eq!(cfg.bind_port, LOCAL_PROXY_PORT); // local listener is 7117, not 8000
        assert_ne!(cfg.bind_port, cfg.upstream_addr.port()); // the decoupling invariant
        assert!(matches!(cfg.upstream_addr.ip(), IpAddr::V6(_)));
    }

    #[test]
    fn config_from_bundle_rejects_bad_ip() {
        let mut b = fixture_bundle();
        b.internal_ip = "not-an-ip".into();
        assert!(ProxyConfig::from_bundle(&b).is_err());
    }

    #[test]
    fn config_from_bundle_accepts_ipv4() {
        let mut b = fixture_bundle();
        b.internal_ip = "10.0.0.5".into();
        let cfg = ProxyConfig::from_bundle(&b).unwrap();
        assert!(matches!(cfg.upstream_addr.ip(), IpAddr::V4(_)));
    }

    #[test]
    fn hop_by_hop_strips_fixed_set() {
        let mut h = hmap(&[
            ("connection", "keep-alive"),
            ("keep-alive", "timeout=5"),
            ("proxy-authenticate", "Basic"),
            ("proxy-authorization", "Basic Zm9v"),
            ("te", "trailers"),
            ("trailer", "Expires"),
            ("transfer-encoding", "chunked"),
            ("upgrade", "h2c"),
            ("content-type", "application/json"),
            ("authorization", "Bearer abc"),
        ]);
        strip_hop_by_hop(&mut h);

        for name in [
            "connection",
            "keep-alive",
            "proxy-authenticate",
            "proxy-authorization",
            "te",
            "trailer",
            "transfer-encoding",
            "upgrade",
        ] {
            assert!(!h.contains_key(name), "expected `{name}` stripped");
        }
        // End-to-end headers preserved.
        assert!(h.contains_key("content-type"));
        assert!(h.contains_key("authorization"));
    }

    #[test]
    fn hop_by_hop_strips_connection_listed_headers() {
        let mut h = hmap(&[
            ("connection", "Keep-Alive, X-Custom-Hop"),
            ("x-custom-hop", "value"),
            ("x-end-to-end", "preserved"),
        ]);
        strip_hop_by_hop(&mut h);
        assert!(!h.contains_key("connection"));
        assert!(!h.contains_key("x-custom-hop"));
        assert!(h.contains_key("x-end-to-end"));
    }

    #[test]
    fn upgrade_detection_websocket() {
        let h = hmap(&[("connection", "Upgrade"), ("upgrade", "websocket")]);
        assert!(is_upgrade_headers(&h));
    }

    #[test]
    fn upgrade_detection_mixed_connection_values() {
        // Some clients send `Connection: keep-alive, Upgrade`.
        let h = hmap(&[("connection", "keep-alive, Upgrade"), ("upgrade", "websocket")]);
        assert!(is_upgrade_headers(&h));
    }

    #[test]
    fn upgrade_detection_negative_no_connection() {
        let h = hmap(&[("upgrade", "websocket")]);
        assert!(!is_upgrade_headers(&h));
    }

    #[test]
    fn upgrade_detection_negative_no_upgrade_header() {
        let h = hmap(&[("connection", "Upgrade")]);
        assert!(!is_upgrade_headers(&h));
    }

    #[test]
    fn upgrade_detection_negative_keepalive() {
        let h = hmap(&[("connection", "keep-alive")]);
        assert!(!is_upgrade_headers(&h));
    }

    #[test]
    fn hop_by_hop_strip_test_helper_paths() {
        // Verify the standalone strip_hop_by_hop matches what
        // rewrite_request_headers does in the non-upgrade branch. We can't
        // easily call rewrite_request_headers (needs Request<Incoming>) from
        // a unit test, but the strip helper IS what runs underneath — and
        // we already test it directly above. The upgrade-preservation path
        // is tested end-to-end at integration level.
        let mut h = hmap(&[
            ("connection", "upgrade"),
            ("upgrade", "websocket"),
            ("authorization", "Bearer abc"),
        ]);
        strip_hop_by_hop(&mut h);
        // Without preservation logic the strip removes both.
        assert!(!h.contains_key("upgrade"));
        assert!(!h.contains_key("connection"));
        assert!(h.contains_key("authorization"));
    }

    #[tokio::test]
    async fn synthetic_502_has_text_body() {
        let cfg = ProxyConfig::from_bundle(&fixture_bundle()).unwrap();
        let resp = upstream_unreachable(&cfg, "test".to_string());
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
        let collected = resp.into_body().collect().await.unwrap();
        let body = collected.to_bytes();
        let text = std::str::from_utf8(&body).unwrap();
        assert!(text.contains("could not reach the box"));
        assert!(text.contains("fd00:5654::1"));
    }
}

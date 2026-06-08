//! Reverse-proxy axum handler for `/service/:action_id/*path`.
//!
//! Routes external HTTP to the matching `app`-runtime child process via its
//! allocated localhost port. Streams request and response bodies to support
//! long uploads, SSE, etc.

use std::time::Duration;

use axum::{
    body::{Body, Bytes},
    extract::{Path, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use futures::stream::TryStreamExt;

use super::ServiceSupervisor;

const PROXY_TIMEOUT: Duration = Duration::from_secs(60);

/// Headers we strip when forwarding either direction. Hop-by-hop per RFC 7230,
/// plus a couple of axum-injected ones that don't make sense on the upstream.
const HOP_BY_HOP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailers",
    "transfer-encoding",
    "upgrade",
    "host",
];

/// Axum handler for the bare `/service/:action_id` route (no trailing path).
pub async fn handle_service_proxy(
    State(supervisor): State<ServiceSupervisor>,
    Path(action_id): Path<String>,
    req: Request,
) -> Response {
    serve(supervisor, action_id, req).await
}

/// Axum handler for `/service/:action_id/*rest`. Axum produces a 2-tuple of
/// path params here; we ignore `rest` because the actual path forwarded to
/// the upstream is reconstructed from the original URI inside `forward()`.
pub async fn handle_service_proxy_rest(
    State(supervisor): State<ServiceSupervisor>,
    Path((action_id, _rest)): Path<(String, String)>,
    req: Request,
) -> Response {
    serve(supervisor, action_id, req).await
}

async fn serve(supervisor: ServiceSupervisor, action_id: String, req: Request) -> Response {
    let port = match supervisor.proxy_port(&action_id).await {
        Some(p) => p,
        None => {
            // Either the action doesn't exist as an app, or it's still
            // starting / in backoff / crashed. 404 if no row, 503 otherwise.
            return match supervisor.registry.get(&action_id).await {
                Some(_state) => (
                    StatusCode::SERVICE_UNAVAILABLE,
                    [(axum::http::header::RETRY_AFTER, "2")],
                    "app not ready",
                )
                    .into_response(),
                None => (StatusCode::NOT_FOUND, "app not found").into_response(),
            };
        }
    };

    forward(port, &action_id, req).await
}

async fn forward(port: u16, action_id: &str, req: Request) -> Response {
    let (parts, body) = req.into_parts();
    let method = parts.method;
    let uri = parts.uri;
    let headers = parts.headers;

    // Reconstruct upstream URL: strip the `/service/<action_id>` prefix from
    // the request path; everything after is forwarded.
    let upstream_path = strip_proxy_prefix(&uri, action_id);
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    let upstream_url = format!("http://127.0.0.1:{port}{upstream_path}{query}");

    // Buffer the request body. For v1 this is fine (personal-AI workloads
    // are small); switch to streaming when an app needs large uploads or
    // SSE on the request side.
    let body_bytes = match axum::body::to_bytes(body, 16 * 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                format!("failed to buffer request body: {e}"),
            )
                .into_response();
        }
    };

    let client = reqwest::Client::builder()
        .timeout(PROXY_TIMEOUT)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let upstream_method =
        reqwest::Method::from_bytes(method.as_str().as_bytes()).unwrap_or(reqwest::Method::GET);

    let mut upstream_req = client
        .request(upstream_method, &upstream_url)
        .body(body_bytes.to_vec());

    for (name, value) in &headers {
        if HOP_BY_HOP.contains(&name.as_str()) {
            continue;
        }
        if let Ok(s) = value.to_str() {
            upstream_req = upstream_req.header(name.as_str(), s);
        }
    }

    let upstream_resp = match upstream_req.send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(
                action_id = %action_id,
                error = %e,
                upstream_url,
                "proxy: upstream request failed"
            );
            return (
                StatusCode::BAD_GATEWAY,
                format!("upstream error: {e}"),
            )
                .into_response();
        }
    };

    // Map status + headers back to the client.
    let status = StatusCode::from_u16(upstream_resp.status().as_u16())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    let mut out_headers = HeaderMap::new();
    for (name, value) in upstream_resp.headers() {
        if HOP_BY_HOP.contains(&name.as_str()) {
            continue;
        }
        if let Ok(hn) = axum::http::HeaderName::from_bytes(name.as_str().as_bytes()) {
            if let Ok(hv) = HeaderValue::from_bytes(value.as_bytes()) {
                out_headers.insert(hn, hv);
            }
        }
    }

    // Stream the upstream body back so SSE / large responses don't buffer.
    let resp_stream = upstream_resp
        .bytes_stream()
        .map_ok(Bytes::from)
        .map_err(std::io::Error::other);
    let body = Body::from_stream(resp_stream);

    (status, out_headers, body).into_response()
}

/// Given a request URI like `/service/<action_id>/foo/bar`, return `/foo/bar`.
/// Returns `/` when the proxied request is the bare `/service/<id>` path.
fn strip_proxy_prefix(uri: &Uri, action_id: &str) -> String {
    let path = uri.path();
    let prefix = format!("/service/{action_id}");
    if let Some(rest) = path.strip_prefix(&prefix) {
        if rest.is_empty() {
            "/".to_string()
        } else {
            rest.to_string()
        }
    } else {
        // Shouldn't happen — the route only matches /service/:action_id*.
        path.to_string()
    }
}

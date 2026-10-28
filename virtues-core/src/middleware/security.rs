//! The box's HTTP security layer.
//!
//! `headers_layer` (applied as `axum::middleware::from_fn` around both
//! routers) does three things:
//!
//!   1. It refuses a request the box takes as the owner by how it arrived
//!      when its `Host` names something the box does not serve
//!      ([`ServedNames`]). That is the defense against DNS rebinding.
//!   2. It sets `X-Frame-Options: DENY`, `X-Content-Type-Options: nosniff`,
//!      a conservative `Content-Security-Policy` and
//!      `Referrer-Policy: same-origin` on every response.
//!   3. It stamps `Cache-Control: no-store` on every response that carries a
//!      secret or a pair code ([`never_stored`]).
//!
//! There is no CSRF token. Authority comes from how a request arrives, not
//! from a cookie, and on a paired Mac that includes any connection to the
//! app's `127.0.0.1:7117` splice. A page on another site can therefore send
//! a request there as the owner. CORS keeps it from reading the reply, and
//! nothing here stops a write. agents/plan/secrets-plan.md ("Who can read
//! what") records that gap and the routes that will need a fresh owner
//! proof.

use axum::{
    extract::Request,
    http::{header, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};

// ─── Security headers ───────────────────────────────────────────────────────

pub async fn headers_layer(req: Request, next: Next) -> Response {
    if let Some(refused) = refuse_foreign_host(&req, crate::middleware::auth::is_dev(), ServedNames::get()) {
        return refused;
    }

    // Applet faces (and service-URL faces) are DESIGNED to be framed by the
    // box UI — that's the sandbox model. They must be exempt from the global
    // `X-Frame-Options: DENY` / `frame-ancestors 'none'`, and they set their
    // own sandbox-appropriate CSP which this layer must not clobber.
    let path = req.uri().path();
    let is_face = path.starts_with("/face/");
    let no_store = never_stored(path);

    let mut resp = next.run(req).await;
    let h = resp.headers_mut();
    // MIME-sniffing defense for any served file content.
    h.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    // Don't leak the full URL on outbound nav.
    h.insert("referrer-policy", HeaderValue::from_static("same-origin"));
    if no_store {
        h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        // A validator beside `no-store` is a mixed message, and some caches
        // honor the weaker half.
        h.remove(header::ETAG);
        h.remove(header::LAST_MODIFIED);
    }

    if is_face {
        // Framable by the box's own origin only; cross-origin framing still
        // denied. Leave the handler's Content-Security-Policy in place.
        h.insert("x-frame-options", HeaderValue::from_static("SAMEORIGIN"));
    } else {
        // Clickjacking — the box's UI must never render inside someone else's frame.
        h.insert("x-frame-options", HeaderValue::from_static("DENY"));
        // Conservative CSP — same-origin only. SvelteKit inlines styles, so style-src
        // allows 'unsafe-inline'; script-src stays strict. `connect-src` is the one
        // operator-tunable axis (a BYO api/atlas on a custom domain) — see csp_header.
        //
        // Except a handler's own `sandbox` policy, which only takes away: a
        // file the model's code wrote (`/api/chats/:id/files/…`) or a
        // stranger sent ([`STORED_FILE_CSP`]) is served that way so an HTML
        // or SVG file opened on its own cannot script this origin.
        let handler_sandboxed = h
            .get("content-security-policy")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("sandbox"));
        if !handler_sandboxed {
            h.insert("content-security-policy", csp_header().clone());
        }
    }
    resp
}

// ─── Host check ─────────────────────────────────────────────────────────────

/// The answer to a request whose `Host` the box does not serve, or `None` to
/// let it through.
///
/// **Why.** On a paired Mac, the app splices every connection to
/// `127.0.0.1:7117` onto its own iroh identity, and the box answers it as the
/// owner. A page at `http://evil.example:7117` whose name a DNS rebinding then
/// points at 127.0.0.1 is same-origin with itself, so its `fetch` of
/// `/api/passwords` needs no CORS and reads the reply. The splice passes the
/// bytes through unchanged, so the box sees `Host: evil.example:7117`. A
/// browser always names the host it dialed, and a page cannot change that
/// header, so refusing the name stops the page.
///
/// **Which requests.** Only those the box takes as the owner by how they
/// arrived ([`owner_by_transport`]: a proven iroh peer, a direct loopback
/// connection, and when `dev` is set any loopback connection). Any other
/// request gets only what the box gives a stranger on its network, which a
/// page could ask for at the box's own address anyway, and its `Host` is
/// whatever name the owner typed to pair (`virtues.local`, a router's name, a
/// Tailscale name), which the box cannot list.
///
/// The SPA's static files and the JSON 404 for an unknown API path are routed
/// outside this layer (`server::build_app`). Both are the same for every
/// caller.
///
/// [`owner_by_transport`]: crate::middleware::auth::owner_by_transport
fn refuse_foreign_host(req: &Request, dev: bool, names: &ServedNames) -> Option<Response> {
    if !crate::middleware::auth::owner_by_transport(req.extensions(), req.headers(), dev) {
        return None;
    }
    // Every authority the request names: each `Host` header, and the target's
    // own when it is in absolute form, which HTTP/1.1 says wins over `Host`.
    // A request that names none is no browser's, and a rebinding page is
    // always a browser's, so it passes.
    let foreign = req
        .headers()
        .get_all(header::HOST)
        .iter()
        .map(|v| v.to_str().ok())
        .chain(req.uri().authority().map(|a| Some(a.as_str())))
        .find(|named| !named.is_some_and(|n| names.serves(n, dev)))?;
    let shown: String = foreign.unwrap_or("(not text)").chars().take(100).collect();
    tracing::warn!(host = %shown, "request refused: the box does not serve this host");
    Some(
        (
            StatusCode::MISDIRECTED_REQUEST,
            Json(serde_json::json!({
                "error": "Your server does not serve the host this request named. \
                          Open it at localhost, at its IP address, or by its own name.",
            })),
        )
            .into_response(),
    )
}

/// The `Host` values the box answers on a request that carries the owner's
/// authority. Each rule is one a rebinding page cannot meet:
///
/// - **An IP literal, any port.** A rebinding needs a name to re-point. A
///   page loaded from an IP literal dials that address and nothing else.
///   This covers the Mac and iOS webviews (`127.0.0.1:<splice port>`), the
///   box's own CLI (`127.0.0.1:8000`) and a collector paired by address.
/// - **`localhost`, any port.** Browsers resolve it to loopback themselves
///   and never ask DNS. This covers the Mac app's own calls through the splice
///   and the collector (`localhost:7117`, or the port a dev profile or
///   `VIRTUES_PROXY_PORT` moves it to), `virtues-client up`, the kiosk
///   (`localhost:8000/display`), an `ssh -L` tunnel, and `make dev`, whose
///   proxy rewrites `Host` to `localhost:8000`.
/// - **`box` and `virtues` with no port.** The native iroh clients write one
///   of these by hand (`apps/web/plugins/reach/src/upload.rs`,
///   `crates/virtues-reach-client`: the MCP bridge, pairing, push, handoff).
///   A browser names any port other than 80, and nothing that carries the
///   owner's authority listens on 80 unless `VIRTUES_PROXY_PORT` puts the
///   splice there.
/// - **The box's own name**, its hostname and `<hostname>.local`, with no
///   port or with the port the box serves on. An on-box tool may dial it,
///   and a collector may have been paired by it. A page that rebinds one of
///   them reaches the splice only on the splice's port, which is neither 80
///   nor the box's.
/// - **On a developer's stack (`ENVIRONMENT=dev`) only, the names an operator
///   serves the UI from** through a proxy of their own
///   (`VIRTUES_PUBLIC_ORIGINS`, [`public_origins`]), with no port or with the
///   port the origin names. The review demo box runs this way behind Caddy:
///   a request the proxy forwards from loopback carries the owner's authority
///   there, and its `Host` is one of these names. Only the operator decides
///   where such a name points, so no page can rebind one, and a page that
///   named one on the splice's port would be refused anyway. On any other box
///   a request the proxy forwards does not carry the owner's authority, so
///   one that reaches this check came through a proxy that added no
///   forwarding header. Refusing these names there keeps such a proxy from
///   handing the owner's authority to whoever reaches it.
///
/// Matched case-insensitively and otherwise exactly: `localhost.evil.example`,
/// `localhost.` and `127.0.0.1.evil.example` are names, and refused.
///
/// [`public_origins`]: crate::server::public_origins
struct ServedNames {
    /// This machine's hostname, lowercased, without `.local`.
    hostname: Option<String>,
    /// The port `virtues server` listens on (its `--port`).
    port: u16,
    /// The host and port of each [`public_origins`] entry
    /// ([`origin_authority`]), served only on a developer's stack.
    ///
    /// [`public_origins`]: crate::server::public_origins
    proxied: Vec<(String, u16)>,
}

/// The port `virtues server` listens on, recorded by [`set_listen_port`].
static LISTEN_PORT: std::sync::OnceLock<u16> = std::sync::OnceLock::new();

/// Record the port the server listens on, so the Host check accepts the
/// box's own name on that port. `server::run` calls this before it serves the
/// first request. A later call changes nothing.
pub fn set_listen_port(port: u16) {
    let _ = LISTEN_PORT.set(port);
}

impl ServedNames {
    /// Read once: the hostname, the port and the operator's origins do not
    /// move while the server runs. A process that serves the app without
    /// `server::run`, such as a test, gets the default port.
    fn get() -> &'static ServedNames {
        static NAMES: std::sync::OnceLock<ServedNames> = std::sync::OnceLock::new();
        NAMES.get_or_init(|| {
            let mdns = crate::cli::link::mdns_host().to_ascii_lowercase();
            let hostname = mdns.trim_end_matches(".local").to_string();
            ServedNames {
                hostname: (!hostname.is_empty()).then_some(hostname),
                port: LISTEN_PORT
                    .get()
                    .copied()
                    .unwrap_or_else(crate::cli::types::default_port),
                proxied: crate::server::public_origins()
                    .iter()
                    .filter_map(|o| origin_authority(o))
                    .collect(),
            }
        })
    }

    /// Whether the box answers `authority` on an owner's request. `dev` is
    /// whether this is a developer's stack, the only place the operator's
    /// proxied names are served.
    fn serves(&self, authority: &str, dev: bool) -> bool {
        let authority = authority.to_ascii_lowercase();
        let (host, port) = split_authority(&authority);
        if is_ip_literal(host) || host == "localhost" {
            return true;
        }
        let own = self
            .hostname
            .as_deref()
            .is_some_and(|h| host == h || host.strip_suffix(".local") == Some(h));
        let proxied = |port: Option<u16>| {
            dev && self
                .proxied
                .iter()
                .any(|(h, p)| h == host && port.is_none_or(|port| port == *p))
        };
        match port {
            None => own || host == "box" || host == "virtues" || proxied(None),
            Some(port) => {
                let Ok(port) = port.parse::<u16>() else { return false };
                (own && port == self.port) || proxied(Some(port))
            }
        }
    }
}

/// The host and port a browser names when it opens `origin`, an entry of
/// [`public_origins`]: the port the origin names, or its scheme's default.
/// `None` for an entry with no usable host or port, which then serves nothing.
///
/// [`public_origins`]: crate::server::public_origins
fn origin_authority(origin: &str) -> Option<(String, u16)> {
    let (rest, default_port) = match origin.strip_prefix("https://") {
        Some(rest) => (rest, 443),
        None => (origin.strip_prefix("http://")?, 80),
    };
    let (host, port) = split_authority(rest);
    let port = match port {
        Some(port) => port.parse::<u16>().ok()?,
        None => default_port,
    };
    let bracketed = host.starts_with('[') && host.ends_with(']');
    (!host.is_empty() && (bracketed || !host.contains(':'))).then(|| (host.to_ascii_lowercase(), port))
}

/// `host[:port]` into the host and the port, if it has one. An IPv6 literal
/// is bracketed (`[::1]:7117`), so splitting on the last colon leaves the
/// brackets whole.
fn split_authority(authority: &str) -> (&str, Option<&str>) {
    match authority.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => {
            (host, Some(port))
        }
        _ => (authority, None),
    }
}

/// A dotted-quad IPv4 address or a bracketed IPv6 one. Other spellings of an
/// address (`2130706433`, `0x7f.1`) are what a browser rewrites to a dotted
/// quad before it sends `Host`, so here they are names.
fn is_ip_literal(host: &str) -> bool {
    match host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        Some(v6) => v6.parse::<std::net::Ipv6Addr>().is_ok(),
        None => host.parse::<std::net::Ipv4Addr>().is_ok(),
    }
}

// ─── Caching ────────────────────────────────────────────────────────────────

/// Whether a response carries a secret value or a pair code, which no HTTP
/// cache may keep: a saved password (`/api/passwords/:id/secret`), a key's
/// value (`/api/keys/:name/value`), and every pairing route (`/api/pair/…`:
/// a minted code or collector token, a pairing's status, a consumed
/// pairing's reply). These carry no validator, but a browser may still write
/// a storable reply to its disk cache, where it outlives the page.
/// `/api/display/state`, which shows the live pair code on the box's own
/// screen, sets `no-store` itself.
fn never_stored(path: &str) -> bool {
    let mut segments = path.strip_prefix('/').unwrap_or(path).split('/');
    match (segments.next(), segments.next(), segments.next(), segments.next(), segments.next()) {
        (Some("api"), Some("passwords"), Some(id), Some("secret"), None) => !id.is_empty(),
        (Some("api"), Some("keys"), Some(name), Some("value"), None) => !name.is_empty(),
        (Some("api"), Some("pair"), Some(route), ..) => !route.is_empty(),
        _ => false,
    }
}

// ─── Files a third party wrote ──────────────────────────────────────────────

/// The `Content-Security-Policy` for bytes the box keeps but did not write,
/// such as a message attachment. Rendered as a document, they run no script
/// and get an opaque origin, so they cannot act as the box's own page, which
/// every `/api` route answers as the owner. `style-src 'unsafe-inline'` lets
/// a browser style the page it draws around an image. [`headers_layer`] keeps
/// a policy that starts with `sandbox`.
pub const STORED_FILE_CSP: &str = "sandbox; default-src 'none'; style-src 'unsafe-inline'";

/// The type to serve stored bytes under when they may display inline, from
/// the type their sender declared, or `None` when they must download as
/// `application/octet-stream` with `Content-Disposition: attachment`.
///
/// Only raster images, audio and video display inline: none of them can run
/// script. Everything else, HTML, SVG, XML, JavaScript, PDF and text
/// included, downloads. The type matters as much as the disposition: a
/// `<script src>` on this origin ignores `Content-Disposition`, and only
/// `nosniff` with a type that is not a script's refuses to run the bytes.
pub fn inline_type(declared: &str) -> Option<&'static str> {
    let essence = declared.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    Some(match essence.as_str() {
        "image/jpeg" | "image/jpg" => "image/jpeg",
        "image/png" => "image/png",
        "image/gif" => "image/gif",
        "image/webp" => "image/webp",
        "image/avif" => "image/avif",
        "image/heic" => "image/heic",
        "image/heif" => "image/heif",
        "image/tiff" => "image/tiff",
        "audio/mpeg" => "audio/mpeg",
        "audio/mp4" => "audio/mp4",
        "audio/x-m4a" => "audio/x-m4a",
        "audio/aac" => "audio/aac",
        "audio/wav" | "audio/x-wav" => "audio/wav",
        "audio/ogg" => "audio/ogg",
        "audio/webm" => "audio/webm",
        "video/mp4" => "video/mp4",
        "video/quicktime" => "video/quicktime",
        "video/webm" => "video/webm",
        _ => return None,
    })
}

/// The `Content-Security-Policy` header value, built once on first use and
/// cached. Everything is same-origin-locked except `connect-src`, which defaults
/// to same-origin plus the Virtues cloud endpoints (api/atlas) and ws/wss for
/// live updates. A BYO deployment appends space-separated origins via
/// `VIRTUES_CSP_CONNECT_SRC_EXTRA`. Malformed operator input falls back to the
/// locked-down default (never to a weaker policy).
fn csp_header() -> &'static HeaderValue {
    static CSP: std::sync::OnceLock<HeaderValue> = std::sync::OnceLock::new();
    CSP.get_or_init(|| {
        let mut connect_src = String::from(DEFAULT_CONNECT_SRC);
        if let Ok(extra) = std::env::var("VIRTUES_CSP_CONNECT_SRC_EXTRA") {
            let extra = extra.trim();
            if !extra.is_empty() {
                connect_src.push(' ');
                connect_src.push_str(extra);
            }
        }
        HeaderValue::from_str(&csp(&connect_src)).unwrap_or_else(|_| {
            tracing::warn!(
                "VIRTUES_CSP_CONNECT_SRC_EXTRA produced an invalid CSP header; \
                 ignoring it and using the default connect-src"
            );
            HeaderValue::from_str(&csp(DEFAULT_CONNECT_SRC)).expect("the default policy is a valid header")
        })
    })
}

const DEFAULT_CONNECT_SRC: &str = "'self' https://api.virtues.com https://atlas.virtues.com ws: wss:";

/// The policy, its `connect-src` given. `script-src` allows the page's own
/// scripts and compiling WebAssembly (`'wasm-unsafe-eval'`, which permits no
/// `eval` of script): the page editor's markdown converter is the server's
/// own, compiled to WebAssembly (`apps/web/src/lib/document/converter.ts`),
/// and code highlighting runs Shiki's Oniguruma engine, also WebAssembly.
fn csp(connect_src: &str) -> String {
    format!(
        "default-src 'self'; \
         script-src 'self' 'wasm-unsafe-eval'; \
         style-src 'self' 'unsafe-inline'; \
         img-src 'self' data: blob:; \
         connect-src {connect_src}; \
         frame-ancestors 'none'; \
         base-uri 'self'; \
         form-action 'self'"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The page compiles WebAssembly, and nothing else is let through:
    /// `script-src` names exactly the page's own scripts and that.
    #[test]
    fn scripts_are_the_pages_own_and_webassembly_compiles() {
        for policy in [csp(DEFAULT_CONNECT_SRC), csp("'self' https://example.com")] {
            let script_src = policy
                .split(';')
                .map(str::trim)
                .find_map(|d| d.strip_prefix("script-src "))
                .expect("a script-src");
            assert_eq!(script_src, "'self' 'wasm-unsafe-eval'", "{policy}");
            assert!(HeaderValue::from_str(&policy).is_ok(), "{policy}");
        }
        assert!(csp_header().to_str().unwrap().contains("script-src 'self' 'wasm-unsafe-eval';"));
    }

    fn names() -> ServedNames {
        ServedNames { hostname: Some("virtues".into()), port: 8000, proxied: vec![] }
    }

    /// [`names`] on a box whose operator lists `origins`, written as
    /// `server::listed_origins` leaves them.
    fn listed(origins: &[&str]) -> ServedNames {
        ServedNames {
            proxied: origins.iter().filter_map(|o| origin_authority(o)).collect(),
            ..names()
        }
    }

    /// Every `Host` a real client sends on a request that carries the
    /// owner's authority, with or without an operator's list, on a box and
    /// on a developer's stack.
    #[test]
    fn the_hosts_real_clients_send_are_served() {
        let hosts = [
            // The Mac and iOS webviews and the Mac app through the splice,
            // on the default port, a dev profile's and an override's.
            "127.0.0.1:7117",
            "localhost:7117",
            "127.0.0.1:7163",
            "LOCALHOST:7125",
            "localhost",
            // The native iroh clients.
            "box",
            "virtues",
            // The box's CLI, its kiosk, an ssh tunnel, `make dev`'s proxy
            // and its page.
            "127.0.0.1:8000",
            "[::1]:8000",
            "localhost:8000",
            "localhost:18000",
            "localhost:5173",
            // A collector paired by the box's address or its own name.
            "192.168.1.50:8000",
            "virtues.local:8000",
            "virtues:8000",
            "virtues.local",
        ];
        for names in [names(), listed(&["https://demo.example.com", "http://localhost:5173"])] {
            for dev in [false, true] {
                for host in hosts {
                    assert!(names.serves(host, dev), "must serve {host} (dev={dev})");
                }
            }
        }
    }

    /// Names a rebinding page could present, and near-misses that a suffix
    /// or prefix test would wave through.
    #[test]
    fn a_rebound_name_is_refused() {
        let names = names();
        for host in [
            "evil.example:7117",
            "evil.example",
            "rebound.example:8000",
            "localhost.evil.example:7117",
            "127.0.0.1.evil.example:7117",
            "localhost.:7117",
            "tauri.localhost:7117",
            // The native names and the box's own, on the splice's port.
            "box:7117",
            "virtues:7117",
            "virtues.local:7117",
            "x.virtues:7117",
            "box.virtues",
            "virtues.local.evil.example",
            // A proxied name the operator did not list.
            "demo.example.com",
            // Address spellings a browser would have rewritten.
            "2130706433:7117",
            "::1:7117",
            "[::1",
            "",
        ] {
            for dev in [false, true] {
                assert!(!names.serves(host, dev), "must refuse {host:?} (dev={dev})");
            }
        }
        let unnamed = ServedNames { hostname: None, ..names };
        assert!(!unnamed.serves("virtues.local:8000", false));
        assert!(unnamed.serves("virtues", false), "the native name needs no hostname");
    }

    /// On a developer's stack, a name in `VIRTUES_PUBLIC_ORIGINS` is served
    /// bare and on the port its origin names, and on no other port: a page
    /// that rebound it onto the Mac's splice would name `:7117`. On any other
    /// box it is not served at all.
    #[test]
    fn a_listed_public_origin_is_served_on_its_own_port_alone() {
        let names = listed(&["https://demo.example.com", "https://demo-x1.example.net:8443", "http://proxy.example.org"]);
        for host in [
            "demo.example.com",
            "DEMO.example.com",
            "demo.example.com:443",
            "demo-x1.example.net",
            "demo-x1.example.net:8443",
            "proxy.example.org:80",
        ] {
            assert!(names.serves(host, true), "must serve {host} on a dev stack");
            assert!(!names.serves(host, false), "must refuse {host} off a dev stack");
        }
        for host in [
            "demo.example.com:7117",
            "demo.example.com:80",
            "demo.example.com:8000",
            "demo-x1.example.net:443",
            "proxy.example.org:443",
            "x.demo.example.com",
            "demo.example.com.evil.example",
            "example.com",
            "demo.example.com:",
            "demo.example.com:99999",
        ] {
            for dev in [false, true] {
                assert!(!names.serves(host, dev), "must refuse {host:?} (dev={dev})");
            }
        }
    }

    #[test]
    fn an_origin_names_its_host_and_port() {
        for (origin, authority) in [
            ("https://demo.example.com", Some(("demo.example.com", 443))),
            ("http://demo.example.com", Some(("demo.example.com", 80))),
            ("https://demo.example.com:8443", Some(("demo.example.com", 8443))),
            ("http://[::1]:5173", Some(("[::1]", 5173))),
            ("https://demo.example.com:", None),
            ("https://demo.example.com:http", None),
            ("https://demo.example.com:99999", None),
            ("https://", None),
            ("ws://demo.example.com", None),
            ("demo.example.com", None),
        ] {
            let got = origin_authority(origin);
            assert_eq!(got.as_ref().map(|(h, p)| (h.as_str(), *p)), authority, "{origin}");
        }
    }

    async fn call(req: axum::http::Request<axum::body::Body>) -> Response {
        use tower::Service;
        let mut app = axum::Router::new()
            .route(
                "/*rest",
                axum::routing::get(|| async {
                    (
                        [
                            (header::CACHE_CONTROL, "max-age=60"),
                            (header::ETAG, "\"v1\""),
                        ],
                        "ok",
                    )
                }),
            )
            .layer(axum::middleware::from_fn(headers_layer));
        app.call(req).await.unwrap()
    }

    fn request(path: &str, host: Option<&str>) -> axum::http::request::Builder {
        let mut b = axum::http::Request::builder().uri(path);
        if let Some(host) = host {
            b = b.header(header::HOST, host);
        }
        b
    }

    fn proven() -> virtues_iroh::ProvenPeer {
        virtues_iroh::ProvenPeer(virtues_iroh::SecretKey::from_bytes(&[3; 32]).public())
    }

    fn peer(addr: &str) -> axum::extract::ConnectInfo<std::net::SocketAddr> {
        axum::extract::ConnectInfo(addr.parse().unwrap())
    }

    /// A rebinding page on the Mac's splice arrives as a proven iroh peer
    /// with its own name in `Host`, and the handler never runs. The app's
    /// own requests, and a request that carries no authority, still pass.
    #[tokio::test]
    async fn the_layer_refuses_a_foreign_host_only_where_authority_comes_with_it() {
        let empty = axum::body::Body::empty;
        let refused = call(request("/api/passwords", Some("evil.example:7117")).extension(proven()).body(empty()).unwrap()).await;
        assert_eq!(refused.status(), StatusCode::MISDIRECTED_REQUEST);
        let body = axum::body::to_bytes(refused.into_body(), usize::MAX).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("does not serve the host"));

        // Absolute form names the authority that counts.
        let absolute = request("http://evil.example:7117/api/passwords", Some("127.0.0.1:7117"));
        assert_eq!(
            call(absolute.extension(proven()).body(empty()).unwrap()).await.status(),
            StatusCode::MISDIRECTED_REQUEST
        );
        // A loopback console request names a foreign host too.
        let console = request("/api/passwords", Some("evil.example:8000")).extension(peer("127.0.0.1:50000"));
        assert_eq!(call(console.body(empty()).unwrap()).await.status(), StatusCode::MISDIRECTED_REQUEST);

        for host in [Some("127.0.0.1:7117"), Some("localhost"), Some("box"), None] {
            let ok = request("/api/passwords", host).extension(proven());
            assert_eq!(call(ok.body(empty()).unwrap()).await.status(), StatusCode::OK, "{host:?}");
        }
        // Someone on the box's network is not the owner by how it arrived,
        // so any name passes here.
        let lan = request("/api/box/identity", Some("rebound.example:8000")).extension(peer("192.168.1.20:50000"));
        assert_eq!(call(lan.body(empty()).unwrap()).await.status(), StatusCode::OK);
    }

    /// Caddy in front of the review demo box connects from loopback and adds
    /// a forwarding header. That box runs as a developer's stack, where such
    /// a request is the owner's, so its `Host` must be a name the operator
    /// listed. On any other box it is not the owner's, and any name passes.
    /// There, a listed name arrives as the owner's only through a proxy that
    /// added no forwarding header, and it is refused.
    #[test]
    fn a_proxied_request_on_a_dev_box_names_a_listed_origin() {
        let names = listed(&["https://demo-review.example.com"]);
        let from_loopback = |host: &str, forwarded: bool| {
            let mut b = request("/api/pair/consume", Some(host)).extension(peer("127.0.0.1:50000"));
            if forwarded {
                b = b.header("x-forwarded-for", "203.0.113.9");
            }
            b.body(axum::body::Body::empty()).unwrap()
        };
        let proxied = |host: &str| from_loopback(host, true);
        let dev = true;
        assert!(refuse_foreign_host(&proxied("demo-review.example.com"), dev, &names).is_none());
        for host in ["unlisted.example.com", "demo-review.example.com:7117"] {
            let refused = refuse_foreign_host(&proxied(host), dev, &names).expect(host);
            assert_eq!(refused.status(), StatusCode::MISDIRECTED_REQUEST, "{host}");
        }

        let dev = false;
        assert!(refuse_foreign_host(&proxied("unlisted.example.com"), dev, &names).is_none());
        assert!(refuse_foreign_host(&proxied("demo-review.example.com"), dev, &names).is_none());
        let unforwarded = from_loopback("demo-review.example.com", false);
        let refused = refuse_foreign_host(&unforwarded, dev, &names).expect("a listed name with no forwarding header");
        assert_eq!(refused.status(), StatusCode::MISDIRECTED_REQUEST);
    }

    #[test]
    fn secrets_and_pair_codes_are_never_stored() {
        for path in [
            "/api/passwords/pw_1/secret",
            "/api/keys/OPENAI_API_KEY/value",
            "/api/pair/mint",
            "/api/pair/mint-collector",
            "/api/pair/status/tok_1",
            "/api/pair/consume",
        ] {
            assert!(never_stored(path), "{path}");
        }
        for path in [
            "/api/passwords",
            "/api/passwords/pw_1",
            "/api/passwords//secret",
            "/api/passwords/pw_1/secret/x",
            "/api/keys",
            "/api/keys/OPENAI_API_KEY",
            "/api/pair",
            "/api/pairs/mint",
            "/api/messages/m_1/attachments/0",
        ] {
            assert!(!never_stored(path), "{path}");
        }
    }

    /// The layer's `no-store` replaces whatever the handler said, and drops
    /// the validator beside it.
    #[tokio::test]
    async fn a_revealed_secret_carries_no_store() {
        let res = call(request("/api/passwords/pw_1/secret", None).body(axum::body::Body::empty()).unwrap()).await;
        assert_eq!(res.headers().get(header::CACHE_CONTROL).unwrap(), "no-store");
        assert!(res.headers().get(header::ETAG).is_none());

        let res = call(request("/api/pins", None).body(axum::body::Body::empty()).unwrap()).await;
        assert_eq!(res.headers().get(header::CACHE_CONTROL).unwrap(), "max-age=60");
    }

    #[test]
    fn only_images_audio_and_video_display_inline() {
        for (declared, served) in [
            ("image/jpeg", "image/jpeg"),
            ("IMAGE/JPG", "image/jpeg"),
            ("image/heic", "image/heic"),
            ("image/png; charset=binary", "image/png"),
            ("video/quicktime", "video/quicktime"),
            ("audio/x-m4a", "audio/x-m4a"),
        ] {
            assert_eq!(inline_type(declared), Some(served), "{declared}");
        }
        for declared in [
            "text/html",
            "image/svg+xml",
            "text/javascript",
            "application/javascript",
            "application/xhtml+xml",
            "text/xml",
            "application/pdf",
            "text/plain",
            "text/css",
            "",
        ] {
            assert_eq!(inline_type(declared), None, "{declared}");
        }
    }

    /// A handler's sandbox policy survives the layer; the app's own policy
    /// would let a same-origin script run in the file.
    #[tokio::test]
    async fn a_stored_files_sandbox_survives_the_layer() {
        use tower::Service;
        let mut app = axum::Router::new()
            .route(
                "/file",
                axum::routing::get(|| async { ([(header::CONTENT_SECURITY_POLICY, STORED_FILE_CSP)], "x") }),
            )
            .layer(axum::middleware::from_fn(headers_layer));
        let res = app
            .call(axum::http::Request::builder().uri("/file").body(axum::body::Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.headers().get(header::CONTENT_SECURITY_POLICY).unwrap(), STORED_FILE_CSP);
        assert_eq!(res.headers().get("x-content-type-options").unwrap(), "nosniff");
    }
}

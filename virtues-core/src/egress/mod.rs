//! The one way out of the sandbox: an HTTP proxy on a Unix socket.
//!
//! A sandboxed run has no network of its own (`PrivateNetwork=yes`). It gets
//! one socket, served by [`Proxy`], and every request goes through it. Which
//! socket a request arrived on says which run made it, so the proxy needs no
//! credentials. It:
//!
//! - accepts `CONNECT host:443` for HTTPS and absolute-form `http://host/…`
//!   requests on port 80, and nothing else;
//! - for HTTPS, reads the TLS ClientHello and refuses it unless its server
//!   name is the host the tunnel was opened for; for HTTP, sends the checked
//!   host as `Host` and one request per connection, so a shared front end
//!   cannot be asked for another site;
//! - allows any public host by name ([`policy::check_host`]);
//! - resolves the name itself and refuses it when any address it resolves to
//!   is inside the box or its network ([`policy::check_address`]), then
//!   connects to the address it checked, so DNS cannot answer differently
//!   for the check and the connection;
//! - records every request, allowed or refused, with its byte counts.
//!
//! The design is in agents/plan/applet-shape.md under Network.

pub mod policy;

use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UnixListener, UnixStream};
use tokio::task::JoinHandle;

/// The largest request head the proxy reads before refusing it.
const MAX_HEAD_BYTES: usize = 16 * 1024;
/// The largest TLS record (2^14 bytes and its 5-byte header, plus the
/// expansion TLS allows), which bounds a ClientHello read.
const MAX_TLS_RECORD: usize = 5 + 16384 + 2048;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const HEAD_TIMEOUT: Duration = Duration::from_secs(10);

/// One request through the proxy, for the run's record.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Request {
    /// The host asked for, as the client sent it, lowercased.
    pub host: String,
    /// `CONNECT` for HTTPS, otherwise the HTTP method.
    pub method: String,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    /// Why it was refused, when it was.
    pub refused: Option<String>,
}

/// How a host name becomes addresses. Production uses the system resolver;
/// tests point names at local servers, port included.
#[derive(Clone)]
pub enum Resolver {
    System,
    #[cfg(test)]
    Fixed(std::collections::HashMap<String, Vec<SocketAddr>>),
}

impl Resolver {
    async fn resolve(&self, host: &str, port: u16) -> std::io::Result<Vec<SocketAddr>> {
        match self {
            Resolver::System => Ok(tokio::net::lookup_host((host, port)).await?.collect()),
            #[cfg(test)]
            Resolver::Fixed(map) => {
                let _ = port;
                Ok(map.get(host).cloned().unwrap_or_default())
            }
        }
    }
}

/// How one run's requests are resolved and their addresses checked.
#[derive(Clone)]
pub struct Grant {
    pub resolver: Resolver,
    /// The address check. [`policy::check_address`] against the box's own
    /// addresses in production.
    pub check_address: Arc<dyn Fn(IpAddr) -> Result<(), String> + Send + Sync>,
}

impl Grant {
    /// The system resolver, with the box's own addresses refused.
    pub fn new() -> Self {
        let own: Vec<IpAddr> = crate::net_check::list_interfaces()
            .into_iter()
            .flat_map(|(_, addrs)| addrs)
            .collect();
        Self {
            resolver: Resolver::System,
            check_address: Arc::new(move |ip| policy::check_address(ip, &own)),
        }
    }
}

/// A proxy serving one run on one socket. Dropping it stops it and removes
/// the socket.
pub struct Proxy {
    path: PathBuf,
    log: Arc<Mutex<Vec<Request>>>,
    /// Connections being served, so [`Proxy::finish`] can wait for them.
    open: Arc<std::sync::atomic::AtomicUsize>,
    task: JoinHandle<()>,
}

impl Proxy {
    /// Listen on `path`, which must not exist yet.
    pub fn start(path: &Path, grant: Grant) -> std::io::Result<Self> {
        let listener = UnixListener::bind(path)?;
        // The run's throwaway user connects to it; the socket is bind-mounted
        // into that run's unit alone, so the mode is not what keeps others out.
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o666))?;
        }
        let log: Arc<Mutex<Vec<Request>>> = Arc::default();
        let open: Arc<std::sync::atomic::AtomicUsize> = Arc::default();
        let grant = Arc::new(grant);
        let task = {
            let (log, open) = (log.clone(), open.clone());
            tokio::spawn(async move {
                loop {
                    let Ok((stream, _)) = listener.accept().await else { break };
                    let (grant, log, open) = (grant.clone(), log.clone(), open.clone());
                    open.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    tokio::spawn(async move {
                        let request = serve(stream, &grant).await;
                        if let Some(r) = request {
                            log.lock().expect("egress log poisoned").push(r);
                        }
                        open.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                    });
                }
            })
        };
        Ok(Self { path: path.to_path_buf(), log, open, task })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Every request so far, in the order each finished.
    pub fn requests(&self) -> Vec<Request> {
        self.log.lock().expect("egress log poisoned").clone()
    }

    /// Stop taking connections, give the open ones up to `grace` to close,
    /// and return every request. A request is recorded when its connection
    /// closes, which can be just after the client process has exited.
    pub async fn finish(self, grace: Duration) -> Vec<Request> {
        self.task.abort();
        let deadline = tokio::time::Instant::now() + grace;
        while self.open.load(std::sync::atomic::Ordering::SeqCst) > 0 && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        self.requests()
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.task.abort();
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Serve one client connection: read the request head, decide, and either
/// refuse or relay until either side closes. `None` when the client sent no
/// request at all.
async fn serve(mut client: UnixStream, grant: &Grant) -> Option<Request> {
    let mut head = Vec::with_capacity(1024);
    let end = match tokio::time::timeout(HEAD_TIMEOUT, read_head(&mut client, &mut head)).await {
        Ok(Ok(Some(end))) => end,
        Ok(Ok(None)) if head.is_empty() => return None,
        _ => {
            let _ = refuse(&mut client, 400, "The request was not a complete HTTP request head.").await;
            return None;
        }
    };
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut parsed = httparse::Request::new(&mut headers);
    if !matches!(parsed.parse(&head[..end]), Ok(httparse::Status::Complete(_))) {
        let _ = refuse(&mut client, 400, "The request head could not be parsed.").await;
        return None;
    }
    let method = parsed.method.unwrap_or_default().to_string();
    let target = parsed.path.unwrap_or_default().to_string();

    // Where the client wants to go, and what to send the server first.
    let (host, port, preface) = if method == "CONNECT" {
        let Some((host, port)) = target.rsplit_once(':') else {
            return Some(refused(&mut client, &target, &method, "CONNECT needs host:port.").await);
        };
        (host.to_ascii_lowercase(), port.parse::<u16>().unwrap_or(0), None)
    } else {
        let Some(rest) = target.strip_prefix("http://") else {
            return Some(
                refused(&mut client, &target, &method, "Only CONNECT for HTTPS and http:// URLs go through this proxy.")
                    .await,
            );
        };
        let (authority, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) => (h.to_ascii_lowercase(), p.parse::<u16>().unwrap_or(0)),
            None => (authority.to_ascii_lowercase(), 80),
        };
        let path = if path.is_empty() { "/" } else { path };
        (host.clone(), port, Some(origin_form(&parsed, &method, &host, path)))
    };

    let wanted_port = if method == "CONNECT" { 443 } else { 80 };
    if port != wanted_port {
        let why = format!("Port {port} is refused. HTTPS goes to 443 and HTTP to 80.");
        return Some(refused(&mut client, &host, &method, &why).await);
    }
    let host = match policy::check_host(&host) {
        Ok(h) => h,
        Err(why) => return Some(refused(&mut client, &host, &method, &why).await),
    };
    let addrs = match grant.resolver.resolve(&host, port).await {
        Ok(a) if !a.is_empty() => a,
        _ => {
            let why = format!("{host} did not resolve.");
            return Some(refused(&mut client, &host, &method, &why).await);
        }
    };
    // Every address must pass, not just the first: a name that resolves to
    // one public and one private address is pointing inside.
    if let Some(why) = addrs.iter().find_map(|a| (grant.check_address)(a.ip()).err()) {
        let why = format!("{host} resolves to an address that is refused: {why}");
        return Some(refused(&mut client, &host, &method, &why).await);
    }
    let upstream = match tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(&addrs[..])).await {
        Ok(Ok(s)) => s,
        _ => {
            let why = format!("Could not connect to {host}.");
            return Some(refused(&mut client, &host, &method, &why).await);
        }
    };

    let early = &head[end..];
    let first = match preface {
        Some(mut p) => {
            p.extend_from_slice(early);
            p
        }
        None => {
            // The tunnel is open from the client's side; what it sends first
            // must be a ClientHello for the host it asked for.
            if client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await.is_err() {
                return None;
            }
            let hello = match tokio::time::timeout(HEAD_TIMEOUT, read_client_hello(&mut client, early.to_vec())).await {
                Ok(Ok(h)) => h,
                _ => return Some(closed(&host, &method, "The tunnel did not start with a TLS ClientHello.")),
            };
            match server_name(&hello) {
                Some(name) if name.eq_ignore_ascii_case(&host) => hello,
                Some(name) => {
                    let why = format!("The TLS server name {name} is not {host}, which the tunnel was opened for.");
                    return Some(closed(&host, &method, &why));
                }
                None => return Some(closed(&host, &method, "The TLS ClientHello names no server.")),
            }
        }
    };
    let (sent, received) = relay(client, upstream, &first).await;
    Some(Request { host, method, bytes_sent: sent, bytes_received: received, refused: None })
}

/// Read the client's first TLS record, starting from bytes already read.
/// A ClientHello fits in one record in practice; one split across records is
/// refused rather than reassembled.
async fn read_client_hello(client: &mut UnixStream, mut buf: Vec<u8>) -> std::io::Result<Vec<u8>> {
    let mut chunk = [0u8; 4096];
    loop {
        if buf.len() >= 5 {
            let want = 5 + u16::from_be_bytes([buf[3], buf[4]]) as usize;
            if buf[0] != 0x16 || want > MAX_TLS_RECORD {
                return Err(std::io::Error::other("not a TLS handshake record"));
            }
            if buf.len() >= want {
                return Ok(buf);
            }
        }
        let n = client.read(&mut chunk).await?;
        if n == 0 {
            return Err(std::io::Error::other("closed before the ClientHello"));
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

/// The `server_name` a TLS record holding a ClientHello asks for, or `None`
/// when it is not one, names no host, or does not parse.
fn server_name(record: &[u8]) -> Option<String> {
    let take = |b: &[u8], at: usize, n: usize| b.get(at..at + n).map(|s| s.to_vec());
    let u16_at = |b: &[u8], at: usize| take(b, at, 2).map(|s| u16::from_be_bytes([s[0], s[1]]) as usize);
    let record_len = u16_at(record, 3)?;
    let hs = record.get(5..5 + record_len)?;
    if *hs.first()? != 0x01 {
        return None;
    }
    let hs_len = u32::from_be_bytes([0, *hs.get(1)?, *hs.get(2)?, *hs.get(3)?]) as usize;
    let body = hs.get(4..4 + hs_len)?;
    let mut at = 2 + 32; // client_version, random
    at += 1 + *body.get(at)? as usize; // session_id
    at += 2 + u16_at(body, at)?; // cipher_suites
    at += 1 + *body.get(at)? as usize; // compression_methods
    let ext_end = at + 2 + u16_at(body, at)?;
    at += 2;
    while at + 4 <= ext_end {
        let (kind, len) = (u16_at(body, at)?, u16_at(body, at + 2)?);
        let data = body.get(at + 4..at + 4 + len)?;
        if kind == 0 {
            // server_name_list: length, then (type, length, name) entries.
            let mut i = 2;
            while i + 3 <= data.len() {
                let n = u16_at(data, i + 1)?;
                if data[i] == 0 {
                    return String::from_utf8(data.get(i + 3..i + 3 + n)?.to_vec()).ok();
                }
                i += 3 + n;
            }
            return None;
        }
        at += 4 + len;
    }
    None
}

/// Read until the blank line that ends a request head. Returns the offset
/// just past it, `None` at end of stream, or an error past the size limit.
async fn read_head(client: &mut UnixStream, buf: &mut Vec<u8>) -> std::io::Result<Option<usize>> {
    let mut chunk = [0u8; 2048];
    loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            return Ok(Some(i + 4));
        }
        if buf.len() > MAX_HEAD_BYTES {
            return Err(std::io::Error::other("request head too large"));
        }
        let n = client.read(&mut chunk).await?;
        if n == 0 {
            return Ok(None);
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

/// The request rewritten for the origin server: origin-form target, no
/// proxy headers, and one request per connection so the proxy sees every
/// request's host.
fn origin_form(parsed: &httparse::Request, method: &str, host: &str, path: &str) -> Vec<u8> {
    // The checked host, never the client's own `Host`, which a shared front
    // end would route by.
    let mut out = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\n").into_bytes();
    for h in parsed.headers.iter() {
        let name = h.name.to_ascii_lowercase();
        if name.starts_with("proxy-") || matches!(name.as_str(), "host" | "connection" | "keep-alive" | "upgrade") {
            continue;
        }
        out.extend_from_slice(h.name.as_bytes());
        out.extend_from_slice(b": ");
        out.extend_from_slice(h.value);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(b"Connection: close\r\n\r\n");
    out
}

/// Send `first` upstream, then copy both ways until either side closes.
/// Returns bytes sent upstream and bytes received from it.
async fn relay(mut client: UnixStream, mut upstream: TcpStream, first: &[u8]) -> (u64, u64) {
    if upstream.write_all(first).await.is_err() {
        return (0, 0);
    }
    let sent = first.len() as u64;
    match tokio::io::copy_bidirectional(&mut client, &mut upstream).await {
        Ok((up, down)) => (sent + up, down),
        Err(_) => (sent, 0),
    }
}

async fn refuse(client: &mut UnixStream, status: u16, why: &str) -> std::io::Result<()> {
    let reason = if status == 403 { "Forbidden" } else { "Bad Request" };
    let body = format!("{why}\n");
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    client.write_all(response.as_bytes()).await
}

async fn refused(client: &mut UnixStream, host: &str, method: &str, why: &str) -> Request {
    let _ = refuse(client, 403, why).await;
    closed(host, method, why)
}

/// A refusal recorded after the tunnel opened, when no HTTP answer can be
/// sent: the connection just closes.
fn closed(host: &str, method: &str, why: &str) -> Request {
    tracing::info!(host, method, why, "egress refused");
    Request {
        host: host.to_ascii_lowercase(),
        method: method.to_string(),
        bytes_sent: 0,
        bytes_received: 0,
        refused: Some(why.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tokio::net::TcpListener;

    /// A server on loopback that reads what it is sent, answers `reply`, and
    /// hands back what it read.
    async fn upstream(reply: &'static [u8]) -> (SocketAddr, tokio::sync::oneshot::Receiver<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let n = s.read(&mut buf).await.unwrap();
            s.write_all(reply).await.unwrap();
            s.shutdown().await.unwrap();
            let _ = tx.send(buf[..n].to_vec());
        });
        (addr, rx)
    }

    fn addr(s: &str) -> SocketAddr {
        s.parse().unwrap()
    }

    /// A grant resolving through `names`. `inward_ok` lets a loopback test
    /// server through, which only a test may do.
    fn grant(names: &[(&str, Vec<SocketAddr>)], inward_ok: bool) -> Grant {
        Grant {
            resolver: Resolver::Fixed(names.iter().map(|(n, a)| (n.to_string(), a.clone())).collect::<HashMap<_, _>>()),
            check_address: if inward_ok {
                Arc::new(|_| Ok(()))
            } else {
                Arc::new(|ip| policy::check_address(ip, &[]))
            },
        }
    }

    fn socket() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("egress.sock");
        (dir, path)
    }

    async fn ask(proxy: &Proxy, request: &str) -> String {
        let mut c = UnixStream::connect(proxy.path()).await.unwrap();
        c.write_all(request.as_bytes()).await.unwrap();
        let mut out = Vec::new();
        let _ = tokio::time::timeout(Duration::from_secs(5), c.read_to_end(&mut out)).await;
        String::from_utf8_lossy(&out).into_owned()
    }

    /// The log once `n` requests have finished.
    async fn settled(proxy: &Proxy, n: usize) -> Vec<Request> {
        for _ in 0..100 {
            if proxy.requests().len() >= n {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        proxy.requests()
    }

    #[tokio::test]
    async fn a_listed_host_is_reached_over_http_and_the_request_is_recorded() {
        let (server, seen) = upstream(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok").await;
        let (_dir, path) = socket();
        let proxy = Proxy::start(&path, grant(&[("api.example.com", vec![server])], true)).unwrap();

        let reply = ask(
            &proxy,
            "GET http://api.example.com/tides?x=1 HTTP/1.1\r\nHost: other.example.net\r\nProxy-Authorization: x\r\n\r\n",
        )
        .await;
        assert!(reply.ends_with("ok"), "{reply}");
        let forwarded = String::from_utf8(seen.await.unwrap()).unwrap();
        assert!(forwarded.starts_with("GET /tides?x=1 HTTP/1.1\r\n"), "{forwarded}");
        assert!(!forwarded.to_ascii_lowercase().contains("proxy-authorization"), "{forwarded}");
        assert!(forwarded.contains("Connection: close"), "{forwarded}");
        assert_eq!(forwarded.matches("Host: ").count(), 1, "{forwarded}");
        assert!(forwarded.contains("Host: api.example.com\r\n"), "the checked host, not the client's: {forwarded}");

        let log = settled(&proxy, 1).await;
        assert_eq!(log.len(), 1);
        assert_eq!((log[0].host.as_str(), log[0].method.as_str()), ("api.example.com", "GET"));
        assert!(log[0].refused.is_none());
        assert!(log[0].bytes_sent > 0 && log[0].bytes_received > 0);
    }

    #[tokio::test]
    async fn connect_opens_a_tunnel_that_relays_both_ways() {
        let (server, seen) = upstream(b"pong").await;
        let (_dir, path) = socket();
        let proxy = Proxy::start(&path, grant(&[("api.example.com", vec![server])], true)).unwrap();

        let mut c = UnixStream::connect(proxy.path()).await.unwrap();
        c.write_all(b"CONNECT api.example.com:443 HTTP/1.1\r\nHost: api.example.com:443\r\n\r\n").await.unwrap();
        let mut buf = vec![0u8; 128];
        let n = c.read(&mut buf).await.unwrap();
        assert!(String::from_utf8_lossy(&buf[..n]).starts_with("HTTP/1.1 200"), "tunnel opens");
        let hello = client_hello("api.example.com");
        c.write_all(&hello).await.unwrap();
        let mut out = Vec::new();
        c.read_to_end(&mut out).await.unwrap();
        assert_eq!(out, b"pong");
        assert_eq!(seen.await.unwrap(), hello);
        // A request is recorded when both sides have closed; finish waits.
        drop(c);
        let log = proxy.finish(Duration::from_secs(2)).await;
        assert_eq!(
            (log[0].method.as_str(), log[0].bytes_sent, log[0].bytes_received),
            ("CONNECT", hello.len() as u64, 4)
        );
    }

    /// A minimal TLS record holding a ClientHello that names `host`.
    fn client_hello(host: &str) -> Vec<u8> {
        let name = host.as_bytes();
        let mut sni = vec![0u8, 0];
        let list_len = (3 + name.len()) as u16;
        let ext_len = 2 + list_len;
        sni.extend_from_slice(&ext_len.to_be_bytes());
        sni.extend_from_slice(&list_len.to_be_bytes());
        sni.push(0);
        sni.extend_from_slice(&(name.len() as u16).to_be_bytes());
        sni.extend_from_slice(name);
        let mut body = vec![3u8, 3];
        body.extend_from_slice(&[7u8; 32]);
        body.push(0); // session id
        body.extend_from_slice(&[0, 2, 0x13, 0x01]); // one cipher suite
        body.extend_from_slice(&[1, 0]); // null compression
        body.extend_from_slice(&(sni.len() as u16).to_be_bytes());
        body.extend_from_slice(&sni);
        let mut hs = vec![1u8];
        hs.extend_from_slice(&(body.len() as u32).to_be_bytes()[1..]);
        hs.extend_from_slice(&body);
        let mut record = vec![0x16, 3, 1];
        record.extend_from_slice(&(hs.len() as u16).to_be_bytes());
        record.extend_from_slice(&hs);
        record
    }

    /// A tunnel opened for one host may not carry a TLS session for another,
    /// which a shared front end would route by its server name.
    #[tokio::test]
    async fn a_tunnel_refuses_a_different_tls_server_name() {
        assert_eq!(server_name(&client_hello("api.example.com")).as_deref(), Some("api.example.com"));
        assert_eq!(server_name(b"\x16\x03\x01\x00\x01\x02"), None);
        let (server, _seen) = upstream(b"pong").await;
        let (_dir, path) = socket();
        let proxy = Proxy::start(&path, grant(&[("api.example.com", vec![server])], true)).unwrap();
        for first in [client_hello("other.example.net"), b"ping".to_vec()] {
            let mut c = UnixStream::connect(proxy.path()).await.unwrap();
            c.write_all(b"CONNECT api.example.com:443 HTTP/1.1\r\n\r\n").await.unwrap();
            c.write_all(&first).await.unwrap();
            let mut out = Vec::new();
            let _ = tokio::time::timeout(Duration::from_secs(5), c.read_to_end(&mut out)).await;
            assert!(!String::from_utf8_lossy(&out).contains("pong"));
        }
        let log = settled(&proxy, 2).await;
        assert!(log.iter().all(|r| r.refused.is_some()), "{log:?}");
        assert_eq!(log.iter().map(|r| r.bytes_sent).sum::<u64>(), 0);
    }

    #[tokio::test]
    async fn ip_literals_and_inward_hosts_are_refused_and_recorded() {
        let (_dir, path) = socket();
        let public = addr("93.184.216.34:443");
        let names = [
            ("api.example.com", vec![public]),
            ("inside.example.com", vec![public, addr("192.168.1.10:443")]),
            ("metadata.example.com", vec![addr("169.254.169.254:80")]),
        ];
        let proxy = Proxy::start(&path, grant(&names, false)).unwrap();

        let cases = [
            ("CONNECT other.example.com:443 HTTP/1.1\r\n\r\n", "did not resolve"),
            ("CONNECT 93.184.216.34:443 HTTP/1.1\r\n\r\n", "is an IP address"),
            ("CONNECT inside.example.com:443 HTTP/1.1\r\n\r\n", "192.168.1.10 is a private network address"),
            ("GET http://metadata.example.com/latest HTTP/1.1\r\n\r\n", "169.254.169.254 is link-local"),
            ("CONNECT api.example.com:22 HTTP/1.1\r\n\r\n", "Port 22 is refused"),
            ("GET https://api.example.com/ HTTP/1.1\r\n\r\n", "Only CONNECT for HTTPS"),
        ];
        for (request, expect) in cases {
            let reply = ask(&proxy, request).await;
            assert!(reply.starts_with("HTTP/1.1 403"), "{request}: {reply}");
            assert!(reply.contains(expect), "{request}: {reply}");
        }
        let log = settled(&proxy, cases.len()).await;
        assert_eq!(log.len(), cases.len());
        assert!(log.iter().all(|r| r.refused.is_some() && r.bytes_sent == 0));
    }

    /// The box's own API listens on loopback. With the real address check a
    /// listed name that resolves there is refused.
    #[tokio::test]
    async fn a_name_pointing_at_loopback_cannot_reach_the_box() {
        let (server, _seen) = upstream(b"HTTP/1.1 200 OK\r\n\r\n").await;
        let (_dir, path) = socket();
        let proxy = Proxy::start(&path, grant(&[("api.example.com", vec![server])], false)).unwrap();
        let reply = ask(&proxy, "GET http://api.example.com/ HTTP/1.1\r\n\r\n").await;
        assert!(reply.contains("127.0.0.1 is loopback"), "{reply}");
    }

}

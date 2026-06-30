//! Box-held-cert TLS termination for the LAN-direct listener.
//!
//! In the relay model the box **terminates TLS itself** with its own per-box
//! cert — both for LAN-direct clients and (over the relay's blind passthrough)
//! for remote clients. The relay never holds this key. The cert is normally
//! obtained via ACME/DNS-01; [`self_signed`] provides a bootstrap cert for dev
//! and for first-boot before the ACME cert is issued.
//!
//! Uses an explicit `ring` provider (no dependency on a process-wide default
//! `CryptoProvider` being installed) so it's safe to call from a library.

use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::server::TlsStream;
use tokio_rustls::{rustls, TlsAcceptor};

/// Build a `rustls::ServerConfig` from PEM cert chain + private key.
pub fn server_config_from_pem(cert_pem: &str, key_pem: &str) -> anyhow::Result<rustls::ServerConfig> {
    let certs: Vec<CertificateDer<'static>> =
        rustls_pemfile::certs(&mut cert_pem.as_bytes()).collect::<Result<_, _>>()?;
    if certs.is_empty() {
        anyhow::bail!("no certificates found in cert PEM");
    }
    let key: PrivateKeyDer<'static> = rustls_pemfile::private_key(&mut key_pem.as_bytes())?
        .ok_or_else(|| anyhow::anyhow!("no private key found in key PEM"))?;

    let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_no_client_auth()
    .with_single_cert(certs, key)?;

    // Offer HTTP/2 (with HTTP/1.1 fallback). The box terminates TLS here and
    // splices the *decrypted* stream to its local `axum::serve` server, which
    // auto-detects HTTP/2 cleartext (h2c) — so a browser that negotiates `h2`
    // multiplexes a whole page over ONE TLS connection, hence one relay
    // work-conn, instead of opening 6+ parallel connections (each its own
    // OpenConn → work-conn dance). Browsers without h2 fall back to http/1.1.
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(config)
}

/// Generate a self-signed cert + key (PEM) for `names` — dev / pre-ACME bootstrap.
pub fn self_signed(names: Vec<String>) -> anyhow::Result<(String, String)> {
    let key = rcgen::generate_simple_self_signed(names)?;
    Ok((key.cert.pem(), key.key_pair.serialize_pem()))
}

/// A TCP listener that terminates TLS with the box's cert on each accept. The
/// cert is **hot-swappable** via [`TlsListener::reloader`] so a renewed ACME
/// cert (or the upgrade from the self-signed bootstrap to the first ACME cert)
/// takes effect on the next accept without rebinding the socket.
pub struct TlsListener {
    inner: TcpListener,
    config: Arc<RwLock<Arc<rustls::ServerConfig>>>,
}

/// Handle that swaps the cert a [`TlsListener`] presents. Cheap to clone and
/// move into a background renewal task.
#[derive(Clone)]
pub struct CertReloader {
    config: Arc<RwLock<Arc<rustls::ServerConfig>>>,
}

impl CertReloader {
    /// Atomically replace the served `ServerConfig`. The next accepted
    /// connection handshakes with the new cert; in-flight ones are unaffected.
    pub fn reload(&self, config: rustls::ServerConfig) {
        *self.config.write().expect("cert lock poisoned") = Arc::new(config);
    }
}

impl TlsListener {
    pub fn new(inner: TcpListener, config: rustls::ServerConfig) -> Self {
        Self {
            inner,
            config: Arc::new(RwLock::new(Arc::new(config))),
        }
    }

    /// A handle for hot-swapping the served cert.
    pub fn reloader(&self) -> CertReloader {
        CertReloader {
            config: self.config.clone(),
        }
    }

    /// Accept and complete a TLS handshake with the *currently-loaded* cert.
    /// Returns the decrypted stream + peer.
    ///
    /// Note: this awaits the handshake inline, so it must NOT be called in a bare
    /// accept loop — a slow/stalled client handshake would block every other
    /// pending connection (head-of-line DoS). For a server loop use
    /// [`Self::accept_raw`] + [`Self::handshake`] and run the handshake in a
    /// spawned task. This convenience form is for one-shot/test callers.
    pub async fn accept(&self) -> io::Result<(TlsStream<TcpStream>, SocketAddr)> {
        let (stream, peer, config) = self.accept_raw().await?;
        let tls = Self::handshake(config, stream).await?;
        Ok((tls, peer))
    }

    /// Accept a raw TCP connection plus a snapshot of the cert config currently
    /// loaded, WITHOUT handshaking. Run [`Self::handshake`] on the returned
    /// stream inside a spawned task so a stalled handshake can't block the loop.
    pub async fn accept_raw(
        &self,
    ) -> io::Result<(TcpStream, SocketAddr, Arc<rustls::ServerConfig>)> {
        let (stream, peer) = self.inner.accept().await?;
        // Snapshot the current config (brief lock, no await held) so a concurrent
        // reload doesn't tear out the Arc mid-handshake.
        let config = self.config.read().expect("cert lock poisoned").clone();
        Ok((stream, peer, config))
    }

    /// Complete a TLS handshake on a previously-[`accept_raw`](Self::accept_raw)ed
    /// stream with the given (snapshotted) cert config.
    pub async fn handshake(
        config: Arc<rustls::ServerConfig>,
        stream: TcpStream,
    ) -> io::Result<TlsStream<TcpStream>> {
        TlsAcceptor::from(config).accept(stream).await
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn box_held_cert_tls_roundtrip() {
        let (cert_pem, key_pem) = self_signed(vec!["box.local".into()]).unwrap();
        let server_config = server_config_from_pem(&cert_pem, &key_pem).unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let tls_listener = TlsListener::new(listener, server_config);

        // Server: accept, read 5 bytes, echo them.
        tokio::spawn(async move {
            let (mut s, _) = tls_listener.accept().await.unwrap();
            let mut buf = [0u8; 5];
            s.read_exact(&mut buf).await.unwrap();
            s.write_all(&buf).await.unwrap();
            s.flush().await.unwrap();
        });

        // Client: trust the self-signed cert, connect, write, read back.
        let mut roots = rustls::RootCertStore::empty();
        let cert = rustls_pemfile::certs(&mut cert_pem.as_bytes())
            .next()
            .unwrap()
            .unwrap();
        roots.add(cert).unwrap();
        let client_config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();

        let connector = TlsConnector::from(Arc::new(client_config));
        let tcp = TcpStream::connect(addr).await.unwrap();
        let domain = rustls::pki_types::ServerName::try_from("box.local").unwrap();
        let mut tls = connector.connect(domain, tcp).await.unwrap();

        tls.write_all(b"hello").await.unwrap();
        tls.flush().await.unwrap();
        let mut got = [0u8; 5];
        tls.read_exact(&mut got).await.unwrap();
        assert_eq!(&got, b"hello");
    }

    /// The renewal/bootstrap-swap mechanism (#3/#6): `CertReloader::reload`
    /// atomically changes the cert the listener presents on the next accept.
    #[tokio::test]
    async fn cert_hot_swap_changes_served_cert() {
        // Two distinct self-signed certs for the same name.
        let (cert_a, key_a) = self_signed(vec!["box.local".into()]).unwrap();
        let (cert_b, key_b) = self_signed(vec!["box.local".into()]).unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let tls_listener =
            TlsListener::new(listener, server_config_from_pem(&cert_a, &key_a).unwrap());
        let reloader = tls_listener.reloader();

        // Accept loop: handshake then drop. We only assert on cert verification.
        tokio::spawn(async move {
            loop {
                if let Ok((mut s, _)) = tls_listener.accept().await {
                    let _ = s.shutdown().await;
                }
            }
        });

        // Initially serving A: a client that trusts A connects; one trusting B fails.
        assert!(try_connect(addr, &cert_a).await.is_ok(), "A-trusting client should reach A");
        assert!(try_connect(addr, &cert_b).await.is_err(), "B-trusting client should not reach A");

        // Hot-swap to B.
        reloader.reload(server_config_from_pem(&cert_b, &key_b).unwrap());

        // Now the served cert is B: A-trusting fails, B-trusting succeeds.
        assert!(
            try_connect(addr, &cert_a).await.is_err(),
            "A-trusting client should fail after swap to B"
        );
        assert!(
            try_connect(addr, &cert_b).await.is_ok(),
            "B-trusting client should reach B after swap"
        );
    }

    /// Connect to `addr` trusting only `trusted_cert_pem` as a root. `Ok` iff the
    /// server presented a cert that chains to it (a self-signed cert is its own
    /// root), so this doubles as "which cert is being served right now".
    async fn try_connect(addr: SocketAddr, trusted_cert_pem: &str) -> anyhow::Result<()> {
        let mut roots = rustls::RootCertStore::empty();
        let cert = rustls_pemfile::certs(&mut trusted_cert_pem.as_bytes())
            .next()
            .unwrap()?;
        roots.add(cert)?;
        let client_config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()?
        .with_root_certificates(roots)
        .with_no_client_auth();

        let connector = TlsConnector::from(Arc::new(client_config));
        let tcp = TcpStream::connect(addr).await?;
        let domain = rustls::pki_types::ServerName::try_from("box.local")?;
        let mut tls = connector.connect(domain, tcp).await?;
        tls.shutdown().await.ok();
        Ok(())
    }

    use tokio_rustls::TlsConnector;
}

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
use std::sync::Arc;
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

    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_no_client_auth()
    .with_single_cert(certs, key)?;
    Ok(config)
}

/// Generate a self-signed cert + key (PEM) for `names` — dev / pre-ACME bootstrap.
pub fn self_signed(names: Vec<String>) -> anyhow::Result<(String, String)> {
    let key = rcgen::generate_simple_self_signed(names)?;
    Ok((key.cert.pem(), key.key_pair.serialize_pem()))
}

/// A TCP listener that terminates TLS with the box's cert on each accept.
pub struct TlsListener {
    inner: TcpListener,
    acceptor: TlsAcceptor,
}

impl TlsListener {
    pub fn new(inner: TcpListener, config: rustls::ServerConfig) -> Self {
        Self {
            inner,
            acceptor: TlsAcceptor::from(Arc::new(config)),
        }
    }

    /// Accept and complete a TLS handshake. Returns the decrypted stream + peer.
    pub async fn accept(&self) -> io::Result<(TlsStream<TcpStream>, SocketAddr)> {
        let (stream, peer) = self.inner.accept().await?;
        let tls = self.acceptor.accept(stream).await?;
        Ok((tls, peer))
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

    use tokio_rustls::TlsConnector;
}

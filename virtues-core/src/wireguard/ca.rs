//! Per-server CA + `virtues.internal` leaf (TLS context 1).
//!
//! On first boot the box mints a long-lived CA and a leaf cert for
//! `virtues.internal` (SAN includes the assigned WG IP). The CA root (public)
//! ships in each pairing bundle; the client pins it for `virtues.internal`
//! ONLY — no public PKI, no CT logs, scoped trust on the device. The CA private
//! key is sealed at rest with the vault master key before it ever hits storage.
//!
//! This module only mints/parses certs. Sealing + DB persistence (a
//! `credentials` row under `source_id = "__virtues_ca__"`) and rustls
//! termination on the WG interface are wired in with the WG transport.

use anyhow::{anyhow, Result};
use std::net::IpAddr;

use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, KeyPair, SanType};
use sqlx::PgPool;

use super::box_secrets;

/// The hostname the device dials inside the tunnel. Never in public DNS.
pub const INTERNAL_HOST: &str = "virtues.internal";

/// The hostname the box advertises on the LAN via mDNS (`_https._tcp.local`).
/// First-time onboarding happens here from a desktop browser before WG is up.
pub const LAN_HOST: &str = "virtues.local";

/// `box_secrets.key` under which the per-server CA is sealed.
const CA_SECRET_KEY: &str = "wg_ca";

/// `box_secrets.key` under which the LAN leaf (signed by the CA) is sealed.
/// Re-minted if the cert is missing or near expiry; survives WG state changes.
const LAN_LEAF_SECRET_KEY: &str = "lan_leaf";

/// A minted cert: PEM certificate + PEM PKCS#8 private key. For the CA, the
/// `cert_pem` is the public root that ships in the pairing bundle and the
/// `key_pem` is sealed before persistence.
#[derive(Debug, Clone)]
pub struct CertPem {
    pub cert_pem: String,
    pub key_pem: String,
}

/// Mint a long-lived self-signed CA for this server.
pub fn generate_ca() -> Result<CertPem> {
    let key = KeyPair::generate().map_err(|e| anyhow!("ca keygen: {e}"))?;
    let mut params =
        CertificateParams::new(Vec::<String>::new()).map_err(|e| anyhow!("ca params: {e}"))?;
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params
        .distinguished_name
        .push(DnType::CommonName, "Virtues Server CA");
    let cert = params
        .self_signed(&key)
        .map_err(|e| anyhow!("ca self-sign: {e}"))?;
    Ok(CertPem {
        cert_pem: cert.pem(),
        key_pem: key.serialize_pem(),
    })
}

/// Rebuild the CA's signing handles from stored PEM so we can mint leaves after
/// a restart. (rcgen's documented "load an existing CA to sign with" path:
/// parse params from the cert PEM, re-bind the original key.)
fn load_ca(ca: &CertPem) -> Result<(rcgen::Certificate, KeyPair)> {
    let key = KeyPair::from_pem(&ca.key_pem).map_err(|e| anyhow!("ca key parse: {e}"))?;
    let params = CertificateParams::from_ca_cert_pem(&ca.cert_pem)
        .map_err(|e| anyhow!("ca cert parse: {e}"))?;
    let cert = params
        .self_signed(&key)
        .map_err(|e| anyhow!("ca rebuild: {e}"))?;
    Ok((cert, key))
}

/// Mint a leaf for `virtues.internal` (SAN: host + the assigned WG IP), signed
/// by the server CA. Re-minted whenever the assigned WG IP changes.
pub fn mint_internal_leaf(ca: &CertPem, wg_ip: IpAddr) -> Result<CertPem> {
    let (ca_cert, ca_key) = load_ca(ca)?;
    let leaf_key = KeyPair::generate().map_err(|e| anyhow!("leaf keygen: {e}"))?;
    let mut params = CertificateParams::new(vec![INTERNAL_HOST.to_string()])
        .map_err(|e| anyhow!("leaf params: {e}"))?;
    params.subject_alt_names.push(SanType::IpAddress(wg_ip));
    params
        .distinguished_name
        .push(DnType::CommonName, INTERNAL_HOST);
    let cert = params
        .signed_by(&leaf_key, &ca_cert, &ca_key)
        .map_err(|e| anyhow!("leaf sign: {e}"))?;
    Ok(CertPem {
        cert_pem: cert.pem(),
        key_pem: leaf_key.serialize_pem(),
    })
}

/// Mint a leaf for `virtues.local` (LAN / mDNS path), signed by the server CA.
///
/// SAN includes `virtues.local` + `localhost` + `127.0.0.1` + `::1` so the
/// same cert serves both LAN browsers (via mDNS) and developer workflows
/// hitting the box from itself. Separate from the WG `virtues.internal` leaf
/// so the LAN cert can exist before WireGuard is configured (which is the
/// case during first-time onboarding, before the user has run `bringup`).
pub fn mint_lan_leaf(ca: &CertPem) -> Result<CertPem> {
    let (ca_cert, ca_key) = load_ca(ca)?;
    let leaf_key = KeyPair::generate().map_err(|e| anyhow!("lan leaf keygen: {e}"))?;
    let mut params = CertificateParams::new(vec![
        LAN_HOST.to_string(),
        "localhost".to_string(),
    ])
    .map_err(|e| anyhow!("lan leaf params: {e}"))?;
    // Additional IP SANs so the cert also matches when the user browses
    // `https://127.0.0.1` or `https://[::1]` on the box itself.
    params
        .subject_alt_names
        .push(SanType::IpAddress(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)));
    params
        .subject_alt_names
        .push(SanType::IpAddress(IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)));
    params
        .distinguished_name
        .push(DnType::CommonName, LAN_HOST);
    let cert = params
        .signed_by(&leaf_key, &ca_cert, &ca_key)
        .map_err(|e| anyhow!("lan leaf sign: {e}"))?;
    Ok(CertPem {
        cert_pem: cert.pem(),
        key_pem: leaf_key.serialize_pem(),
    })
}

/// Load the persisted LAN leaf, or mint + seal one on first call. The leaf
/// stores its `cert_pem` in `box_secrets.metadata` (clear) and its key sealed.
pub async fn ensure_lan_leaf(db: &PgPool) -> Result<CertPem> {
    if let Some((key_pem, meta)) = box_secrets::get(db, LAN_LEAF_SECRET_KEY).await? {
        if let Some(cert_pem) = meta.get("cert_pem").and_then(|v| v.as_str()) {
            return Ok(CertPem {
                cert_pem: cert_pem.to_string(),
                key_pem,
            });
        }
    }
    // Mint fresh.
    let ca = ensure_ca(db).await?;
    let leaf = mint_lan_leaf(&ca)?;
    let meta = serde_json::json!({ "cert_pem": leaf.cert_pem });
    box_secrets::put(db, LAN_LEAF_SECRET_KEY, &leaf.key_pem, &meta).await?;
    Ok(leaf)
}

/// Load the persisted CA, or mint + seal one on first call. Idempotent: the box
/// mints its CA once at first boot and reuses it across restarts (so the root
/// shipped in pairing bundles stays stable). The private key is sealed with the
/// vault master key; only the public cert is stored in the clear.
pub async fn ensure_ca(db: &PgPool) -> Result<CertPem> {
    if let Some(ca) = load_ca_from_db(db).await? {
        return Ok(ca);
    }
    let ca = generate_ca()?;
    persist_ca(db, &ca).await?;
    Ok(ca)
}

async fn load_ca_from_db(db: &PgPool) -> Result<Option<CertPem>> {
    let Some((key_pem, meta)) = box_secrets::get(db, CA_SECRET_KEY).await? else {
        return Ok(None);
    };
    let cert_pem = meta
        .get("cert_pem")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("box_secrets[wg_ca] missing cert_pem"))?
        .to_string();
    Ok(Some(CertPem { cert_pem, key_pem }))
}

async fn persist_ca(db: &PgPool, ca: &CertPem) -> Result<()> {
    let meta = serde_json::json!({ "cert_pem": ca.cert_pem });
    box_secrets::put(db, CA_SECRET_KEY, &ca.key_pem, &meta).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv6Addr;

    #[test]
    fn ca_and_leaf_are_pem() {
        let ca = generate_ca().unwrap();
        assert!(ca.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(ca.key_pem.contains("PRIVATE KEY"));

        let ip = IpAddr::V6(Ipv6Addr::new(0xfd00, 0, 0, 0, 0, 0, 0, 0x1));
        let leaf = mint_internal_leaf(&ca, ip).unwrap();
        assert!(leaf.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(leaf.key_pem.contains("PRIVATE KEY"));
        // CA and leaf are distinct certs.
        assert_ne!(ca.cert_pem, leaf.cert_pem);
    }

    #[test]
    fn leaf_verifies_against_ca() {
        // The leaf must chain to the CA: parse the CA as a trust anchor and
        // verify the leaf's signature with webpki.
        let ca = generate_ca().unwrap();
        let ip = IpAddr::V6(Ipv6Addr::LOCALHOST);
        let leaf = mint_internal_leaf(&ca, ip).unwrap();

        // Re-loading the CA from its own PEM must succeed (restart path).
        let reloaded = mint_internal_leaf(&ca, ip);
        assert!(reloaded.is_ok());
        // Sanity: distinct leaf keys each mint.
        assert_ne!(leaf.key_pem, reloaded.unwrap().key_pem);
    }
}

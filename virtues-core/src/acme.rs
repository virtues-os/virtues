//! ACME (DNS-01) for the box's own per-box cert.
//!
//! The **box generates and holds the private key**; our authority's *only* role
//! is writing the `_acme-challenge` TXT record (the sandcats 2020 model). This
//! gives the box a browser-trusted cert for `*.<boxhash>.boxes.virtues.com`
//! without the relay ever touching the key.
//!
//! CA stack (free, capacity-uncapped): **Let's Encrypt primary** (apply for the
//! rate-limit override), with **Google Trust Services** as a DNS-01 failover.
//! Wildcards require DNS-01, which is exactly this flow.
//!
//! This module is structurally complete and compiles; the live issuance path is
//! only exercisable against a real ACME directory + a DNS authority that can
//! write the TXT record, so callers fall back to a self-signed bootstrap cert
//! ([`crate::relay`]) until that infra is configured.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::time::Duration;

use instant_acme::{
    Account, AuthorizationStatus, ChallengeType, Identifier, NewAccount, NewOrder, OrderStatus,
};

/// Issued cert material — both PEM strings, ready for
/// [`virtues_helpers::transport::tls::server_config_from_pem`].
pub struct CertMaterial {
    pub cert_pem: String,
    pub key_pem: String,
}

/// How to publish a DNS-01 challenge TXT via the Virtues authority. Abstracted
/// so it's testable and so the authority client can evolve independently.
#[async_trait::async_trait]
pub trait DnsPublisher: Send + Sync {
    /// Publish `value` at TXT record `name` (e.g. `_acme-challenge.box.boxes.virtues.com`).
    async fn publish_txt(&self, name: &str, value: &str) -> Result<()>;
}

/// Config for obtaining a cert. `None` from [`AcmeConfig::from_env`] means ACME
/// is not configured and the caller should use the self-signed bootstrap.
pub struct AcmeConfig {
    /// ACME directory URL (LE prod/staging, or Google Trust Services).
    pub directory_url: String,
    /// Names to certify — typically `<boxhash>.boxes.virtues.com` and the
    /// per-box wildcard `*.<boxhash>.boxes.virtues.com`.
    pub names: Vec<String>,
    /// Contact email (optional; `mailto:` is added).
    pub contact_email: Option<String>,
    /// Seconds to wait after publishing TXT records before telling the CA they're
    /// ready (DNS propagation slack).
    pub propagation_secs: u64,
    /// Where to cache the issued cert+key on the box.
    pub cert_dir: PathBuf,
}

impl AcmeConfig {
    /// Build from env, or `None` if not configured. Requires `VIRTUES_ACME_DIRECTORY`
    /// and at least one name (`VIRTUES_RELAY_SNI`).
    pub fn from_env() -> Option<Self> {
        let directory_url = std::env::var("VIRTUES_ACME_DIRECTORY").ok().filter(|s| !s.is_empty())?;
        let sni = std::env::var("VIRTUES_RELAY_SNI").ok().filter(|s| !s.is_empty())?;
        // Per-box wildcard covers the LAN dashed-IP name too.
        let names = vec![sni.clone(), format!("*.{sni}")];
        let cert_dir = std::env::var("VIRTUES_TLS_CERT_DIR")
            .unwrap_or_else(|_| "./data/tls".to_string())
            .into();
        Some(Self {
            directory_url,
            names,
            contact_email: std::env::var("VIRTUES_ACME_CONTACT").ok().filter(|s| !s.is_empty()),
            propagation_secs: std::env::var("VIRTUES_ACME_PROPAGATION_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(15),
            cert_dir,
        })
    }
}

/// Load a cached cert from disk, else obtain a fresh one via ACME and cache it.
/// (Expiry-based renewal is handled by the renewal loop; this is load-or-obtain.)
pub async fn ensure_cert(cfg: &AcmeConfig, publisher: &dyn DnsPublisher) -> Result<CertMaterial> {
    if let Some(existing) = load_from_disk(&cfg.cert_dir) {
        tracing::info!(dir = %cfg.cert_dir.display(), "using cached TLS cert");
        return Ok(existing);
    }
    let material = obtain(cfg, publisher).await.context("obtain ACME cert")?;
    if let Err(e) = save_to_disk(&cfg.cert_dir, &material) {
        tracing::warn!(error = %e, "failed to cache issued cert (continuing in-memory)");
    }
    Ok(material)
}

/// Run the full ACME DNS-01 flow and return the issued chain + locally-held key.
pub async fn obtain(cfg: &AcmeConfig, publisher: &dyn DnsPublisher) -> Result<CertMaterial> {
    let contact: Vec<String> = cfg
        .contact_email
        .as_ref()
        .map(|e| vec![format!("mailto:{e}")])
        .unwrap_or_default();
    let contact_refs: Vec<&str> = contact.iter().map(|s| s.as_str()).collect();

    // TODO(P3): persist + reuse AccountCredentials rather than creating a fresh
    // account per issuance (LE rate-limits accounts too).
    let (account, _credentials) = Account::create(
        &NewAccount {
            contact: &contact_refs,
            terms_of_service_agreed: true,
            only_return_existing: false,
        },
        &cfg.directory_url,
        None,
    )
    .await
    .context("create ACME account")?;

    let identifiers: Vec<Identifier> = cfg.names.iter().cloned().map(Identifier::Dns).collect();
    let mut order = account
        .new_order(&NewOrder {
            identifiers: &identifiers,
        })
        .await
        .context("create ACME order")?;

    // Publish a DNS-01 TXT for each pending authorization.
    let authorizations = order.authorizations().await.context("fetch authorizations")?;
    let mut challenge_urls = Vec::with_capacity(authorizations.len());
    for authz in &authorizations {
        match authz.status {
            AuthorizationStatus::Pending => {}
            AuthorizationStatus::Valid => continue,
            other => anyhow::bail!("unexpected authorization status: {other:?}"),
        }
        let challenge = authz
            .challenges
            .iter()
            .find(|c| c.r#type == ChallengeType::Dns01)
            .ok_or_else(|| anyhow::anyhow!("no dns-01 challenge offered"))?;
        let Identifier::Dns(domain) = &authz.identifier;
        let value = order.key_authorization(challenge).dns_value();
        publisher
            .publish_txt(&format!("_acme-challenge.{domain}"), &value)
            .await
            .with_context(|| format!("publish TXT for {domain}"))?;
        challenge_urls.push(challenge.url.clone());
    }

    // Give DNS time to propagate, then tell the CA the challenges are ready.
    tokio::time::sleep(Duration::from_secs(cfg.propagation_secs)).await;
    for url in &challenge_urls {
        order.set_challenge_ready(url).await.context("set challenge ready")?;
    }

    // Poll until the order is Ready (or Invalid), with exponential backoff.
    let mut delay = Duration::from_millis(250);
    for _ in 0..10 {
        tokio::time::sleep(delay).await;
        let state = order.refresh().await.context("refresh order")?;
        match state.status {
            OrderStatus::Ready => break,
            OrderStatus::Invalid => anyhow::bail!("ACME order became invalid"),
            _ => delay = (delay * 2).min(Duration::from_secs(8)),
        }
    }
    if order.state().status != OrderStatus::Ready {
        anyhow::bail!("ACME order not ready after polling");
    }

    // Finalize with a locally-generated key (the box holds it) + CSR.
    let key_pair = rcgen::KeyPair::generate().context("generate key pair")?;
    let mut params = rcgen::CertificateParams::new(cfg.names.clone()).context("cert params")?;
    params.distinguished_name = rcgen::DistinguishedName::new();
    let csr = params.serialize_request(&key_pair).context("serialize CSR")?;
    order.finalize(csr.der()).await.context("finalize order")?;

    let cert_pem = loop {
        match order.certificate().await.context("download certificate")? {
            Some(pem) => break pem,
            None => tokio::time::sleep(Duration::from_secs(1)).await,
        }
    };

    Ok(CertMaterial {
        cert_pem,
        key_pem: key_pair.serialize_pem(),
    })
}

fn load_from_disk(dir: &PathBuf) -> Option<CertMaterial> {
    let cert_pem = std::fs::read_to_string(dir.join("cert.pem")).ok()?;
    let key_pem = std::fs::read_to_string(dir.join("key.pem")).ok()?;
    if cert_pem.is_empty() || key_pem.is_empty() {
        return None;
    }
    Some(CertMaterial { cert_pem, key_pem })
}

fn save_to_disk(dir: &PathBuf, material: &CertMaterial) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("cert.pem"), &material.cert_pem)?;
    std::fs::write(dir.join("key.pem"), &material.key_pem)?;
    Ok(())
}

/// HTTP `DnsPublisher` that POSTs `{name, value}` to the Virtues authority's
/// TXT-writer endpoint (`VIRTUES_DNS_TXT_URL`, bearer `VIRTUES_DNS_TXT_TOKEN`).
pub struct HttpDnsPublisher {
    url: String,
    token: String,
    client: reqwest::Client,
}

impl HttpDnsPublisher {
    pub fn from_env() -> Option<Self> {
        let url = std::env::var("VIRTUES_DNS_TXT_URL").ok().filter(|s| !s.is_empty())?;
        Some(Self {
            url,
            token: std::env::var("VIRTUES_DNS_TXT_TOKEN").unwrap_or_default(),
            client: reqwest::Client::new(),
        })
    }
}

#[async_trait::async_trait]
impl DnsPublisher for HttpDnsPublisher {
    async fn publish_txt(&self, name: &str, value: &str) -> Result<()> {
        let resp = self
            .client
            .post(&self.url)
            .bearer_auth(&self.token)
            .json(&serde_json::json!({ "name": name, "value": value }))
            .send()
            .await
            .context("POST TXT record")?;
        if !resp.status().is_success() {
            anyhow::bail!("authority rejected TXT publish: {}", resp.status());
        }
        Ok(())
    }
}

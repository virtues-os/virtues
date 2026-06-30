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
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Renew once the cached cert is older than this. Let's Encrypt certs live 90
/// days, so renewing at 60 leaves a 30-day window to retry on failure.
pub const RENEW_AFTER: Duration = Duration::from_secs(60 * 24 * 3600);

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
    /// Publish the full set of `values` for TXT record `name` (e.g.
    /// `_acme-challenge.box.boxes.virtues.com`) as one atomic RRset.
    ///
    /// A box certifying both an apex (`b.boxes.virtues.com`) and its wildcard
    /// (`*.b.boxes.virtues.com`) gets two DNS-01 authorizations that share the
    /// single TXT name `_acme-challenge.b.boxes.virtues.com` but require
    /// **different** values present **simultaneously**. The authority MUST set
    /// the record to exactly `values` (replace the whole RRset), not overwrite
    /// it value-by-value, or one authorization fails validation.
    async fn publish_txt(&self, name: &str, values: &[String]) -> Result<()>;
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

/// Return a usable cert: the cached one if it's still fresh, else obtain a new
/// one via ACME and cache it. Expiry-driven renewal of a *running* listener is
/// handled separately by [`crate::relay`]'s renewal loop (it calls [`obtain`] +
/// [`save_to_disk`] and hot-swaps the cert); this is the startup load-or-obtain.
pub async fn ensure_cert(cfg: &AcmeConfig, publisher: &dyn DnsPublisher) -> Result<CertMaterial> {
    if !cert_stale(&cfg.cert_dir) {
        if let Some(existing) = load_from_disk(&cfg.cert_dir).await {
            tracing::info!(dir = %cfg.cert_dir.display(), "using cached TLS cert");
            return Ok(existing);
        }
    }
    let material = obtain(cfg, publisher).await.context("obtain ACME cert")?;
    if let Err(e) = save_to_disk(&cfg.cert_dir, &material).await {
        tracing::warn!(error = %e, "failed to cache issued cert (continuing in-memory)");
    }
    Ok(material)
}

/// True if there is no usable, still-fresh cached cert — missing, or older than
/// [`RENEW_AFTER`]. A cert with no `issued_at` marker is treated as stale so the
/// next issuance writes one (and so we never serve an unknown-age cert forever).
pub fn cert_stale(cert_dir: &Path) -> bool {
    match cert_age(cert_dir) {
        Some(age) => age >= RENEW_AFTER,
        None => true,
    }
}

/// Age of the cached cert from its `issued_at` marker, or `None` if absent.
fn cert_age(cert_dir: &Path) -> Option<Duration> {
    let secs: u64 = std::fs::read_to_string(cert_dir.join("issued_at"))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    let issued = std::time::UNIX_EPOCH + Duration::from_secs(secs);
    std::time::SystemTime::now().duration_since(issued).ok()
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

    // Collect the DNS-01 challenge values, grouped by TXT record name. The apex
    // and its wildcard share `_acme-challenge.<base>` with different values, so
    // we must publish both values for that name together (one atomic RRset),
    // else the second publish clobbers the first and one authz fails.
    let authorizations = order.authorizations().await.context("fetch authorizations")?;
    let mut entries: Vec<(String, String)> = Vec::with_capacity(authorizations.len());
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
        // `authz.identifier` for a wildcard order carries the *base* domain (the
        // `*.` is stripped), so apex + wildcard collapse to the same TXT name.
        let Identifier::Dns(domain) = &authz.identifier;
        let value = order.key_authorization(challenge).dns_value();
        entries.push((domain.clone(), value));
        challenge_urls.push(challenge.url.clone());
    }
    for (name, values) in &group_txt_values(&entries) {
        publisher
            .publish_txt(name, values)
            .await
            .with_context(|| format!("publish TXT for {name}"))?;
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

/// Group DNS-01 challenge values by their `_acme-challenge.<domain>` TXT name.
/// The apex and its wildcard share one TXT name with **different** values, so
/// they must be published together as one RRset — this is what prevents the
/// second publish from clobbering the first. `entries` is `(domain, value)`.
fn group_txt_values(entries: &[(String, String)]) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut m: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    for (domain, value) in entries {
        m.entry(format!("_acme-challenge.{domain}"))
            .or_default()
            .push(value.clone());
    }
    m
}

async fn load_from_disk(dir: &Path) -> Option<CertMaterial> {
    let cert_pem = tokio::fs::read_to_string(dir.join("cert.pem")).await.ok()?;
    let key_pem = tokio::fs::read_to_string(dir.join("key.pem")).await.ok()?;
    if cert_pem.is_empty() || key_pem.is_empty() {
        return None;
    }
    Some(CertMaterial { cert_pem, key_pem })
}

/// Cache the issued cert + a unix-seconds `issued_at` marker (drives renewal).
/// `pub(crate)` so the relay renewal loop persists renewed certs too.
pub(crate) async fn save_to_disk(dir: &Path, material: &CertMaterial) -> Result<()> {
    tokio::fs::create_dir_all(dir).await?;
    tokio::fs::write(dir.join("cert.pem"), &material.cert_pem).await?;
    tokio::fs::write(dir.join("key.pem"), &material.key_pem).await?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    tokio::fs::write(dir.join("issued_at"), now.to_string()).await?;
    Ok(())
}

/// HTTP `DnsPublisher` that POSTs `{name, values: [..]}` to the Virtues
/// authority's TXT-writer endpoint (`VIRTUES_DNS_TXT_URL`, bearer
/// `VIRTUES_DNS_TXT_TOKEN`). The authority replaces the whole RRset with `values`.
pub struct HttpDnsPublisher {
    url: String,
    token: String,
    client: reqwest::Client,
}

impl HttpDnsPublisher {
    pub fn from_env() -> Option<Self> {
        let url = std::env::var("VIRTUES_DNS_TXT_URL").ok().filter(|s| !s.is_empty())?;
        // Must go through `base_builder()`: virtues-core's reqwest is built with
        // `rustls-tls-no-provider`, so a bare `reqwest::Client::new()` has an
        // empty trust store and every HTTPS publish fails. See http_client.rs.
        let client = crate::http_client::base_builder()
            .build()
            .expect("build ACME DNS HTTP client");
        Some(Self {
            url,
            token: std::env::var("VIRTUES_DNS_TXT_TOKEN").unwrap_or_default(),
            client,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apex_and_wildcard_share_one_txt_name_with_both_values() {
        // What `from_env` builds for sni "h.boxes.virtues.com": the apex and the
        // wildcard. instant-acme strips the `*.`, so both authorizations report
        // the same base domain — and thus the same `_acme-challenge` TXT name —
        // with different challenge values.
        let entries = vec![
            ("h.boxes.virtues.com".to_string(), "value-apex".to_string()),
            ("h.boxes.virtues.com".to_string(), "value-wildcard".to_string()),
            ("other.example".to_string(), "value-other".to_string()),
        ];
        let grouped = group_txt_values(&entries);

        // Two distinct TXT names, not three: apex+wildcard collapsed.
        assert_eq!(grouped.len(), 2);
        // Both values published together under the one shared name (order
        // preserved) — so the authority sets the full RRset and neither
        // authorization's value clobbers the other.
        assert_eq!(
            grouped["_acme-challenge.h.boxes.virtues.com"],
            vec!["value-apex".to_string(), "value-wildcard".to_string()]
        );
        assert_eq!(
            grouped["_acme-challenge.other.example"],
            vec!["value-other".to_string()]
        );
    }
}

#[async_trait::async_trait]
impl DnsPublisher for HttpDnsPublisher {
    async fn publish_txt(&self, name: &str, values: &[String]) -> Result<()> {
        let resp = self
            .client
            .post(&self.url)
            .bearer_auth(&self.token)
            .json(&serde_json::json!({ "name": name, "values": values }))
            .send()
            .await
            .context("POST TXT record")?;
        if !resp.status().is_success() {
            anyhow::bail!("authority rejected TXT publish: {}", resp.status());
        }
        Ok(())
    }
}

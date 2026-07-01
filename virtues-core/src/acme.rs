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
    Account, AccountCredentials, AuthorizationStatus, ChallengeType, Identifier, NewAccount,
    NewOrder, OrderStatus, RetryPolicy,
};

/// Filename (under `cert_dir`) for the cached ACME account credentials. Holds the
/// account key — reused across issuances so we don't register a new account every
/// time (Let's Encrypt rate-limits *account creation*, and a stable account is the
/// prerequisite for ARI-driven renewals).
const ACCOUNT_FILE: &str = "account.json";

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
    /// Build from env for the resolved box `sni`, or `None` if ACME isn't
    /// configured (`VIRTUES_ACME_DIRECTORY` unset) or `sni` is empty.
    ///
    /// The certified name is the **resolved** relay SNI passed in — NOT read from
    /// env. An atlas-provisioned box receives its SNI in `box_secrets`, so
    /// `VIRTUES_RELAY_SNI` is typically unset in its environment; reading it here
    /// would leave every provisioned box stuck on the self-signed bootstrap.
    pub fn from_env(sni: &str) -> Option<Self> {
        let directory_url = std::env::var("VIRTUES_ACME_DIRECTORY").ok().filter(|s| !s.is_empty())?;
        if sni.is_empty() {
            return None;
        }
        // v1 is **apex-only** (just `<boxhash>.virtues.ch`). No wildcard: its only
        // purpose was the LAN dashed-IP name, and LAN-direct is moving to
        // WebTransport + self-signed-cert-by-hash (no public cert needed locally) —
        // see docs/relay-control-plane.md "Path selection". Apex-only also halves
        // the DNS-01 authorizations and sidesteps the wildcard rate-limit class.
        let names = vec![sni.to_string()];
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

    // Reuse the cached account if we have one; otherwise register once and cache
    // the credentials. LE rate-limits *account creation*, and a stable account is
    // also the anchor for ARI-driven renewals — so we never want a fresh account
    // per issuance. A cached account that fails to load (corrupt/rotated key) falls
    // through to a fresh registration rather than wedging issuance.
    let account = match load_account(&cfg.cert_dir).await {
        Some(creds) => match Account::builder()
            .context("build ACME client")?
            .from_credentials(creds)
            .await
        {
            Ok(a) => {
                tracing::debug!("reusing cached ACME account");
                a
            }
            Err(e) => {
                tracing::warn!(error = %e, "cached ACME account unusable; registering a new one");
                register_account(cfg, &contact_refs).await?
            }
        },
        None => register_account(cfg, &contact_refs).await?,
    };

    let identifiers: Vec<Identifier> = cfg.names.iter().cloned().map(Identifier::Dns).collect();
    let mut order = account
        .new_order(&NewOrder::new(&identifiers))
        .await
        .context("create ACME order")?;

    // Publish each pending authorization's DNS-01 TXT, wait for propagation, then
    // mark it ready. v1 is apex-only (one identifier → one authorization); the loop
    // still handles multiple. `Authorizations` is a stream, not an iterator — drive
    // it with `.next().await`. The borrow is scoped so `order` is free to poll after.
    {
        let mut authorizations = order.authorizations();
        while let Some(result) = authorizations.next().await {
            let mut authz = result.context("fetch authorization")?;
            match authz.status {
                AuthorizationStatus::Pending => {}
                AuthorizationStatus::Valid => continue,
                other => anyhow::bail!("unexpected authorization status: {other:?}"),
            }
            let mut challenge = authz
                .challenge(ChallengeType::Dns01)
                .ok_or_else(|| anyhow::anyhow!("no dns-01 challenge offered"))?;
            let name = format!("_acme-challenge.{}", challenge.identifier());
            let value = challenge.key_authorization().dns_value();
            publisher
                .publish_txt(&name, &[value])
                .await
                .with_context(|| format!("publish TXT for {name}"))?;
            // The atlas writer already blocks until the record is INSYNC in Route 53;
            // this extra slack covers resolver caching before the CA validates.
            tokio::time::sleep(Duration::from_secs(cfg.propagation_secs)).await;
            challenge.set_ready().await.context("set challenge ready")?;
        }
    }

    // Poll to Ready (honors Retry-After + backoff), finalize (instant-acme's rcgen
    // generates the cert keypair **on the box** and returns the private key PEM —
    // the key never leaves the box), then download the chain.
    let status = order
        .poll_ready(&RetryPolicy::default())
        .await
        .context("poll ACME order ready")?;
    if status != OrderStatus::Ready {
        anyhow::bail!("ACME order not ready (status: {status:?})");
    }
    let key_pem = order.finalize().await.context("finalize ACME order")?;
    let cert_pem = order
        .poll_certificate(&RetryPolicy::default())
        .await
        .context("download certificate")?;

    Ok(CertMaterial { cert_pem, key_pem })
}

/// Register a fresh ACME account and cache its credentials under `cert_dir` for
/// reuse on the next issuance. A failure to cache is non-fatal (we just re-register
/// next time) — but logged, because repeated re-registration risks LE's account
/// rate limit.
async fn register_account(cfg: &AcmeConfig, contact_refs: &[&str]) -> Result<Account> {
    let (account, credentials) = Account::builder()
        .context("build ACME client")?
        .create(
            &NewAccount {
                contact: contact_refs,
                terms_of_service_agreed: true,
                only_return_existing: false,
            },
            cfg.directory_url.clone(),
            None,
        )
        .await
        .context("create ACME account")?;
    if let Err(e) = save_account(&cfg.cert_dir, &credentials).await {
        tracing::warn!(error = %e, "failed to cache ACME account (will re-register next issuance)");
    }
    Ok(account)
}

/// Load cached ACME account credentials, if present and parseable.
async fn load_account(dir: &Path) -> Option<AccountCredentials> {
    let raw = tokio::fs::read_to_string(dir.join(ACCOUNT_FILE)).await.ok()?;
    serde_json::from_str(&raw).ok()
}

/// Persist ACME account credentials (contains the account key — same sensitivity
/// as `key.pem`, same dir).
async fn save_account(dir: &Path, credentials: &AccountCredentials) -> Result<()> {
    tokio::fs::create_dir_all(dir).await?;
    let json = serde_json::to_string(credentials).context("serialize ACME account")?;
    tokio::fs::write(dir.join(ACCOUNT_FILE), json).await?;
    Ok(())
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

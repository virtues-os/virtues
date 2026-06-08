//! Atlas hydration — RAM cache of per-user tier + subscription state.
//!
//! All paid calls charge the DB entitlement wallet (`entitlement.rs`); there is
//! no RAM budget metering. This type's sole job is the Atlas sync that populates
//! the `TierManager` + `SubscriptionManager` on startup and on a periodic
//! re-hydrate, plus a hydrated-user count for the health probe.

use dashmap::DashMap;
use serde::Deserialize;
use std::sync::Arc;
use tokio::time::{interval, Duration};

use crate::config::Config;
use crate::subscription::SubscriptionManager;
use crate::tier::TierManager;

/// Per-user tier/subscription snapshot from Atlas.
#[derive(Debug, Deserialize)]
pub struct AtlasUserState {
    pub user_id: String,
    pub tier: Option<String>,
    /// Subscription status: "active", "trialing", "past_due", "canceled", "unpaid"
    pub subscription_status: Option<String>,
    /// Trial expiry as ISO-8601 string (e.g. "2026-03-07T00:00:00Z")
    pub trial_expires_at: Option<String>,
}

/// Hydrates per-user tier + subscription state from Atlas into RAM.
#[derive(Clone)]
pub struct AtlasHydrator {
    /// Set of hydrated user ids — backs the health readiness count.
    hydrated: Arc<DashMap<String, ()>>,
    http_client: reqwest::Client,
    atlas_url: Option<String>,
    atlas_secret: Option<String>,
    /// Populated during hydration; read by `/v1/limits/*`.
    tier_manager: TierManager,
    /// Populated during hydration; read by `/v1/subscription`.
    subscription_manager: SubscriptionManager,
    /// Tenant subdomain for Atlas API calls.
    subdomain: Option<String>,
}

impl AtlasHydrator {
    /// Create the hydrator. If Atlas is configured, hydrates tiers +
    /// subscriptions from Atlas on startup; otherwise runs standalone.
    pub async fn new(
        config: &Config,
        tier_manager: &TierManager,
        subscription_manager: &SubscriptionManager,
    ) -> anyhow::Result<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        let manager = Self {
            hydrated: Arc::new(DashMap::new()),
            http_client,
            atlas_url: config.atlas_url.clone(),
            atlas_secret: config.atlas_secret.clone(),
            tier_manager: tier_manager.clone(),
            subscription_manager: subscription_manager.clone(),
            subdomain: config.subdomain.clone(),
        };

        if config.has_atlas() {
            match manager.hydrate_from_atlas().await {
                Ok(count) => {
                    tracing::info!("Hydrated {} user tiers/subscriptions from Atlas", count);
                }
                Err(e) => {
                    tracing::warn!("Failed to hydrate from Atlas, running standalone: {}", e);
                }
            }
        } else {
            tracing::info!("Running in standalone mode (VIRTUES_ATLAS_URL not set)");
        }

        Ok(manager)
    }

    /// Fetch the per-user tier/subscription snapshot from Atlas and populate
    /// the tier + subscription managers.
    async fn hydrate_from_atlas(&self) -> anyhow::Result<usize> {
        let url = self
            .atlas_url
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Atlas URL not configured"))?;
        let secret = self
            .atlas_secret
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Atlas secret not configured"))?;

        let hydration_url = match &self.subdomain {
            Some(sub) => format!("{}/api/internal/budgets?subdomain={}", url, sub),
            None => format!("{}/api/internal/budgets", url),
        };

        let response = self
            .http_client
            .get(&hydration_url)
            .header("X-Atlas-Secret", secret)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Atlas API error ({}): {}", status, body);
        }

        let users: Vec<AtlasUserState> = response.json().await?;
        let count = users.len();

        for user in users {
            self.hydrated.insert(user.user_id.clone(), ());
            if let Some(tier) = &user.tier {
                self.tier_manager.set_tier(&user.user_id, tier);
            }
            if let Some(status) = &user.subscription_status {
                let trial_ts = user
                    .trial_expires_at
                    .as_deref()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.timestamp())
                    .unwrap_or(0);
                self.subscription_manager
                    .set(&user.user_id, status, trial_ts);
            }
        }

        Ok(count)
    }

    /// Number of users hydrated from Atlas (health readiness signal).
    pub fn hydrated_count(&self) -> usize {
        self.hydrated.len()
    }

    /// Periodic re-hydration: catches tier/subscription changes (trial
    /// expirations, cancellations, plan upgrades) while the process runs.
    pub async fn run_rehydrator(&self, interval_secs: u64) {
        if self.atlas_url.is_none() {
            tracing::debug!("Atlas not configured, re-hydration disabled");
            return;
        }

        tracing::info!("Atlas re-hydration started (interval: {}s)", interval_secs);
        let mut tick = interval(Duration::from_secs(interval_secs));

        loop {
            tick.tick().await;
            match self.hydrate_from_atlas().await {
                Ok(count) => {
                    tracing::info!("Re-hydrated {} user tiers/subscriptions from Atlas", count);
                }
                Err(e) => {
                    tracing::warn!("Re-hydration from Atlas failed (will retry): {}", e);
                }
            }
        }
    }
}

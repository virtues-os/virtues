//! Budget Management - RAM-Only with Atlas Integration
//!
//! Key design principles:
//! 1. Check in RAM (0ms latency) - DashMap with atomic floats
//! 2. Hydrate from Atlas on startup (if configured)
//! 3. Report usage to Atlas periodically (if configured)
//! 4. Works standalone with default budgets when Atlas is not available
//!
//! Two modes:
//! - Standalone: Default budget for all users, usage tracking in RAM only
//! - Production: Hydrate from Atlas, report usage back to Atlas

use dashmap::DashMap;
use portable_atomic::{AtomicF64, Ordering};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::time::{interval, Duration};

use crate::config::Config;
use crate::subscription::SubscriptionManager;
use crate::tier::TierManager;
use crate::version::{VersionCache, VersionInfo};

/// Thread-safe budget entry with atomic balance
pub struct BudgetEntry {
    /// Current balance in USD (can go negative, but requests are blocked at 0)
    pub balance: AtomicF64,
    /// Usage delta since last report (negative values = spending)
    pub delta: AtomicF64,
}

impl BudgetEntry {
    pub fn new(balance: f64) -> Self {
        Self {
            balance: AtomicF64::new(balance),
            delta: AtomicF64::new(0.0),
        }
    }
}

/// User budget from Atlas API
#[derive(Debug, Deserialize)]
pub struct AtlasBudget {
    pub user_id: String,
    pub balance_usd: f64,
    pub tier: Option<String>,
    /// Subscription status: "active", "trialing", "past_due", "canceled", "unpaid"
    pub subscription_status: Option<String>,
    /// Trial expiry as ISO-8601 string (e.g. "2026-03-07T00:00:00Z")
    pub trial_expires_at: Option<String>,
}

/// Usage report to send to Atlas
#[derive(Debug, Serialize)]
struct UsageReport {
    user_id: String,
    tokens_used: u64,
    cost_usd: f64,
}

/// Response from Atlas usage reporting endpoint
/// May include latest_version info for pull-based updates
#[derive(Debug, Deserialize)]
struct UsageReportResponse {
    /// Number of usage records Atlas successfully recorded
    recorded: u64,
    /// Total number of usage records in the request
    total: u64,
    /// Latest available version info (piggybacked on usage response)
    latest_version: Option<VersionInfo>,
}

/// Budget manager with in-memory cache and optional Atlas sync
#[derive(Clone)]
pub struct BudgetManager {
    /// User ID -> Budget entry (lock-free concurrent access)
    budgets: Arc<DashMap<String, Arc<BudgetEntry>>>,
    /// Default budget for new/unknown users
    default_budget: f64,
    /// HTTP client for Atlas API calls
    http_client: reqwest::Client,
    /// Atlas URL (if configured)
    atlas_url: Option<String>,
    /// Atlas secret (if configured)
    atlas_secret: Option<String>,
    /// Reference to tier manager for populating tiers during hydration
    tier_manager: TierManager,
    /// Reference to subscription manager for populating subscriptions during hydration
    subscription_manager: SubscriptionManager,
    /// Subdomain identifying this tenant (for Atlas API calls)
    subdomain: Option<String>,
    /// Shared version cache (updated from Atlas usage report responses)
    version_cache: VersionCache,
}

impl BudgetManager {
    /// Create a new budget manager
    /// If Atlas is configured, hydrates budgets, tiers, and subscriptions from Atlas on startup
    pub async fn new(
        config: &Config,
        tier_manager: &TierManager,
        subscription_manager: &SubscriptionManager,
        version_cache: VersionCache,
    ) -> anyhow::Result<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        let manager = Self {
            budgets: Arc::new(DashMap::new()),
            default_budget: config.default_budget_usd,
            http_client,
            atlas_url: config.atlas_url.clone(),
            atlas_secret: config.atlas_secret.clone(),
            tier_manager: tier_manager.clone(),
            subscription_manager: subscription_manager.clone(),
            subdomain: config.subdomain.clone(),
            version_cache,
        };

        // Hydrate from Atlas if configured
        if config.has_atlas() {
            match manager.hydrate_from_atlas().await {
                Ok(count) => {
                    tracing::info!("Hydrated {} user budgets/tiers from Atlas", count);
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to hydrate from Atlas, running in standalone mode: {}",
                        e
                    );
                }
            }
        } else {
            tracing::info!(
                "Running in standalone mode (ATLAS_URL not set). Default budget: ${:.2}",
                config.default_budget_usd
            );
        }

        Ok(manager)
    }

    /// Hydrate budgets from Atlas on startup
    async fn hydrate_from_atlas(&self) -> anyhow::Result<usize> {
        let url = self.atlas_url.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Atlas URL not configured")
        })?;
        let secret = self.atlas_secret.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Atlas secret not configured")
        })?;

        // Build the hydration URL — include subdomain if configured
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

        let budgets: Vec<AtlasBudget> = response.json().await?;
        let count = budgets.len();

        for budget in budgets {
            // Store budget
            self.budgets.insert(
                budget.user_id.clone(),
                Arc::new(BudgetEntry::new(budget.balance_usd)),
            );
            // Store tier (if provided)
            if let Some(tier) = &budget.tier {
                self.tier_manager.set_tier(&budget.user_id, tier);
            }
            // Store subscription status (if provided)
            if let Some(status) = &budget.subscription_status {
                // Parse ISO-8601 trial_expires_at to unix timestamp
                let trial_ts = budget
                    .trial_expires_at
                    .as_deref()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.timestamp())
                    .unwrap_or(0);

                self.subscription_manager.set(
                    &budget.user_id,
                    status,
                    trial_ts,
                );
            }
        }

        Ok(count)
    }

    /// Check if user has sufficient budget (returns false if balance <= 0)
    pub fn has_budget(&self, user_id: &str) -> bool {
        match self.budgets.get(user_id) {
            Some(entry) => entry.balance.load(Ordering::Relaxed) > 0.0,
            None => {
                // Unknown user gets default budget (lazy initialization)
                self.budgets.insert(
                    user_id.to_string(),
                    Arc::new(BudgetEntry::new(self.default_budget)),
                );
                true
            }
        }
    }

    /// Get current balance for a user
    pub fn get_balance(&self, user_id: &str) -> f64 {
        match self.budgets.get(user_id) {
            Some(entry) => entry.balance.load(Ordering::Relaxed),
            None => self.default_budget,
        }
    }

    /// Deduct cost from user's budget (called after successful API request)
    /// This is lock-free and instant.
    pub fn deduct(&self, user_id: &str, cost_usd: f64) {
        if let Some(entry) = self.budgets.get(user_id) {
            // Atomically subtract from balance
            entry.balance.fetch_sub(cost_usd, Ordering::Relaxed);
            // Track delta for usage reporting
            entry.delta.fetch_sub(cost_usd, Ordering::Relaxed);
        }
    }

    /// Get the number of budgets loaded in RAM
    pub fn budgets_count(&self) -> usize {
        self.budgets.len()
    }

    /// Add credit to user's budget (e.g., from Atlas webhook after payment)
    pub fn credit(&self, user_id: &str, amount_usd: f64) {
        match self.budgets.get(user_id) {
            Some(entry) => {
                entry.balance.fetch_add(amount_usd, Ordering::Relaxed);
                entry.delta.fetch_add(amount_usd, Ordering::Relaxed);
            }
            None => {
                self.budgets.insert(
                    user_id.to_string(),
                    Arc::new(BudgetEntry::new(self.default_budget + amount_usd)),
                );
            }
        }
    }

    /// Set user's budget to a specific amount (from Atlas webhook)
    pub fn set_budget(&self, user_id: &str, balance_usd: f64) {
        match self.budgets.get(user_id) {
            Some(entry) => {
                entry.balance.store(balance_usd, Ordering::Relaxed);
                // Reset delta since we're syncing from Atlas
                entry.delta.store(0.0, Ordering::Relaxed);
            }
            None => {
                self.budgets.insert(
                    user_id.to_string(),
                    Arc::new(BudgetEntry::new(balance_usd)),
                );
            }
        }
        tracing::debug!("Set budget for {}: ${:.4}", user_id, balance_usd);
    }

    /// Run periodic re-hydration from Atlas (call this in a background task)
    /// Re-syncs budgets, tiers, and subscription statuses to catch changes
    /// that happen while Tollbooth is running: trial expirations, cancellations,
    /// plan upgrades, balance top-ups, etc.
    pub async fn run_rehydrator(&self, interval_secs: u64) {
        if self.atlas_url.is_none() {
            tracing::debug!("Atlas not configured, re-hydration disabled");
            return;
        }

        tracing::info!(
            "Budget re-hydration started (interval: {}s)",
            interval_secs
        );

        let mut tick = interval(Duration::from_secs(interval_secs));

        loop {
            tick.tick().await;

            match self.hydrate_from_atlas().await {
                Ok(count) => {
                    tracing::info!("Re-hydrated {} user budgets/tiers/subscriptions from Atlas", count);
                }
                Err(e) => {
                    tracing::warn!("Re-hydration from Atlas failed (will retry): {}", e);
                }
            }
        }
    }

    /// Run the usage reporter (call this in a background task)
    /// Reports accumulated usage to Atlas periodically
    pub async fn run_reporter(&self, interval_secs: u64) {
        // Only run if Atlas is configured
        if self.atlas_url.is_none() {
            tracing::debug!("Atlas not configured, usage reporter disabled");
            return;
        }

        let mut tick = interval(Duration::from_secs(interval_secs));

        loop {
            tick.tick().await;

            if let Err(e) = self.report_usage_to_atlas().await {
                tracing::error!("Usage report to Atlas failed (will retry): {}", e);
                // Don't panic - keep running
            }
        }
    }

    /// Report accumulated usage to Atlas
    async fn report_usage_to_atlas(&self) -> anyhow::Result<()> {
        let url = match &self.atlas_url {
            Some(u) => u,
            None => return Ok(()), // No-op if Atlas not configured
        };
        let secret = match &self.atlas_secret {
            Some(s) => s,
            None => return Ok(()),
        };

        // Collect deltas and reset them atomically
        let mut reports: Vec<UsageReport> = Vec::new();

        for entry in self.budgets.iter() {
            let delta = entry.value().delta.swap(0.0, Ordering::Relaxed);
            if delta.abs() > 0.001 {
                // Only report meaningful changes (negative delta = cost)
                reports.push(UsageReport {
                    user_id: entry.key().clone(),
                    tokens_used: 0, // TODO: Track tokens separately
                    cost_usd: -delta, // Convert to positive cost
                });
            }
        }

        if reports.is_empty() {
            return Ok(());
        }

        tracing::debug!("Reporting {} usage records to Atlas", reports.len());

        let response = self
            .http_client
            .post(format!("{}/api/internal/usage", url))
            .header("X-Atlas-Secret", secret)
            .json(&reports)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            // Put the deltas back if reporting failed
            for report in reports {
                if let Some(entry) = self.budgets.get(&report.user_id) {
                    entry.delta.fetch_sub(report.cost_usd, Ordering::Relaxed);
                }
            }
            anyhow::bail!("Atlas API error ({}): {}", status, body);
        }

        // Parse response — may contain latest_version info for pull-based updates
        let body = response.text().await.unwrap_or_default();
        if !body.is_empty() {
            match serde_json::from_str::<UsageReportResponse>(&body) {
                Ok(resp) => {
                    tracing::info!(
                        "Reported {} usage records to Atlas (recorded: {}/{})",
                        reports.len(),
                        resp.recorded,
                        resp.total
                    );
                    if let Some(version_info) = resp.latest_version {
                        tracing::debug!(
                            "Atlas reports latest version: {}",
                            version_info.version
                        );
                        self.version_cache.set(version_info).await;
                    }
                }
                Err(e) => {
                    // Non-fatal: log what we can and continue
                    tracing::info!("Reported {} usage records to Atlas", reports.len());
                    tracing::trace!("Could not parse usage response: {}", e);
                }
            }
        } else {
            tracing::info!("Reported {} usage records to Atlas", reports.len());
        }
        Ok(())
    }
}

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
}

/// Usage report to send to Atlas
#[derive(Debug, Serialize)]
struct UsageReport {
    user_id: String,
    tokens_used: u64,
    cost_usd: f64,
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
}

impl BudgetManager {
    /// Create a new budget manager
    /// If Atlas is configured, hydrates budgets from Atlas on startup
    pub async fn new(config: &Config) -> anyhow::Result<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        let manager = Self {
            budgets: Arc::new(DashMap::new()),
            default_budget: config.default_budget_usd,
            http_client,
            atlas_url: config.atlas_url.clone(),
            atlas_secret: config.atlas_secret.clone(),
        };

        // Hydrate from Atlas if configured
        if config.has_atlas() {
            match manager.hydrate_from_atlas().await {
                Ok(count) => {
                    tracing::info!("Hydrated {} user budgets from Atlas", count);
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

        let response = self
            .http_client
            .get(format!("{}/internal/active-budgets", url))
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
            self.budgets.insert(
                budget.user_id,
                Arc::new(BudgetEntry::new(budget.balance_usd)),
            );
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
            .post(format!("{}/internal/usage-report", url))
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

        tracing::info!("Reported {} usage records to Atlas", reports.len());
        Ok(())
    }
}

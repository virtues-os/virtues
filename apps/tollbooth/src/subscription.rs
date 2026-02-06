//! Subscription Management - In-Memory Subscription State
//!
//! Tracks user subscription status for enforcement.
//! Hydrated from Atlas alongside budgets and tiers.
//!
//! Status lifecycle:
//! - active: Paid subscriber, full access
//! - trialing: Within trial period, full access
//! - past_due: Payment failed, grace period
//! - expired: Trial/subscription ended, AI blocked

use dashmap::DashMap;
use portable_atomic::{AtomicI64, AtomicU8, Ordering};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Subscription status codes (stored as AtomicU8)
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionStatusCode {
    Active = 0,
    Trialing = 1,
    PastDue = 2,
    Expired = 3,
}

impl SubscriptionStatusCode {
    pub fn from_u8(val: u8) -> Self {
        match val {
            0 => Self::Active,
            1 => Self::Trialing,
            2 => Self::PastDue,
            3 => Self::Expired,
            _ => Self::Expired, // Unknown = expired (safe default)
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "active" => Self::Active,
            "trialing" => Self::Trialing,
            "past_due" => Self::PastDue,
            "canceled" | "expired" => Self::Expired,
            "unpaid" => Self::Expired,
            _ => Self::Active, // Unknown from Atlas = active (don't block)
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Trialing => "trialing",
            Self::PastDue => "past_due",
            Self::Expired => "expired",
        }
    }

    /// Whether this status allows AI access
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active | Self::Trialing | Self::PastDue)
    }
}

impl std::fmt::Display for SubscriptionStatusCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Thread-safe subscription entry with atomic fields
pub struct SubscriptionEntry {
    /// Subscription status
    pub status: AtomicU8,
    /// Trial expiry as unix timestamp (0 = no trial)
    pub trial_expires_at: AtomicI64,
}

impl SubscriptionEntry {
    pub fn new(status: SubscriptionStatusCode, trial_expires_at: i64) -> Self {
        Self {
            status: AtomicU8::new(status as u8),
            trial_expires_at: AtomicI64::new(trial_expires_at),
        }
    }
}

/// Response for subscription status queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionStatus {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trial_expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days_remaining: Option<i64>,
    pub is_active: bool,
}

/// Subscription manager with in-memory cache
#[derive(Clone)]
pub struct SubscriptionManager {
    /// User ID -> Subscription entry (lock-free concurrent access)
    subscriptions: Arc<DashMap<String, Arc<SubscriptionEntry>>>,
}

impl SubscriptionManager {
    /// Create a new empty subscription manager
    pub fn new() -> Self {
        Self {
            subscriptions: Arc::new(DashMap::new()),
        }
    }

    /// Set subscription status for a user (called during hydration)
    pub fn set(&self, user_id: &str, status: &str, trial_expires_at: i64) {
        let code = SubscriptionStatusCode::from_str(status);

        match self.subscriptions.get(user_id) {
            Some(entry) => {
                entry.status.store(code as u8, Ordering::Relaxed);
                entry
                    .trial_expires_at
                    .store(trial_expires_at, Ordering::Relaxed);
            }
            None => {
                self.subscriptions.insert(
                    user_id.to_string(),
                    Arc::new(SubscriptionEntry::new(code, trial_expires_at)),
                );
            }
        }

        tracing::debug!(
            "Set subscription for {}: status={}, trial_expires_at={}",
            user_id,
            code.as_str(),
            trial_expires_at
        );
    }

    /// Check if user has an active subscription (allows AI access)
    /// Unknown users are treated as active (standalone mode / not yet hydrated)
    pub fn is_active(&self, user_id: &str) -> bool {
        match self.subscriptions.get(user_id) {
            Some(entry) => {
                let code = SubscriptionStatusCode::from_u8(entry.status.load(Ordering::Relaxed));
                code.is_active()
            }
            None => true, // Unknown user = active (standalone mode)
        }
    }

    /// Get full subscription status for a user
    pub fn get_status(&self, user_id: &str) -> SubscriptionStatus {
        match self.subscriptions.get(user_id) {
            Some(entry) => {
                let code = SubscriptionStatusCode::from_u8(entry.status.load(Ordering::Relaxed));
                let trial_ts = entry.trial_expires_at.load(Ordering::Relaxed);

                let (trial_expires_at, days_remaining) = if trial_ts > 0 {
                    let expires =
                        chrono::DateTime::from_timestamp(trial_ts, 0).map(|dt| dt.to_rfc3339());
                    let now = chrono::Utc::now().timestamp();
                    let days = ((trial_ts - now) as f64 / 86400.0).ceil() as i64;
                    (expires, Some(days.max(0)))
                } else {
                    (None, None)
                };

                SubscriptionStatus {
                    status: code.as_str().to_string(),
                    trial_expires_at,
                    days_remaining,
                    is_active: code.is_active(),
                }
            }
            None => {
                // Unknown user = active (standalone mode)
                SubscriptionStatus {
                    status: "active".to_string(),
                    trial_expires_at: None,
                    days_remaining: None,
                    is_active: true,
                }
            }
        }
    }

    /// Get the number of entries in memory
    pub fn entries_count(&self) -> usize {
        self.subscriptions.len()
    }
}

impl Default for SubscriptionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_code_from_str() {
        assert_eq!(SubscriptionStatusCode::from_str("active"), SubscriptionStatusCode::Active);
        assert_eq!(SubscriptionStatusCode::from_str("trialing"), SubscriptionStatusCode::Trialing);
        assert_eq!(SubscriptionStatusCode::from_str("past_due"), SubscriptionStatusCode::PastDue);
        assert_eq!(SubscriptionStatusCode::from_str("expired"), SubscriptionStatusCode::Expired);
        // Atlas sends "canceled" for cancelled subscriptions
        assert_eq!(SubscriptionStatusCode::from_str("canceled"), SubscriptionStatusCode::Expired);
        // Atlas sends "unpaid" when payment fails repeatedly
        assert_eq!(SubscriptionStatusCode::from_str("unpaid"), SubscriptionStatusCode::Expired);
        // Unknown defaults to active (safe)
        assert_eq!(SubscriptionStatusCode::from_str("unknown"), SubscriptionStatusCode::Active);
    }

    #[test]
    fn test_is_active() {
        assert!(SubscriptionStatusCode::Active.is_active());
        assert!(SubscriptionStatusCode::Trialing.is_active());
        assert!(SubscriptionStatusCode::PastDue.is_active());
        assert!(!SubscriptionStatusCode::Expired.is_active());
    }

    #[test]
    fn test_subscription_manager_unknown_user() {
        let manager = SubscriptionManager::new();
        // Unknown users default to active (standalone mode)
        assert!(manager.is_active("unknown_user"));
        let status = manager.get_status("unknown_user");
        assert_eq!(status.status, "active");
        assert!(status.is_active);
    }

    #[test]
    fn test_subscription_manager_set_and_check() {
        let manager = SubscriptionManager::new();

        // Active user
        manager.set("user1", "active", 0);
        assert!(manager.is_active("user1"));

        // Trialing user with 30 days remaining
        let future_ts = chrono::Utc::now().timestamp() + 30 * 86400;
        manager.set("user2", "trialing", future_ts);
        assert!(manager.is_active("user2"));
        let status = manager.get_status("user2");
        assert_eq!(status.status, "trialing");
        assert!(status.days_remaining.unwrap() > 0);

        // Expired user
        manager.set("user3", "expired", 0);
        assert!(!manager.is_active("user3"));
        let status = manager.get_status("user3");
        assert_eq!(status.status, "expired");
        assert!(!status.is_active);
    }

    #[test]
    fn test_subscription_manager_update() {
        let manager = SubscriptionManager::new();

        manager.set("user1", "trialing", 1000000);
        assert!(manager.is_active("user1"));

        // Upgrade to expired
        manager.set("user1", "expired", 0);
        assert!(!manager.is_active("user1"));

        // Reactivate
        manager.set("user1", "active", 0);
        assert!(manager.is_active("user1"));
    }

    #[test]
    fn test_entries_count() {
        let manager = SubscriptionManager::new();
        assert_eq!(manager.entries_count(), 0);

        manager.set("user1", "active", 0);
        manager.set("user2", "trialing", 1000);
        assert_eq!(manager.entries_count(), 2);
    }
}

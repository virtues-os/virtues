//! Tier Management - User subscription tiers and connection limits
//!
//! Tracks user subscription tiers and enforces connection limits based on
//! the virtues-registry configuration.
//!
//! Two modes:
//! - Standalone: Default tier (free) for all users
//! - Production: Hydrate tiers from Atlas alongside budgets

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Default tier for unknown users
pub const DEFAULT_TIER: &str = "free";

/// User tier entry
#[derive(Debug, Clone)]
pub struct TierEntry {
    pub tier: String,
}

impl Default for TierEntry {
    fn default() -> Self {
        Self {
            tier: DEFAULT_TIER.to_string(),
        }
    }
}

/// Response for connection limit check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionLimitResponse {
    /// Source type being checked
    pub source: String,
    /// User's tier
    pub tier: String,
    /// Maximum allowed connections for this tier
    pub limit: u8,
    /// Current number of connections (passed by caller)
    pub current: i64,
    /// Whether user can add another connection
    pub can_add: bool,
    /// Remaining connections available
    pub remaining: i64,
}

/// Tier manager with in-memory cache
#[derive(Clone)]
pub struct TierManager {
    /// User ID -> Tier entry (lock-free concurrent access)
    tiers: Arc<DashMap<String, TierEntry>>,
    /// Default tier for new/unknown users
    default_tier: String,
}

impl TierManager {
    /// Create a new tier manager
    pub fn new() -> Self {
        Self {
            tiers: Arc::new(DashMap::new()),
            default_tier: DEFAULT_TIER.to_string(),
        }
    }

    /// Get tier for a user
    pub fn get_tier(&self, user_id: &str) -> String {
        match self.tiers.get(user_id) {
            Some(entry) => entry.tier.clone(),
            None => self.default_tier.clone(),
        }
    }

    /// Set tier for a user (called during hydration from Atlas)
    pub fn set_tier(&self, user_id: &str, tier: &str) {
        let tier_str = if tier.is_empty() {
            self.default_tier.clone()
        } else {
            tier.to_lowercase()
        };

        match self.tiers.get_mut(user_id) {
            Some(mut entry) => {
                entry.tier = tier_str;
            }
            None => {
                self.tiers.insert(
                    user_id.to_string(),
                    TierEntry { tier: tier_str },
                );
            }
        }
    }

    /// Get connection limit for a source at user's tier
    pub fn get_connection_limit(&self, user_id: &str, source_type: &str) -> Option<u8> {
        let tier = self.get_tier(user_id);
        virtues_registry::get_connection_limit(source_type, &tier)
    }

    /// Check if user can add another connection for a source
    pub fn check_connection_limit(
        &self,
        user_id: &str,
        source_type: &str,
        current_count: i64,
    ) -> Option<ConnectionLimitResponse> {
        let tier = self.get_tier(user_id);
        let limit = virtues_registry::get_connection_limit(source_type, &tier)?;
        let can_add = current_count < limit as i64;
        let remaining = (limit as i64 - current_count).max(0);

        Some(ConnectionLimitResponse {
            source: source_type.to_string(),
            tier,
            limit,
            current: current_count,
            can_add,
            remaining,
        })
    }

    /// Check if a source allows multiple instances
    pub fn is_multi_instance(&self, source_type: &str) -> bool {
        virtues_registry::is_multi_instance(source_type)
    }

    /// Get the number of tier entries in memory
    pub fn entries_count(&self) -> usize {
        self.tiers.len()
    }
}

impl Default for TierManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_tier() {
        let manager = TierManager::new();
        assert_eq!(manager.get_tier("unknown_user"), "free");
    }

    #[test]
    fn test_set_tier() {
        let manager = TierManager::new();
        manager.set_tier("user1", "pro");
        assert_eq!(manager.get_tier("user1"), "pro");
    }

    #[test]
    fn test_connection_limit_check() {
        let manager = TierManager::new();
        
        // Free tier user with 1 Plaid connection
        let result = manager.check_connection_limit("user1", "plaid", 1);
        assert!(result.is_some());
        let resp = result.unwrap();
        assert_eq!(resp.tier, "free");
        assert_eq!(resp.limit, 2); // Free tier gets 2 Plaid connections
        assert_eq!(resp.current, 1);
        assert!(resp.can_add);
        assert_eq!(resp.remaining, 1);

        // Free tier user at limit
        let result = manager.check_connection_limit("user1", "plaid", 2);
        let resp = result.unwrap();
        assert!(!resp.can_add);
        assert_eq!(resp.remaining, 0);
    }

    #[test]
    fn test_pro_tier_limits() {
        let manager = TierManager::new();
        manager.set_tier("pro_user", "pro");

        let result = manager.check_connection_limit("pro_user", "plaid", 5);
        let resp = result.unwrap();
        assert_eq!(resp.tier, "pro");
        assert_eq!(resp.limit, 10); // Pro tier gets 10 Plaid connections
        assert!(resp.can_add);
    }

    #[test]
    fn test_singleton_source() {
        let manager = TierManager::new();
        
        // iOS is singleton - limit is always 1
        let result = manager.check_connection_limit("user1", "ios", 0);
        let resp = result.unwrap();
        assert_eq!(resp.limit, 1);
        assert!(resp.can_add);

        let result = manager.check_connection_limit("user1", "ios", 1);
        let resp = result.unwrap();
        assert!(!resp.can_add);
    }
}

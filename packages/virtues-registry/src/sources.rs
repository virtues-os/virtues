//! Source registry - Data source definitions
//!
//! This module defines the metadata for data sources (Google, iOS, Mac, Notion, Plaid).
//! The actual implementation (stream creators, transforms) lives in Core.

use serde::{Deserialize, Serialize};

/// Authentication type required for a source
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthType {
    /// OAuth 2.0 authentication
    OAuth2,
    /// API key authentication
    ApiKey,
    /// Device-based (no external auth needed)
    Device,
    /// No authentication required
    None,
}

/// Tier for a source or stream
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceTier {
    /// Standard tier - default for all users (30-day trial, then paid)
    Standard,
    /// Pro tier - requires pro subscription
    Pro,
}

/// Connection limits per tier for multi-instance sources
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionLimits {
    /// Max connections for standard tier
    pub standard: u8,
    /// Max connections for pro tier
    pub pro: u8,
}

impl Default for ConnectionLimits {
    fn default() -> Self {
        Self {
            standard: 5,
            pro: 10,
        }
    }
}

impl ConnectionLimits {
    /// Create new connection limits
    pub const fn new(standard: u8, pro: u8) -> Self {
        Self { standard, pro }
    }

    /// Get limit for a given tier name
    pub fn for_tier(&self, tier: &str) -> u8 {
        match tier.to_lowercase().as_str() {
            "pro" => self.pro,
            _ => self.standard, // Default to standard tier
        }
    }
}

/// Policy for connecting to a source
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConnectionPolicy {
    /// Only one connection allowed per user (e.g., iOS HealthKit)
    Singleton,
    /// Multiple connections allowed with tier-based limits
    MultiInstance {
        /// Per-tier connection limits
        limits: ConnectionLimits,
    },
}

/// OAuth configuration details for a source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConfig {
    /// OAuth scopes required
    pub scopes: Vec<&'static str>,
    /// Authorization URL
    pub auth_url: &'static str,
    /// Token URL
    pub token_url: &'static str,
}

/// Source descriptor - metadata only, no implementation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceDescriptor {
    /// Unique identifier (e.g., "google", "notion", "ios")
    pub name: &'static str,
    /// Human-readable display name
    pub display_name: &'static str,
    /// Description of what this source provides
    pub description: &'static str,
    /// Authentication type required
    pub auth_type: AuthType,
    /// OAuth-specific configuration (if applicable)
    pub oauth_config: Option<OAuthConfig>,
    /// Iconify icon name for UI display
    pub icon: Option<&'static str>,
    /// Whether this source is enabled
    pub enabled: bool,
    /// Tier required for this source
    pub tier: SourceTier,
    /// Connection policy for this source
    pub connection_policy: ConnectionPolicy,
}

/// Get all registered source descriptors
pub fn registered_sources() -> Vec<SourceDescriptor> {
    vec![
        // Google
        SourceDescriptor {
            name: "google",
            display_name: "Google",
            description: "Sync data from Google Workspace services (Calendar, Gmail, Drive)",
            auth_type: AuthType::OAuth2,
            oauth_config: Some(OAuthConfig {
                scopes: vec!["https://www.googleapis.com/auth/calendar.readonly"],
                auth_url: "https://accounts.google.com/o/oauth2/v2/auth",
                token_url: "https://oauth2.googleapis.com/token",
            }),
            icon: Some("ri:google-fill"),
            enabled: true,
            tier: SourceTier::Standard,
            connection_policy: ConnectionPolicy::MultiInstance {
                limits: ConnectionLimits::new(16, 24),
            },
        },
        // Notion
        SourceDescriptor {
            name: "notion",
            display_name: "Notion",
            description: "Sync pages, databases, and blocks from Notion workspaces",
            auth_type: AuthType::OAuth2,
            oauth_config: Some(OAuthConfig {
                scopes: vec!["read_content"],
                auth_url: "https://api.notion.com/v1/oauth/authorize",
                token_url: "https://api.notion.com/v1/oauth/token",
            }),
            icon: Some("simple-icons:notion"),
            enabled: true,
            tier: SourceTier::Standard,
            connection_policy: ConnectionPolicy::MultiInstance {
                limits: ConnectionLimits::new(5, 20),
            },
        },
        // Plaid (Banking)
        SourceDescriptor {
            name: "plaid",
            display_name: "Plaid",
            description:
                "Connect your bank accounts and credit cards to sync transactions and balances",
            auth_type: AuthType::OAuth2,
            oauth_config: Some(OAuthConfig {
                scopes: vec!["transactions", "auth"],
                auth_url: "https://cdn.plaid.com/link/v2/stable/link.html",
                token_url: "https://production.plaid.com/link/token/exchange",
            }),
            icon: Some("ri:bank-line"),
            enabled: true,
            tier: SourceTier::Standard,
            connection_policy: ConnectionPolicy::MultiInstance {
                limits: ConnectionLimits::new(4, 16),
            },
        },
        // iOS
        SourceDescriptor {
            name: "ios",
            display_name: "iOS",
            description: "Personal data from iOS devices (HealthKit, Location, Microphone)",
            auth_type: AuthType::Device,
            oauth_config: None,
            icon: Some("ri:apple-fill"),
            enabled: true,
            tier: SourceTier::Standard,
            connection_policy: ConnectionPolicy::MultiInstance {
                limits: ConnectionLimits::new(1, 1),
            },
        },
        // macOS
        SourceDescriptor {
            name: "mac",
            display_name: "macOS",
            description:
                "Personal data from macOS devices (App usage, Browser history, iMessage)",
            auth_type: AuthType::Device,
            oauth_config: None,
            icon: Some("ri:macbook-line"),
            enabled: false,
            tier: SourceTier::Standard,
            connection_policy: ConnectionPolicy::Singleton,
        },
    ]
}

/// Get a source descriptor by name
pub fn get_source(name: &str) -> Option<SourceDescriptor> {
    registered_sources().into_iter().find(|s| s.name == name)
}

/// Get the connection limit for a source at a given tier
pub fn get_connection_limit(source_name: &str, tier: &str) -> Option<u8> {
    let source = get_source(source_name)?;
    Some(match source.connection_policy {
        ConnectionPolicy::Singleton => 1,
        ConnectionPolicy::MultiInstance { limits } => limits.for_tier(tier),
    })
}

/// Check if a source allows multiple connections
pub fn is_multi_instance(source_name: &str) -> bool {
    get_source(source_name)
        .map(|s| matches!(s.connection_policy, ConnectionPolicy::MultiInstance { .. }))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registered_sources() {
        let sources = registered_sources();
        assert!(!sources.is_empty());

        // Check we have all expected sources
        let names: Vec<_> = sources.iter().map(|s| s.name).collect();
        assert!(names.contains(&"google"));
        assert!(names.contains(&"ios"));
        assert!(names.contains(&"mac"));
        assert!(names.contains(&"notion"));
        assert!(names.contains(&"plaid"));
    }

    #[test]
    fn test_get_source() {
        let google = get_source("google");
        assert!(google.is_some());
        let g = google.unwrap();
        assert_eq!(g.auth_type, AuthType::OAuth2);
        assert!(g.oauth_config.is_some());
    }

    #[test]
    fn test_auth_types() {
        let sources = registered_sources();

        // OAuth sources
        let oauth_sources: Vec<_> = sources
            .iter()
            .filter(|s| s.auth_type == AuthType::OAuth2)
            .collect();
        assert!(oauth_sources.len() >= 3); // google, notion, plaid

        // Device sources
        let device_sources: Vec<_> = sources
            .iter()
            .filter(|s| s.auth_type == AuthType::Device)
            .collect();
        assert!(device_sources.len() >= 2); // ios, mac
    }

    #[test]
    fn test_connection_limits() {
        // Plaid has tier-based limits
        assert_eq!(get_connection_limit("plaid", "standard"), Some(4));
        assert_eq!(get_connection_limit("plaid", "pro"), Some(16));

        // Google has different limits
        assert_eq!(get_connection_limit("google", "standard"), Some(16));
        assert_eq!(get_connection_limit("google", "pro"), Some(24));

        // iOS allows 1 connection for all tiers
        assert_eq!(get_connection_limit("ios", "standard"), Some(1));
        assert_eq!(get_connection_limit("ios", "pro"), Some(1));

        // Unknown source returns None
        assert_eq!(get_connection_limit("unknown", "standard"), None);
    }

    #[test]
    fn test_is_multi_instance() {
        assert!(is_multi_instance("plaid"));
        assert!(is_multi_instance("google"));
        assert!(is_multi_instance("notion"));
        assert!(is_multi_instance("ios"));
        assert!(!is_multi_instance("mac"));
        assert!(!is_multi_instance("unknown"));
    }

    #[test]
    fn test_connection_limits_default_tier() {
        let limits = ConnectionLimits::default();
        // Unknown tier defaults to standard
        assert_eq!(limits.for_tier("unknown"), limits.standard);
        assert_eq!(limits.for_tier("STANDARD"), limits.standard); // Case insensitive
        assert_eq!(limits.for_tier("PRO"), limits.pro);
    }
}

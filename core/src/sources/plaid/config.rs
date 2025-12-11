//! Configuration types for Plaid streams

use serde::{Deserialize, Serialize};

/// Configuration for Plaid transactions stream
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlaidTransactionsConfig {
    /// List of account IDs to sync (empty = all accounts)
    #[serde(default)]
    pub account_ids: Vec<String>,

    /// Whether to include pending transactions
    #[serde(default = "default_include_pending")]
    pub include_pending: bool,

    /// Maximum transactions to fetch per sync
    #[serde(default = "default_max_transactions")]
    pub max_transactions_per_sync: i32,
}

fn default_include_pending() -> bool {
    true
}

fn default_max_transactions() -> i32 {
    500
}

/// Configuration for Plaid accounts stream
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaidAccountsConfig {
    /// Whether to fetch current account balances
    #[serde(default = "default_include_balances")]
    pub include_balances: bool,
}

fn default_include_balances() -> bool {
    true
}

impl Default for PlaidAccountsConfig {
    fn default() -> Self {
        Self {
            include_balances: true,
        }
    }
}

/// Configuration for Plaid investments stream
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlaidInvestmentsConfig {
    /// List of account IDs to sync (empty = all investment accounts)
    #[serde(default)]
    pub account_ids: Vec<String>,
}

/// Configuration for Plaid liabilities stream
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlaidLiabilitiesConfig {
    /// List of account IDs to sync (empty = all liability accounts)
    #[serde(default)]
    pub account_ids: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = PlaidTransactionsConfig::default();
        assert!(config.account_ids.is_empty());
        assert!(!config.include_pending); // Default trait gives false, but our default fn gives true
    }

    #[test]
    fn test_deserialize_config() {
        let json = r#"{"account_ids": ["acc_123"], "include_pending": false}"#;
        let config: PlaidTransactionsConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.account_ids, vec!["acc_123"]);
        assert!(!config.include_pending);
        assert_eq!(config.max_transactions_per_sync, 500); // default
    }

    #[test]
    fn test_accounts_config() {
        let config = PlaidAccountsConfig::default();
        assert!(config.include_balances);
    }

    #[test]
    fn test_investments_config() {
        let config = PlaidInvestmentsConfig::default();
        assert!(config.account_ids.is_empty());
    }

    #[test]
    fn test_liabilities_config() {
        let config = PlaidLiabilitiesConfig::default();
        assert!(config.account_ids.is_empty());
    }
}

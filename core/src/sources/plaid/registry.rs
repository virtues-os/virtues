//! Plaid source registration for the catalog

use crate::registry::{AuthType, OAuthConfig, RegisteredSource, RegisteredStream, SourceRegistry};
use serde_json::json;

/// Plaid source registration
pub struct PlaidSource;

impl SourceRegistry for PlaidSource {
    fn descriptor() -> RegisteredSource {
        RegisteredSource {
            name: "plaid",
            display_name: "Plaid",
            description: "Connect your bank accounts and credit cards to sync transactions and balances",
            // Plaid uses a custom Link flow rather than standard OAuth2
            // but we model it similarly for the UI
            auth_type: AuthType::OAuth2,
            oauth_config: Some(OAuthConfig {
                scopes: vec!["transactions", "auth"],
                auth_url: "https://cdn.plaid.com/link/v2/stable/link.html",
                token_url: "https://production.plaid.com/link/token/exchange",
            }),
            icon: Some("simple-icons:plaid"),
            streams: vec![
                // Transactions stream
                RegisteredStream::new("transactions")
                    .display_name("Transactions")
                    .description(
                        "Sync bank transactions with merchant and category info",
                    )
                    .table_name("stream_plaid_transactions")
                    .target_ontologies(vec!["financial_transaction"])
                    .config_schema(transactions_config_schema())
                    .config_example(transactions_config_example())
                    .supports_incremental(true)
                    .supports_full_refresh(true)
                    .default_cron_schedule("0 0 */6 * * *") // Every 6 hours
                    .build(),
                // Accounts stream
                RegisteredStream::new("accounts")
                    .display_name("Accounts")
                    .description(
                        "Sync bank accounts, credit cards, and account balances",
                    )
                    .table_name("stream_plaid_accounts")
                    .target_ontologies(vec!["financial_account"])
                    .config_schema(accounts_config_schema())
                    .config_example(accounts_config_example())
                    .supports_incremental(false) // Accounts always fetched in full
                    .supports_full_refresh(true)
                    .default_cron_schedule("0 0 0 * * *") // Daily at midnight
                    .build(),
                // Investments stream
                RegisteredStream::new("investments")
                    .display_name("Investments")
                    .description(
                        "Sync investment holdings, securities, and 401k/IRA/brokerage data",
                    )
                    .table_name("stream_plaid_investments")
                    .target_ontologies(vec!["financial_asset"])
                    .config_schema(investments_config_schema())
                    .config_example(investments_config_example())
                    .supports_incremental(false) // Holdings always fetched in full
                    .supports_full_refresh(true)
                    .default_cron_schedule("0 0 0 * * *") // Daily at midnight
                    .build(),
                // Liabilities stream
                RegisteredStream::new("liabilities")
                    .display_name("Liabilities")
                    .description(
                        "Sync credit card APRs, mortgages, student loans, and loan details",
                    )
                    .table_name("stream_plaid_liabilities")
                    .target_ontologies(vec!["financial_liability"])
                    .config_schema(liabilities_config_schema())
                    .config_example(liabilities_config_example())
                    .supports_incremental(false) // Liabilities always fetched in full
                    .supports_full_refresh(true)
                    .default_cron_schedule("0 0 0 * * *") // Daily at midnight
                    .build(),
            ],
        }
    }
}

/// JSON schema for Plaid transactions configuration
fn transactions_config_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "account_ids": {
                "type": "array",
                "items": { "type": "string" },
                "description": "List of account IDs to sync (leave empty to sync all connected accounts)"
            },
            "include_pending": {
                "type": "boolean",
                "default": true,
                "description": "Include pending transactions"
            },
            "sync_strategy": {
                "type": "object",
                "description": "Strategy for sync operations",
                "default": {
                    "type": "time_window",
                    "days_back": 90
                },
                "oneOf": [
                    {
                        "type": "object",
                        "required": ["type"],
                        "properties": {
                            "type": { "const": "time_window" },
                            "days_back": {
                                "type": "integer",
                                "default": 90,
                                "description": "Number of days of history to sync"
                            }
                        }
                    },
                    {
                        "type": "object",
                        "required": ["type"],
                        "properties": {
                            "type": { "const": "full_history" },
                            "max_records": {
                                "type": "integer",
                                "nullable": true,
                                "description": "Optional limit on number of transactions"
                            }
                        }
                    }
                ]
            },
            "max_transactions_per_sync": {
                "type": "integer",
                "default": 500,
                "minimum": 1,
                "maximum": 500,
                "description": "Maximum number of transactions to fetch per sync (Plaid limit is 500)"
            }
        }
    })
}

/// Example configuration for Plaid transactions
fn transactions_config_example() -> serde_json::Value {
    json!({
        "account_ids": [],
        "include_pending": true,
        "sync_strategy": {
            "type": "time_window",
            "days_back": 90
        },
        "max_transactions_per_sync": 500
    })
}

/// JSON schema for Plaid accounts configuration
fn accounts_config_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "include_balances": {
                "type": "boolean",
                "default": true,
                "description": "Fetch current account balances"
            }
        }
    })
}

/// Example configuration for Plaid accounts
fn accounts_config_example() -> serde_json::Value {
    json!({
        "include_balances": true
    })
}

/// JSON schema for Plaid investments configuration
fn investments_config_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "account_ids": {
                "type": "array",
                "items": { "type": "string" },
                "description": "List of account IDs to sync (leave empty to sync all investment accounts)"
            }
        }
    })
}

/// Example configuration for Plaid investments
fn investments_config_example() -> serde_json::Value {
    json!({
        "account_ids": []
    })
}

/// JSON schema for Plaid liabilities configuration
fn liabilities_config_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "account_ids": {
                "type": "array",
                "items": { "type": "string" },
                "description": "List of account IDs to sync (leave empty to sync all liability accounts)"
            }
        }
    })
}

/// Example configuration for Plaid liabilities
fn liabilities_config_example() -> serde_json::Value {
    json!({
        "account_ids": []
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plaid_descriptor() {
        let desc = PlaidSource::descriptor();
        assert_eq!(desc.name, "plaid");
        assert_eq!(desc.auth_type, AuthType::OAuth2);
        assert!(desc.oauth_config.is_some());
        assert_eq!(desc.streams.len(), 4); // transactions, accounts, investments, liabilities
    }

    #[test]
    fn test_transactions_stream() {
        let desc = PlaidSource::descriptor();
        let transactions = desc.streams.iter().find(|s| s.name == "transactions");
        assert!(transactions.is_some());

        let tx = transactions.unwrap();
        assert_eq!(tx.table_name, "stream_plaid_transactions");
        assert_eq!(tx.target_ontologies, vec!["financial_transaction"]);
        assert!(tx.supports_incremental);
        assert!(tx.supports_full_refresh);
    }

    #[test]
    fn test_accounts_stream() {
        let desc = PlaidSource::descriptor();
        let accounts = desc.streams.iter().find(|s| s.name == "accounts");
        assert!(accounts.is_some());

        let acc = accounts.unwrap();
        assert_eq!(acc.table_name, "stream_plaid_accounts");
        assert_eq!(acc.target_ontologies, vec!["financial_account"]);
        assert!(!acc.supports_incremental);
        assert!(acc.supports_full_refresh);
    }

    #[test]
    fn test_investments_stream() {
        let desc = PlaidSource::descriptor();
        let investments = desc.streams.iter().find(|s| s.name == "investments");
        assert!(investments.is_some());

        let inv = investments.unwrap();
        assert_eq!(inv.table_name, "stream_plaid_investments");
        assert_eq!(inv.target_ontologies, vec!["financial_asset"]);
        assert!(!inv.supports_incremental);
        assert!(inv.supports_full_refresh);
    }

    #[test]
    fn test_liabilities_stream() {
        let desc = PlaidSource::descriptor();
        let liabilities = desc.streams.iter().find(|s| s.name == "liabilities");
        assert!(liabilities.is_some());

        let liab = liabilities.unwrap();
        assert_eq!(liab.table_name, "stream_plaid_liabilities");
        assert_eq!(liab.target_ontologies, vec!["financial_liability"]);
        assert!(!liab.supports_incremental);
        assert!(liab.supports_full_refresh);
    }

    #[test]
    fn test_config_schemas_valid() {
        // Ensure schemas are valid JSON
        let tx_schema = transactions_config_schema();
        assert_eq!(tx_schema["type"], "object");
        assert!(tx_schema["properties"].is_object());

        let acc_schema = accounts_config_schema();
        assert_eq!(acc_schema["type"], "object");

        let inv_schema = investments_config_schema();
        assert_eq!(inv_schema["type"], "object");

        let liab_schema = liabilities_config_schema();
        assert_eq!(liab_schema["type"], "object");
    }
}

//! Plaid source registration for the catalog
//!
//! This module provides the unified registration for Plaid sources, including
//! both UI metadata, transform logic, and stream creation in a single place.

use crate::registry::{RegisteredSource, RegisteredStream, SourceRegistry};
use crate::sources::stream_type::StreamType;
use serde_json::json;

// Import transforms and stream types for unified registration
use super::accounts::{transform::PlaidAccountTransform, PlaidAccountsStream};
use super::transactions::{transform::PlaidTransactionTransform, PlaidTransactionsStream};
// Note: Investment and Liability transforms exist but their target ontologies don't yet

/// Plaid source registration
pub struct PlaidSource;

impl SourceRegistry for PlaidSource {
    fn descriptor() -> RegisteredSource {
        let descriptor = virtues_registry::sources::get_source("plaid")
            .expect("Plaid source not found in virtues-registry");

        RegisteredSource {
            descriptor,
            streams: vec![
                // Transactions stream with unified transform and stream creator
                RegisteredStream::new("transactions")
                    .config_schema(transactions_config_schema())
                    .config_example(transactions_config_example())
                    .transform("financial_transaction", |_ctx| Ok(Box::new(PlaidTransactionTransform)))
                    .stream_creator(|ctx| {
                        let stream = PlaidTransactionsStream::new(
                            ctx.source_id.clone(),
                            ctx.db.clone(),
                            ctx.stream_writer.clone(),
                        )?;
                        Ok(StreamType::Pull(Box::new(stream)))
                    })
                    .build(),
                // Accounts stream with unified transform and stream creator
                RegisteredStream::new("accounts")
                    .config_schema(accounts_config_schema())
                    .config_example(accounts_config_example())
                    .transform("financial_account", |_ctx| Ok(Box::new(PlaidAccountTransform)))
                    .stream_creator(|ctx| {
                        let stream = PlaidAccountsStream::new(
                            ctx.source_id.clone(),
                            ctx.db.clone(),
                            ctx.stream_writer.clone(),
                        )?;
                        Ok(StreamType::Pull(Box::new(stream)))
                    })
                    .build(),
                // Investments stream
                // Note: target_ontologies empty until financial_asset ontology is created
                RegisteredStream::new("investments")
                    .config_schema(investments_config_schema())
                    .config_example(investments_config_example())
                    .build(),
                // Liabilities stream
                // Note: target_ontologies empty until financial_liability ontology is created
                RegisteredStream::new("liabilities")
                    .config_schema(liabilities_config_schema())
                    .config_example(liabilities_config_example())
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
    use crate::registry::AuthType;

    #[test]
    fn test_plaid_descriptor() {
        let desc = PlaidSource::descriptor();
        assert_eq!(desc.descriptor.name, "plaid");
        assert_eq!(desc.descriptor.auth_type, AuthType::OAuth2);
        assert!(desc.descriptor.oauth_config.is_some());
        assert_eq!(desc.streams.len(), 4); // transactions, accounts, investments, liabilities
    }

    #[test]
    fn test_transactions_stream() {
        let desc = PlaidSource::descriptor();
        let transactions = desc.streams.iter().find(|s| s.descriptor.name == "transactions");
        assert!(transactions.is_some());

        let tx = transactions.unwrap();
        assert_eq!(tx.descriptor.table_name, "stream_plaid_transactions");
        assert_eq!(tx.descriptor.target_ontologies, vec!["financial_transaction"]);
        assert!(tx.descriptor.supports_incremental);
        assert!(tx.descriptor.supports_full_refresh);
    }

    #[test]
    fn test_accounts_stream() {
        let desc = PlaidSource::descriptor();
        let accounts = desc.streams.iter().find(|s| s.descriptor.name == "accounts");
        assert!(accounts.is_some());

        let acc = accounts.unwrap();
        assert_eq!(acc.descriptor.table_name, "stream_plaid_accounts");
        assert_eq!(acc.descriptor.target_ontologies, vec!["financial_account"]);
        assert!(!acc.descriptor.supports_incremental);
        assert!(acc.descriptor.supports_full_refresh);
    }

    #[test]
    fn test_investments_stream() {
        let desc = PlaidSource::descriptor();
        let investments = desc.streams.iter().find(|s| s.descriptor.name == "investments");
        assert!(investments.is_some());

        let inv = investments.unwrap();
        assert_eq!(inv.descriptor.table_name, "stream_plaid_investments");
        // target_ontologies empty until financial_asset ontology is created
        assert!(inv.descriptor.target_ontologies.is_empty());
        assert!(!inv.descriptor.supports_incremental);
        assert!(inv.descriptor.supports_full_refresh);
    }

    #[test]
    fn test_liabilities_stream() {
        let desc = PlaidSource::descriptor();
        let liabilities = desc.streams.iter().find(|s| s.descriptor.name == "liabilities");
        assert!(liabilities.is_some());

        let liab = liabilities.unwrap();
        assert_eq!(liab.descriptor.table_name, "stream_plaid_liabilities");
        // target_ontologies empty until financial_liability ontology is created
        assert!(liab.descriptor.target_ontologies.is_empty());
        assert!(!liab.descriptor.supports_incremental);
        assert!(liab.descriptor.supports_full_refresh);
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

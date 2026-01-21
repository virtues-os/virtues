//! Financial transaction ontology
//!
//! Financial transactions from bank accounts and credit cards.
//! Includes merchant info, categorization, and location data.

use crate::ontologies::{Ontology, OntologyBuilder, OntologyDescriptor};

pub struct FinancialTransactionOntology;

impl OntologyDescriptor for FinancialTransactionOntology {
    fn descriptor() -> Ontology {
        OntologyBuilder::new("financial_transaction")
            .display_name("Financial Transactions")
            .description("Bank and credit card transactions from Plaid with merchant and category info")
            .domain("financial")
            .table_name("financial_transaction")
            .source_streams(vec!["stream_plaid_transactions"])
            .timestamp_column("timestamp")
            // Enable semantic search on transactions
            .embedding(
                // Text to embed: merchant name + transaction name + category
                "COALESCE(merchant_name, name) || ' ' || COALESCE(category, '')",
                "transaction",
                Some("COALESCE(merchant_name, name)"), // title_sql
                "COALESCE(merchant_name, name) || ' - $' || ABS(amount)::text || ' on ' || transaction_date::text", // preview_sql
                None, // author_sql
                "timestamp", // timestamp_sql
            )
            .build()
    }
}

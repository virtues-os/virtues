//! Financial account ontology
//!
//! Bank accounts, credit cards, loans, and investment accounts from financial providers.

use crate::ontologies::{Ontology, OntologyBuilder, OntologyDescriptor};

pub struct FinancialAccountOntology;

impl OntologyDescriptor for FinancialAccountOntology {
    fn descriptor() -> Ontology {
        OntologyBuilder::new("financial_account")
            .display_name("Financial Accounts")
            .description("Bank accounts, credit cards, and other financial accounts from Plaid")
            .domain("financial")
            .table_name("financial_account")
            .source_streams(vec!["stream_plaid_accounts", "stream_ios_finance"])
            // Accounts are not time-series events, but we need a timestamp for consistency
            // Using created_at as the "when was this account added" timestamp
            .timestamp_column("created_at")
            .build()
    }
}

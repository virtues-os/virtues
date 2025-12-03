//! Financial domain ontologies
//!
//! Ontologies for financial data from Plaid and other financial providers.
//! Includes accounts (bank accounts, credit cards) and transactions.

pub mod registry;

// Individual ontology modules
pub mod account;
pub mod transaction;

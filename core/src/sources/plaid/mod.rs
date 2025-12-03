//! Plaid source for bank and financial data
//!
//! This module provides integration with Plaid for syncing financial data
//! such as transactions, accounts, and balances.
//!
//! ## Architecture
//!
//! - `client.rs` - Plaid API client wrapper with rate limiting
//! - `transactions/` - Transaction stream and transform
//! - `accounts/` - Account stream and transform
//! - `config.rs` - Stream configuration types
//! - `registry.rs` - Source registration

pub mod client;
pub mod config;
pub mod registry;
pub mod transactions;

pub use client::PlaidClient;

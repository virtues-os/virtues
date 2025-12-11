//! Plaid source for bank and financial data
//!
//! This module provides integration with Plaid for syncing financial data
//! such as transactions, accounts, investments, and liabilities.
//!
//! ## Architecture
//!
//! - `client.rs` - Plaid API client wrapper with rate limiting
//! - `transactions/` - Transaction stream and transform
//! - `accounts/` - Account stream and transform
//! - `investments/` - Investment holdings stream and transform
//! - `liabilities/` - Credit/loan liability stream and transform
//! - `config.rs` - Stream configuration types
//! - `registry.rs` - Source registration

pub mod accounts;
pub mod client;
pub mod config;
pub mod investments;
pub mod liabilities;
pub mod registry;
pub mod transactions;

pub use client::PlaidClient;

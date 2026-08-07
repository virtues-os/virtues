//! Route handlers for virtues-api API proxy
//!
//! All billable API requests are proxied through virtues-api for unified budget enforcement.
//!
//! Routes (all metered calls use bearer-auth + DB entitlement::charge):
//! - /v1/ai/*            - LLM chat / completions / embeddings / models
//! - /v1/exa/*           - Web search
//! - /v1/places/*        - Location autocomplete
//! - /v1/unsplash/*      - Image search
//! - /v1/services/plaid/* - Bank data (keeps the master Plaid secret off the box)
//!
//! Bank connections (Plaid) start through the OAuth proxy (`oauth.rs`,
//! via_proxy); the per-user data syncs run through the `plaid.rs` proxy so the
//! box never holds the master Plaid credential.

pub mod ai;
pub mod bearer_test;
pub mod exa;
pub mod parallel;
pub mod health;
pub mod internal;
pub mod oauth;
pub mod places;
pub mod plaid;
pub mod streaming;
pub mod unsplash;
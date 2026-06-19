//! Route handlers for virtues-api API proxy
//!
//! All billable API requests are proxied through virtues-api for unified budget enforcement.
//!
//! Routes (all metered calls use bearer-auth + DB entitlement::charge):
//! - /v1/ai/*       - LLM chat / completions / embeddings / models
//! - /v1/exa/*      - Web search
//! - /v1/places/*   - Location autocomplete
//! - /v1/unsplash/* - Image search
//!
//! Bank connections (Plaid) run through the OAuth proxy (`oauth.rs`, via_proxy)
//! plus direct-to-Plaid syncs in the action binaries — no proxy route here.

pub mod ai;
pub mod bearer_test;
pub mod exa;
pub mod health;
pub mod internal;
pub mod net_probe;
pub mod oauth;
pub mod places;
pub mod redeem;
pub mod streaming;
pub mod unsplash;
//! Route handlers for virtues-api API proxy
//!
//! Every request that costs Virtues money is proxied through virtues-api. AI
//! (`/v1/ai/*`, including web search) settles against the wallet through
//! `entitlement::settle`. Everything else is included in the subscription and
//! needs only a live wallet (`BearerAuth`), with fair-use limits on Places
//! (per day) and Plaid (banks per account, `crate::banks`).
//!
//! Routes:
//! - /v1/ai/*            - LLM chat / completions / models
//! - /v1/ai/search       - Web search (gateway-executed)
//! - /v1/places/*        - Location autocomplete
//! - /v1/unsplash/*      - Image search
//! - /v1/services/plaid/* - Bank data (keeps the master Plaid secret off the box)
//! - /v1/maps/*          - Map files for the boxes (bearer-gated, never logged)
//! - /v1/push/*          - Calendar change signals a box collects (push.rs)
//! - /v1/google/*, /v1/outlook/* - mail and calendar through verified apps we do not run yet (vendor.rs)
//!
//! Bank connections (Plaid) start through the OAuth proxy (`oauth.rs`,
//! via_proxy); the per-user data syncs run through the `plaid.rs` proxy so the
//! box never holds the master Plaid credential.

pub mod ai;
pub mod backgrounds;
pub mod bearer_test;
pub mod connections;
pub mod search;
pub mod vendor;
pub mod health;
pub mod internal;
pub mod maps;
pub mod oauth;
pub mod places;
pub mod plaid;
pub mod push;
pub mod streaming;
pub mod unsplash;
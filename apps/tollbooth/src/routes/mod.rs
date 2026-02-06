//! Route handlers for Tollbooth API proxy
//!
//! All billable API requests are proxied through Tollbooth for unified budget enforcement.
//!
//! Routes:
//! - /v1/chat/completions - LLM requests (OpenAI, Anthropic, Cerebras)
//! - /v1/services/exa/* - Web search
//! - /v1/services/google/places/* - Location autocomplete
//! - /v1/services/unsplash/* - Image search
//! - /v1/services/plaid/* - Bank account connections
//! - /v1/limits/* - Connection limits and tier info
//! - /v1/budget/check - Pre-flight budget check
//! - /v1/version - Latest available version (pull-based updates)
//! - /v1/update - Trigger rolling update (via Atlas)
//!
//! Budget is checked before requests and deducted after completion.

pub mod chat;
pub mod feedback;
pub mod health;
pub mod limits;
pub mod plaid;
pub mod services;
pub mod streaming;
pub mod subscription;
pub mod version;
//! Route handlers for Tollbooth API proxy
//!
//! All billable API requests are proxied through Tollbooth for unified budget enforcement.
//!
//! Routes:
//! - /v1/chat/completions - LLM requests (OpenAI, Anthropic, Cerebras)
//! - /v1/services/exa/* - Web search
//! - /v1/services/google/places/* - Location autocomplete
//! - /v1/budget/check - Pre-flight budget check
//!
//! Budget is checked before requests and deducted after completion.

pub mod chat;
pub mod health;
pub mod services;
pub mod streaming;

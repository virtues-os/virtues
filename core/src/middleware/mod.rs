//! HTTP middleware for the Virtues server
//!
//! This module provides middleware for:
//! - Authentication via session cookies
//! - Rate limiting

pub mod auth;

pub use auth::{require_auth, AuthUser};

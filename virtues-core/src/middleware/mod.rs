//! HTTP middleware for the Virtues server
//!
//! This module provides middleware for:
//! - Authentication via session cookies
//! - Rate limiting

pub mod auth;
pub mod http;
pub mod rate_limit;
pub mod security;

pub use auth::AuthUser;
pub use http::{client_ip, is_secure_environment, OWNER_USER_ID};

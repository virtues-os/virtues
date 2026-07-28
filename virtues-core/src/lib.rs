//! Virtues - Open Source Personal Data Ecosystem
//!
//! High-performance data pipeline for personal data collection, storage, and analysis.

pub mod action_git_import;
pub mod action_runner;
pub mod action_templates;
pub mod agent;
pub mod api;
pub mod cli;
pub mod client;
pub mod codename;
pub mod credentials;
pub mod crypto;
pub mod database;
pub mod dayline;
pub mod entity_resolution;
pub mod error;
pub mod extraction;
pub mod geo;
pub mod http_client;
pub mod ids;
pub mod inference_report;
pub mod magnet;
pub mod maintenance;
pub mod mcp;
pub mod middleware;
pub mod box_secrets;
pub mod net_check;
pub mod relay;
pub mod scheduler;
pub mod search;
pub mod seeding;
pub mod sessionize;
pub mod server;
pub mod setup;
pub mod storage;
pub mod timezone;
pub mod virtues_api;
pub mod tools;
pub mod types;

// Re-export main types
pub use client::{Virtues, VirtuesBuilder};
pub use error::{Error, Result};
pub use types::Timestamp;

// Re-export Scheduler
pub use scheduler::Scheduler;

// Re-export tools
pub use tools::{get_tool_definitions_for_llm, ToolContext, ToolError, ToolExecutor, ToolResult};

// Re-export library API functions
pub use api::{
    // Credential management (post-Phase-6: pair flows live in source_auth.rs)
    check_pairing_status,
    list_pending_pairings,
    DeviceInfo,
    PairingStatus,
    PendingPairing,
};

// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert_eq!(VERSION, "0.1.0");
    }
}

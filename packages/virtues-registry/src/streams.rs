//! Stream registry - Data stream definitions
//!
//! This module defines the metadata for data streams (calendar, healthkit, etc.).
//! The actual implementation (stream creators, transforms) lives in Core.

use serde::{Deserialize, Serialize};
use crate::sources::SourceTier;

/// Stream descriptor - metadata only, no implementation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamDescriptor {
    /// Stream identifier (e.g., "calendar", "gmail")
    pub name: &'static str,
    /// Parent source name (e.g., "google", "ios")
    pub source: &'static str,
    /// Human-readable display name
    pub display_name: &'static str,
    /// Description of what this stream provides
    pub description: &'static str,
    /// Database table name (e.g., "stream_google_calendar")
    pub table_name: &'static str,
    /// Target ontology tables this stream feeds into
    pub target_ontologies: Vec<&'static str>,
    /// Whether this stream supports incremental sync
    pub supports_incremental: bool,
    /// Whether this stream supports full refresh
    pub supports_full_refresh: bool,
    /// Default cron schedule (6-field: sec min hour day month dow)
    pub default_cron_schedule: Option<&'static str>,
    /// Whether this stream is enabled
    pub enabled: bool,
    /// Tier required for this stream (overrides source tier if higher)
    pub tier: SourceTier,
}

/// Get all registered stream descriptors
pub fn registered_streams() -> Vec<StreamDescriptor> {
    vec![
        // ===== Google Streams =====
        StreamDescriptor {
            name: "calendar",
            source: "google",
            display_name: "Google Calendar",
            description: "Sync calendar events with attendees, locations, and conference details",
            table_name: "stream_google_calendar",
            target_ontologies: vec!["calendar_event"],
            supports_incremental: true,
            supports_full_refresh: true,
            default_cron_schedule: Some("0 */15 * * * *"), // Every 15 minutes
            enabled: true,
            tier: SourceTier::Standard,
        },
        StreamDescriptor {
            name: "gmail",
            source: "google",
            display_name: "Gmail",
            description: "Sync email messages and threads with full metadata",
            table_name: "stream_google_gmail",
            target_ontologies: vec!["communication_email"],
            supports_incremental: true,
            supports_full_refresh: true,
            default_cron_schedule: Some("0 */15 * * * *"), // Every 15 minutes
            enabled: false,
            tier: SourceTier::Standard,
        },
        // ===== iOS Streams =====
        StreamDescriptor {
            name: "healthkit",
            source: "ios",
            display_name: "HealthKit",
            description:
                "Health and fitness metrics including heart rate, steps, sleep, and workouts",
            table_name: "stream_ios_healthkit",
            target_ontologies: vec![
                "health_heart_rate",
                "health_hrv",
                "health_steps",
                "health_sleep",
                "health_workout",
            ],
            supports_incremental: false,
            supports_full_refresh: false, // Push-based
            default_cron_schedule: Some("0 */5 * * * *"), // Every 5 minutes
            enabled: true,
            tier: SourceTier::Standard,
        },
        StreamDescriptor {
            name: "location",
            source: "ios",
            display_name: "Location",
            description: "GPS coordinates, speed, altitude, and activity type",
            table_name: "stream_ios_location",
            target_ontologies: vec!["location_point"],
            supports_incremental: false,
            supports_full_refresh: false, // Push-based
            default_cron_schedule: Some("0 */5 * * * *"),
            enabled: true,
            tier: SourceTier::Standard,
        },
        StreamDescriptor {
            name: "microphone",
            source: "ios",
            display_name: "Microphone",
            description: "Audio levels, transcriptions, and recordings",
            table_name: "stream_ios_microphone",
            target_ontologies: vec!["communication_transcription"],
            supports_incremental: false,
            supports_full_refresh: false,
            default_cron_schedule: Some("0 */5 * * * *"),
            enabled: true,
            tier: SourceTier::Standard,
        },
        StreamDescriptor {
            name: "contacts",
            source: "ios",
            display_name: "Contacts",
            description: "Address book contacts from iOS device",
            table_name: "stream_ios_contacts",
            target_ontologies: vec![],  // Contacts feed wiki_people, not an ontology table
            supports_incremental: false,
            supports_full_refresh: false,
            default_cron_schedule: Some("0 */5 * * * *"),
            enabled: true,
            tier: SourceTier::Standard,
        },
        StreamDescriptor {
            name: "financekit",
            source: "ios",
            display_name: "FinanceKit",
            description: "Financial transactions and accounts from Apple FinanceKit",
            table_name: "stream_ios_financekit",
            target_ontologies: vec!["financial_transaction", "financial_account"],
            supports_incremental: false,
            supports_full_refresh: false, // Push-based
            default_cron_schedule: Some("0 */5 * * * *"),
            enabled: true,
            tier: SourceTier::Standard,
        },
        StreamDescriptor {
            name: "eventkit",
            source: "ios",
            display_name: "EventKit",
            description: "Calendar events and reminders from iOS EventKit",
            table_name: "stream_ios_eventkit",
            target_ontologies: vec!["calendar_event"],
            supports_incremental: false,
            supports_full_refresh: false, // Push-based
            default_cron_schedule: Some("0 */5 * * * *"),
            enabled: true,
            tier: SourceTier::Standard,
        },
        // ===== macOS Streams =====
        StreamDescriptor {
            name: "apps",
            source: "mac",
            display_name: "Application Usage",
            description: "Active applications, window titles, and usage duration",
            table_name: "stream_mac_apps",
            target_ontologies: vec!["activity_app_usage"],
            supports_incremental: false,
            supports_full_refresh: false, // Push-based
            default_cron_schedule: Some("0 */5 * * * *"),
            enabled: true,
            tier: SourceTier::Standard,
        },
        StreamDescriptor {
            name: "browser",
            source: "mac",
            display_name: "Browser History",
            description:
                "URLs visited, page titles, and visit durations from Safari, Chrome, Firefox",
            table_name: "stream_mac_browser",
            target_ontologies: vec!["activity_web_browsing"],
            supports_incremental: false,
            supports_full_refresh: false,
            default_cron_schedule: Some("0 */5 * * * *"),
            enabled: true,
            tier: SourceTier::Standard,
        },
        StreamDescriptor {
            name: "imessage",
            source: "mac",
            display_name: "iMessage",
            description: "Message history including SMS and iMessage conversations",
            table_name: "stream_mac_imessage",
            target_ontologies: vec!["communication_message"],
            supports_incremental: false,
            supports_full_refresh: false,
            default_cron_schedule: Some("0 */5 * * * *"),
            enabled: true,
            tier: SourceTier::Standard,
        },
        // ===== Notion Streams =====
        StreamDescriptor {
            name: "pages",
            source: "notion",
            display_name: "Notion Pages",
            description: "Sync pages and their content from Notion databases and workspaces",
            table_name: "stream_notion_pages",
            target_ontologies: vec!["content_document"],
            supports_incremental: false, // Notion API doesn't provide incremental sync
            supports_full_refresh: true,
            default_cron_schedule: Some("0 0 */12 * * *"), // Every 12 hours
            enabled: true,
            tier: SourceTier::Standard,
        },
        // ===== Plaid Streams =====
        StreamDescriptor {
            name: "transactions",
            source: "plaid",
            display_name: "Transactions",
            description: "Sync bank transactions with merchant and category info",
            table_name: "stream_plaid_transactions",
            target_ontologies: vec!["financial_transaction"],
            supports_incremental: true,
            supports_full_refresh: true,
            default_cron_schedule: Some("0 0 */6 * * *"), // Every 6 hours
            enabled: true,
            tier: SourceTier::Standard,
        },
        StreamDescriptor {
            name: "accounts",
            source: "plaid",
            display_name: "Accounts",
            description: "Sync bank accounts, credit cards, and account balances",
            table_name: "stream_plaid_accounts",
            target_ontologies: vec!["financial_account"],
            supports_incremental: false, // Accounts always fetched in full
            supports_full_refresh: true,
            // Run 5 minutes before transactions (at :00 of hours 0,6,12,18) to ensure accounts exist for FK references
            // Free API call (/accounts/get), so same frequency as transactions is fine
            default_cron_schedule: Some("0 55 5,11,17,23 * * *"), // At :55 of hours 5,11,17,23 (5 min before 6,12,18,0)
            enabled: true,
            tier: SourceTier::Standard,
        },
        StreamDescriptor {
            name: "investments",
            source: "plaid",
            display_name: "Investments",
            description: "Sync investment holdings, securities, and 401k/IRA/brokerage data",
            table_name: "stream_plaid_investments",
            target_ontologies: vec![], // No ontology yet
            supports_incremental: false,
            supports_full_refresh: true,
            default_cron_schedule: Some("0 0 0 * * *"),
            enabled: false, // Disabled: expensive API calls (~$0.25/call), no ontology yet
            tier: SourceTier::Standard,
        },
        StreamDescriptor {
            name: "liabilities",
            source: "plaid",
            display_name: "Liabilities",
            description: "Sync credit card APRs, mortgages, student loans, and loan details",
            table_name: "stream_plaid_liabilities",
            target_ontologies: vec![], // No ontology yet
            supports_incremental: false,
            supports_full_refresh: true,
            default_cron_schedule: Some("0 0 0 * * *"),
            enabled: false, // Disabled: expensive API calls (~$0.25/call), no ontology yet
            tier: SourceTier::Standard,
        },
        // ===== Strava Streams =====
        StreamDescriptor {
            name: "activities",
            source: "strava",
            display_name: "Strava Activities",
            description: "Workout activities from Strava (runs, rides, swims, hikes, etc.)",
            table_name: "stream_strava_activities",
            target_ontologies: vec!["health_workout"],
            supports_incremental: true,
            supports_full_refresh: true,
            default_cron_schedule: Some("0 */30 * * * *"), // Every 30 minutes
            enabled: true,
            tier: SourceTier::Standard,
        },
        // ===== GitHub Streams =====
        StreamDescriptor {
            name: "events",
            source: "github",
            display_name: "GitHub Events",
            description: "Activity events from GitHub (stars, forks, pushes, PRs, comments)",
            table_name: "stream_github_events",
            target_ontologies: vec!["content_bookmark"],
            supports_incremental: true,
            supports_full_refresh: true,
            default_cron_schedule: Some("0 */15 * * * *"), // Every 15 minutes
            enabled: true,
            tier: SourceTier::Standard,
        },
    ]
}

/// Get streams for a specific source
pub fn get_streams_for_source(source: &str) -> Vec<StreamDescriptor> {
    registered_streams()
        .into_iter()
        .filter(|s| s.source == source)
        .collect()
}

/// Get a stream by source and stream name
pub fn get_stream(source: &str, name: &str) -> Option<StreamDescriptor> {
    registered_streams()
        .into_iter()
        .find(|s| s.source == source && s.name == name)
}

/// Get a stream by its table name
pub fn get_stream_by_table_name(table_name: &str) -> Option<StreamDescriptor> {
    registered_streams()
        .into_iter()
        .find(|s| s.table_name == table_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registered_streams() {
        let streams = registered_streams();
        assert!(!streams.is_empty());

        // Check we have streams for all sources
        let sources: std::collections::HashSet<_> = streams.iter().map(|s| s.source).collect();
        assert!(sources.contains(&"google"));
        assert!(sources.contains(&"ios"));
        assert!(sources.contains(&"mac"));
        assert!(sources.contains(&"notion"));
        assert!(sources.contains(&"plaid"));
        assert!(sources.contains(&"strava"));
        assert!(sources.contains(&"github"));
    }

    #[test]
    fn test_get_streams_for_source() {
        let ios_streams = get_streams_for_source("ios");
        assert!(ios_streams.len() >= 6); // healthkit, location, microphone, contacts, financekit, eventkit

        let google_streams = get_streams_for_source("google");
        assert!(google_streams.len() >= 2); // calendar, gmail
    }

    #[test]
    fn test_get_stream() {
        let calendar = get_stream("google", "calendar");
        assert!(calendar.is_some());
        let cal = calendar.unwrap();
        assert_eq!(cal.table_name, "stream_google_calendar");
        assert!(cal.supports_incremental);
    }

    #[test]
    fn test_get_stream_by_table_name() {
        let stream = get_stream_by_table_name("stream_ios_healthkit");
        assert!(stream.is_some());
        let s = stream.unwrap();
        assert_eq!(s.name, "healthkit");
        assert_eq!(s.source, "ios");
    }

    #[test]
    fn test_stream_table_name_format() {
        for stream in registered_streams() {
            assert!(
                stream.table_name.starts_with("stream_"),
                "Stream {} should have table_name starting with 'stream_'",
                stream.name
            );
            assert!(
                stream.table_name.contains(stream.source),
                "Stream {} table_name should contain source name",
                stream.name
            );
        }
    }
}

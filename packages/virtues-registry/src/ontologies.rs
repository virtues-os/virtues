//! Ontology registry - Normalized data schema definitions
//!
//! This module defines the metadata for ontology tables (health, location, social, etc.).
//! The actual SQL schema lives in Core migrations.

use serde::{Deserialize, Serialize};

/// Embedding configuration for semantic search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// SQL expression for text to embed
    pub embed_text_sql: &'static str,
    /// Content type label for search results (e.g., "email", "document")
    pub content_type: &'static str,
    /// SQL expression for result title (or None for no title)
    pub title_sql: Option<&'static str>,
    /// SQL expression for result preview (max 200 chars)
    pub preview_sql: &'static str,
    /// SQL expression for author/source (or None)
    pub author_sql: Option<&'static str>,
    /// SQL expression for timestamp
    pub timestamp_sql: &'static str,
}

/// Ontology descriptor - metadata only
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyDescriptor {
    /// Unique ontology name (e.g., "health_sleep", "calendar")
    pub name: &'static str,
    /// Human-readable display name
    pub display_name: &'static str,
    /// Description of what this ontology stores
    pub description: &'static str,
    /// Domain grouping (e.g., "health", "location", "social")
    pub domain: &'static str,
    /// Database table name
    pub table_name: &'static str,
    /// Source streams that feed into this ontology
    pub source_streams: Vec<&'static str>,
    /// Primary timestamp column
    pub timestamp_column: &'static str,
    /// Optional end timestamp column for span/duration events
    pub end_timestamp_column: Option<&'static str>,
    /// Embedding configuration for semantic search (None if not searchable)
    pub embedding: Option<EmbeddingConfig>,
}

/// Get all registered ontology descriptors
pub fn registered_ontologies() -> Vec<OntologyDescriptor> {
    vec![
        // ===== Health Ontologies =====
        OntologyDescriptor {
            name: "health_heart_rate",
            display_name: "Heart Rate",
            description: "Heart rate measurements from HealthKit",
            domain: "health",
            table_name: "health_heart_rate",
            source_streams: vec!["stream_ios_healthkit"],
            timestamp_column: "timestamp",
            end_timestamp_column: None,
            embedding: None,
        },
        OntologyDescriptor {
            name: "health_hrv",
            display_name: "Heart Rate Variability",
            description: "HRV measurements indicating stress and recovery",
            domain: "health",
            table_name: "health_hrv",
            source_streams: vec!["stream_ios_healthkit"],
            timestamp_column: "timestamp",
            end_timestamp_column: None,
            embedding: None,
        },
        OntologyDescriptor {
            name: "health_steps",
            display_name: "Steps",
            description: "Step count data from HealthKit",
            domain: "health",
            table_name: "health_steps",
            source_streams: vec!["stream_ios_healthkit"],
            timestamp_column: "timestamp",
            end_timestamp_column: None,
            embedding: None,
        },
        OntologyDescriptor {
            name: "health_sleep",
            display_name: "Sleep Sessions",
            description: "Sleep analysis from HealthKit with quality metrics",
            domain: "health",
            table_name: "health_sleep",
            source_streams: vec!["stream_ios_healthkit"],
            timestamp_column: "start_time",
            end_timestamp_column: Some("end_time"),
            embedding: None,
        },
        OntologyDescriptor {
            name: "health_workout",
            display_name: "Workouts",
            description: "Workout sessions from HealthKit",
            domain: "health",
            table_name: "health_workout",
            source_streams: vec!["stream_ios_healthkit"],
            timestamp_column: "start_time",
            end_timestamp_column: Some("end_time"),
            embedding: None,
        },
        // ===== Location Ontologies =====
        OntologyDescriptor {
            name: "location_point",
            display_name: "Location Points",
            description: "Raw GPS coordinates from device location services",
            domain: "location",
            table_name: "location_point",
            source_streams: vec!["stream_ios_location"],
            timestamp_column: "timestamp",
            end_timestamp_column: None,
            embedding: None,
        },
        OntologyDescriptor {
            name: "location_visit",
            display_name: "Location Visits",
            description: "Clustered location visits with place resolution",
            domain: "location",
            table_name: "location_visit",
            source_streams: vec![], // Derived from location_point via clustering
            timestamp_column: "arrival_time",
            end_timestamp_column: Some("departure_time"),
            embedding: None,
        },
        // ===== Social Ontologies =====
        OntologyDescriptor {
            name: "social_email",
            display_name: "Email",
            description: "Email messages from Gmail and other providers",
            domain: "social",
            table_name: "social_email",
            source_streams: vec!["stream_google_gmail"],
            timestamp_column: "timestamp",
            end_timestamp_column: None,
            embedding: Some(EmbeddingConfig {
                embed_text_sql: "COALESCE(subject, '') || '\n\n' || COALESCE(body_plain, '')",
                content_type: "email",
                title_sql: Some("subject"),
                preview_sql: "COALESCE(LEFT(snippet, 200), LEFT(body_plain, 200), '')",
                author_sql: Some("from_name"),
                timestamp_sql: "timestamp",
            }),
        },
        OntologyDescriptor {
            name: "social_message",
            display_name: "Messages",
            description: "SMS and iMessage conversations",
            domain: "social",
            table_name: "social_message",
            source_streams: vec!["stream_mac_imessage"],
            timestamp_column: "timestamp",
            end_timestamp_column: None,
            embedding: Some(EmbeddingConfig {
                embed_text_sql: "'From ' || COALESCE(from_name, 'Unknown') || ': ' || COALESCE(body, '')",
                content_type: "message",
                title_sql: None,
                preview_sql: "LEFT(body, 200)",
                author_sql: Some("from_name"),
                timestamp_sql: "timestamp",
            }),
        },
        // ===== Calendar Ontology =====
        OntologyDescriptor {
            name: "calendar",
            display_name: "Calendar Events",
            description: "Scheduled events from Google Calendar",
            domain: "calendar",
            table_name: "calendar",
            source_streams: vec!["stream_google_calendar"],
            timestamp_column: "start_time",
            end_timestamp_column: Some("end_time"),
            embedding: Some(EmbeddingConfig {
                embed_text_sql: "COALESCE(title, '') || '\n\n' || COALESCE(description, '')",
                content_type: "calendar",
                title_sql: Some("title"),
                preview_sql: "COALESCE(LEFT(description, 200), '')",
                author_sql: None,
                timestamp_sql: "start_time",
            }),
        },
        // ===== Activity Ontologies =====
        OntologyDescriptor {
            name: "activity_app_usage",
            display_name: "App Usage",
            description: "Application focus events from macOS",
            domain: "activity",
            table_name: "activity_app_usage",
            source_streams: vec!["stream_mac_apps"],
            timestamp_column: "start_time",
            end_timestamp_column: Some("end_time"),
            embedding: None,
        },
        OntologyDescriptor {
            name: "activity_web_browsing",
            display_name: "Web Browsing",
            description: "Browser history from Safari and Chrome",
            domain: "activity",
            table_name: "activity_web_browsing",
            source_streams: vec!["stream_mac_browser"],
            timestamp_column: "timestamp",
            end_timestamp_column: None,
            embedding: None,
        },
        // ===== Speech Ontologies =====
        OntologyDescriptor {
            name: "speech_transcription",
            display_name: "Voice Transcriptions",
            description: "Transcribed audio from microphone recordings",
            domain: "speech",
            table_name: "speech_transcription",
            source_streams: vec!["stream_ios_microphone"],
            timestamp_column: "start_time",
            end_timestamp_column: Some("end_time"),
            embedding: None,
        },
        // ===== Knowledge Ontologies =====
        OntologyDescriptor {
            name: "knowledge_document",
            display_name: "Documents",
            description: "Pages from Notion and other document sources",
            domain: "knowledge",
            table_name: "knowledge_document",
            source_streams: vec!["stream_notion_pages"],
            timestamp_column: "created_time",
            end_timestamp_column: None,
            embedding: Some(EmbeddingConfig {
                embed_text_sql: "COALESCE(title, '') || '\n\n' || COALESCE(content_summary, LEFT(content, 8000), '')",
                content_type: "document",
                title_sql: Some("title"),
                preview_sql: "COALESCE(LEFT(content_summary, 200), LEFT(content, 200), '')",
                author_sql: Some("source_provider"),
                timestamp_sql: "COALESCE(last_modified_time, created_at)",
            }),
        },
        OntologyDescriptor {
            name: "knowledge_ai_conversation",
            display_name: "AI Conversations",
            description: "Chat sessions from Virtues AI assistant",
            domain: "knowledge",
            table_name: "knowledge_ai_conversation",
            source_streams: vec![], // Messages created directly by chat API
            timestamp_column: "timestamp",
            end_timestamp_column: None,
            embedding: Some(EmbeddingConfig {
                embed_text_sql: "content",
                content_type: "ai_conversation",
                title_sql: None,
                preview_sql: "LEFT(content, 200)",
                author_sql: Some("role"),
                timestamp_sql: "timestamp",
            }),
        },
        // ===== Financial Ontologies =====
        OntologyDescriptor {
            name: "financial_account",
            display_name: "Financial Accounts",
            description: "Bank accounts, credit cards, and other financial accounts from Plaid",
            domain: "financial",
            table_name: "financial_account",
            source_streams: vec!["stream_plaid_accounts"],
            timestamp_column: "created_at",
            end_timestamp_column: None,
            embedding: None,
        },
        OntologyDescriptor {
            name: "financial_transaction",
            display_name: "Financial Transactions",
            description: "Bank and credit card transactions from Plaid with merchant and category info",
            domain: "financial",
            table_name: "financial_transaction",
            source_streams: vec!["stream_plaid_transactions"],
            timestamp_column: "timestamp",
            end_timestamp_column: None,
            embedding: Some(EmbeddingConfig {
                embed_text_sql: "COALESCE(merchant_name, name) || ' ' || COALESCE(category, '')",
                content_type: "transaction",
                title_sql: Some("COALESCE(merchant_name, name)"),
                preview_sql: "COALESCE(merchant_name, name) || ' - $' || ABS(amount)::text || ' on ' || transaction_date::text",
                author_sql: None,
                timestamp_sql: "timestamp",
            }),
        },
        // ===== Device Ontologies =====
        OntologyDescriptor {
            name: "device_battery",
            display_name: "Battery Status",
            description: "Device battery level and charging state telemetry",
            domain: "device",
            table_name: "device_battery",
            source_streams: vec!["stream_ios_battery"],
            timestamp_column: "timestamp",
            end_timestamp_column: None,
            embedding: None,
        },
        // ===== Environment Ontologies =====
        OntologyDescriptor {
            name: "environment_pressure",
            display_name: "Atmospheric Pressure",
            description: "Barometric pressure and relative altitude changes",
            domain: "environment",
            table_name: "environment_pressure",
            source_streams: vec!["stream_ios_barometer"],
            timestamp_column: "timestamp",
            end_timestamp_column: None,
            embedding: None,
        },
    ]
}

/// Get ontology by name
pub fn get_ontology(name: &str) -> Option<OntologyDescriptor> {
    registered_ontologies().into_iter().find(|o| o.name == name)
}

/// Get ontologies by domain
pub fn get_ontologies_by_domain(domain: &str) -> Vec<OntologyDescriptor> {
    registered_ontologies()
        .into_iter()
        .filter(|o| o.domain == domain)
        .collect()
}

/// Get ontologies that have semantic search enabled
pub fn get_searchable_ontologies() -> Vec<OntologyDescriptor> {
    registered_ontologies()
        .into_iter()
        .filter(|o| o.embedding.is_some())
        .collect()
}

/// Get ontologies that are fed by a specific stream
pub fn get_ontologies_for_stream(stream_table: &str) -> Vec<OntologyDescriptor> {
    registered_ontologies()
        .into_iter()
        .filter(|o| o.source_streams.contains(&stream_table))
        .collect()
}

/// List all domain names
pub fn list_domains() -> Vec<&'static str> {
    vec![
        "health",
        "location",
        "social",
        "calendar",
        "activity",
        "speech",
        "knowledge",
        "financial",
        "device",
        "environment",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registered_ontologies() {
        let ontologies = registered_ontologies();
        assert!(!ontologies.is_empty());

        // Check we have all expected domains
        let domains: std::collections::HashSet<_> = ontologies.iter().map(|o| o.domain).collect();
        assert!(domains.contains(&"health"));
        assert!(domains.contains(&"location"));
        assert!(domains.contains(&"social"));
        assert!(domains.contains(&"calendar"));
    }

    #[test]
    fn test_get_ontology() {
        let sleep = get_ontology("health_sleep");
        assert!(sleep.is_some());
        let s = sleep.unwrap();
        assert_eq!(s.domain, "health");
        assert_eq!(s.timestamp_column, "start_time");
        assert_eq!(s.end_timestamp_column, Some("end_time"));
    }

    #[test]
    fn test_searchable_ontologies() {
        let searchable = get_searchable_ontologies();
        // Should have: email, message, calendar, document, ai_conversation, financial_transaction
        assert!(searchable.len() >= 6);
        for o in &searchable {
            assert!(o.embedding.is_some());
        }
    }

    #[test]
    fn test_get_ontologies_for_stream() {
        let healthkit_ontologies = get_ontologies_for_stream("stream_ios_healthkit");
        assert!(healthkit_ontologies.len() >= 5); // heart_rate, hrv, steps, sleep, workout
    }

    #[test]
    fn test_ontology_table_names() {
        for ontology in registered_ontologies() {
            // Most ontologies have table_name == name
            // Exception: calendar (name and table_name are both "calendar")
            assert!(
                !ontology.table_name.is_empty(),
                "Ontology {} should have a table_name",
                ontology.name
            );
        }
    }
}

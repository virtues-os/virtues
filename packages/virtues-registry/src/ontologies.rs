//! Ontology registry - Normalized data schema definitions
//!
//! This module defines the metadata for ontology tables (health, location, social, etc.).
//! The actual SQL schema lives in Core migrations.

use serde::{Deserialize, Serialize};

/// Weights for each context dimension: [who, whom, what, when, where, why, how]
/// Values 0.0-1.0, only non-zero where the ontology genuinely informs that dimension.
pub type ContextWeights = [f32; 7];

pub const CTX_WHO: usize = 0; // self-awareness
pub const CTX_WHOM: usize = 1; // relational
pub const CTX_WHAT: usize = 2; // content/events
pub const CTX_WHEN: usize = 3; // temporal coverage
pub const CTX_WHERE: usize = 4; // spatial
pub const CTX_WHY: usize = 5; // intent/motivation
pub const CTX_HOW: usize = 6; // physical state

/// Embedding configuration for semantic search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// SQL expression for text to embed (use `t.` prefix for column refs — query aliases table as `t`)
    pub embed_text_sql: &'static str,
    /// Content type label for search results (e.g., "email", "document")
    pub content_type: &'static str,
    /// SQL expression for result title (use `t.` prefix — query aliases table as `t`)
    pub title_sql: Option<&'static str>,
    /// SQL expression for result preview (use `t.` prefix — query aliases table as `t`)
    pub preview_sql: &'static str,
    /// SQL expression for author/source (use `t.` prefix — query aliases table as `t`)
    pub author_sql: Option<&'static str>,
    /// SQL expression for timestamp (use `t.` prefix — query aliases table as `t`)
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
    /// Full database table name (e.g., "data_health_sleep", "chats")
    pub table_name: &'static str,
    /// Source streams that feed into this ontology
    pub source_streams: Vec<&'static str>,
    /// Primary timestamp column
    pub timestamp_column: &'static str,
    /// Optional end timestamp column for span/duration events
    pub end_timestamp_column: Option<&'static str>,
    /// Embedding configuration for semantic search (None if not searchable)
    pub embedding: Option<EmbeddingConfig>,
    /// Context dimension weights [who, whom, what, when, where, why, how]
    pub context_weights: ContextWeights,
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
            table_name: "data_health_heart_rate",
            source_streams: vec!["stream_ios_healthkit"],
            timestamp_column: "timestamp",
            end_timestamp_column: None,
            embedding: None,
            //                    who  whom what when where why  how
            context_weights: [0.8, 0.0, 0.0, 0.8, 0.0, 0.0, 0.8],
        },
        OntologyDescriptor {
            name: "health_hrv",
            display_name: "Heart Rate Variability",
            description: "HRV measurements indicating stress and recovery",
            domain: "health",
            table_name: "data_health_hrv",
            source_streams: vec!["stream_ios_healthkit"],
            timestamp_column: "timestamp",
            end_timestamp_column: None,
            embedding: None,
            //                    who  whom what when where why  how
            context_weights: [0.6, 0.0, 0.0, 0.0, 0.0, 0.0, 0.7],
        },
        OntologyDescriptor {
            name: "health_steps",
            display_name: "Steps",
            description: "Step count data from HealthKit",
            domain: "health",
            table_name: "data_health_steps",
            source_streams: vec!["stream_ios_healthkit"],
            timestamp_column: "timestamp",
            end_timestamp_column: None,
            embedding: None,
            //                    who  whom what when where why  how
            context_weights: [0.7, 0.0, 0.0, 0.7, 0.0, 0.0, 0.6],
        },
        OntologyDescriptor {
            name: "health_sleep",
            display_name: "Sleep Sessions",
            description: "Sleep analysis from HealthKit with quality metrics",
            domain: "health",
            table_name: "data_health_sleep",
            source_streams: vec!["stream_ios_healthkit"],
            timestamp_column: "start_time",
            end_timestamp_column: Some("end_time"),
            embedding: None,
            //                    who  whom what when where why  how
            context_weights: [0.8, 0.0, 0.0, 0.6, 0.0, 0.0, 1.0],
        },
        OntologyDescriptor {
            name: "health_workout",
            display_name: "Workouts",
            description: "Workout sessions from HealthKit and Strava",
            domain: "health",
            table_name: "data_health_workout",
            source_streams: vec!["stream_ios_healthkit", "stream_strava_activities"],
            timestamp_column: "start_time",
            end_timestamp_column: Some("end_time"),
            embedding: None,
            //                    who  whom what when where why  how
            context_weights: [0.7, 0.0, 0.3, 0.0, 0.2, 0.0, 0.9],
        },
        // ===== Location Ontologies =====
        OntologyDescriptor {
            name: "location_point",
            display_name: "Location Points",
            description: "Raw GPS coordinates from device location services",
            domain: "location",
            table_name: "data_location_point",
            source_streams: vec!["stream_ios_location"],
            timestamp_column: "timestamp",
            end_timestamp_column: None,
            embedding: None,
            //                    who  whom what when where why  how
            context_weights: [0.6, 0.0, 0.0, 0.9, 1.0, 0.0, 0.0],
        },
        OntologyDescriptor {
            name: "location_visit",
            display_name: "Location Visits",
            description: "Clustered location visits with place resolution",
            domain: "location",
            table_name: "data_location_visit",
            source_streams: vec![], // Derived from location_point via clustering
            timestamp_column: "arrival_time",
            end_timestamp_column: Some("departure_time"),
            embedding: None,
            //                    who  whom what when where why  how
            context_weights: [0.6, 0.0, 0.2, 0.4, 0.9, 0.0, 0.0],
        },
        // ===== Communication Ontologies =====
        OntologyDescriptor {
            name: "communication_email",
            display_name: "Email",
            description: "Email messages from Gmail and other providers",
            domain: "communication",
            table_name: "data_communication_email",
            source_streams: vec!["stream_google_gmail"],
            timestamp_column: "timestamp",
            end_timestamp_column: None,
            embedding: Some(EmbeddingConfig {
                embed_text_sql: "COALESCE(t.subject, '') || '\n\n' || COALESCE(t.body, '')",
                content_type: "email",
                title_sql: Some("t.subject"),
                preview_sql: "COALESCE(SUBSTR(t.body_preview, 1, 200), SUBSTR(t.body, 1, 200), '')",
                author_sql: Some("t.from_name"),
                timestamp_sql: "t.timestamp",
            }),
            //                    who  whom what when where why  how
            context_weights: [0.0, 0.9, 0.4, 0.0, 0.0, 0.0, 0.0],
        },
        OntologyDescriptor {
            name: "communication_message",
            display_name: "Messages",
            description: "SMS and iMessage conversations",
            domain: "communication",
            table_name: "data_communication_message",
            source_streams: vec!["stream_mac_imessage"],
            timestamp_column: "timestamp",
            end_timestamp_column: None,
            embedding: Some(EmbeddingConfig {
                embed_text_sql: "'From ' || COALESCE(t.from_name, 'Unknown') || ': ' || COALESCE(t.body, '')",
                content_type: "message",
                title_sql: None,
                preview_sql: "SUBSTR(t.body, 1, 200)",
                author_sql: Some("t.from_name"),
                timestamp_sql: "t.timestamp",
            }),
            //                    who  whom what when where why  how
            context_weights: [0.0, 1.0, 0.4, 0.0, 0.0, 0.0, 0.0],
        },
        // ===== Calendar Ontology =====
        OntologyDescriptor {
            name: "calendar_event",
            display_name: "Calendar Events",
            description: "Scheduled events from Google Calendar and iOS EventKit",
            domain: "calendar",
            table_name: "data_calendar_event",
            source_streams: vec!["stream_google_calendar", "stream_ios_eventkit"],
            timestamp_column: "start_time",
            end_timestamp_column: Some("end_time"),
            embedding: Some(EmbeddingConfig {
                embed_text_sql: "COALESCE(t.title, '') || '\n\n' || COALESCE(t.description, '')",
                content_type: "calendar",
                title_sql: Some("t.title"),
                preview_sql: "COALESCE(SUBSTR(t.description, 1, 200), '')",
                author_sql: None,
                timestamp_sql: "t.start_time",
            }),
            //                    who  whom what when where why  how
            context_weights: [0.0, 0.8, 0.8, 0.6, 0.0, 0.2, 0.0],
        },
        // ===== Activity Ontologies =====
        OntologyDescriptor {
            name: "activity_app_usage",
            display_name: "App Usage",
            description: "Application focus events from macOS",
            domain: "activity",
            table_name: "data_activity_app_usage",
            source_streams: vec!["stream_mac_apps"],
            timestamp_column: "start_time",
            end_timestamp_column: Some("end_time"),
            embedding: None,
            //                    who  whom what when where why  how
            context_weights: [0.4, 0.0, 0.4, 0.5, 0.0, 0.0, 0.0],
        },
        OntologyDescriptor {
            name: "activity_web_browsing",
            display_name: "Web Browsing",
            description: "Browser history from Safari and Chrome",
            domain: "activity",
            table_name: "data_activity_web_browsing",
            source_streams: vec!["stream_mac_browser"],
            timestamp_column: "timestamp",
            end_timestamp_column: None,
            embedding: None,
            //                    who  whom what when where why  how
            context_weights: [0.3, 0.0, 0.5, 0.0, 0.0, 0.3, 0.0],
        },
        OntologyDescriptor {
            name: "communication_transcription",
            display_name: "Voice Transcriptions",
            description: "Transcribed audio from microphone recordings",
            domain: "communication",
            table_name: "data_communication_transcription",
            source_streams: vec!["stream_ios_microphone"],
            timestamp_column: "start_time",
            end_timestamp_column: Some("end_time"),
            embedding: None,
            //                    who  whom what when where why  how
            context_weights: [0.3, 0.3, 1.0, 0.0, 0.0, 0.8, 0.0],
        },
        // ===== Content Ontologies =====
        OntologyDescriptor {
            name: "content_document",
            display_name: "Documents",
            description: "Pages from Notion and other document sources",
            domain: "content",
            table_name: "data_content_document",
            source_streams: vec!["stream_notion_pages"],
            timestamp_column: "created_time",
            end_timestamp_column: None,
            embedding: Some(EmbeddingConfig {
                embed_text_sql: "COALESCE(t.title, '') || '\n\n' || COALESCE(t.content_summary, SUBSTR(t.content, 1, 8000), '')",
                content_type: "document",
                title_sql: Some("t.title"),
                preview_sql: "COALESCE(SUBSTR(t.content_summary, 1, 200), SUBSTR(t.content, 1, 200), '')",
                author_sql: Some("t.source_provider"),
                timestamp_sql: "COALESCE(t.last_modified_time, t.created_at)",
            }),
            //                    who  whom what when where why  how
            context_weights: [0.0, 0.0, 0.7, 0.0, 0.0, 0.4, 0.0],
        },
        OntologyDescriptor {
            name: "content_conversation",
            display_name: "AI Conversations",
            description: "Chat sessions from Virtues AI assistant (search artifact)",
            domain: "content",
            table_name: "data_content_conversation",
            source_streams: vec![], // Messages created directly by chat API
            timestamp_column: "timestamp",
            end_timestamp_column: None,
            embedding: Some(EmbeddingConfig {
                embed_text_sql: "t.content",
                content_type: "ai_conversation",
                title_sql: None,
                preview_sql: "SUBSTR(t.content, 1, 200)",
                author_sql: Some("t.role"),
                timestamp_sql: "t.timestamp",
            }),
            //                    who  whom what when where why  how
            context_weights: [0.0, 0.0, 0.6, 0.0, 0.0, 0.5, 0.0],
        },
        // ===== Financial Ontologies =====
        OntologyDescriptor {
            name: "financial_account",
            display_name: "Financial Accounts",
            description: "Bank accounts, credit cards, and other financial accounts from Plaid",
            domain: "financial",
            table_name: "data_financial_account",
            source_streams: vec!["stream_plaid_accounts", "stream_ios_financekit"],
            timestamp_column: "created_at",
            end_timestamp_column: None,
            embedding: None,
            //                    who  whom what when where why  how
            context_weights: [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        },
        OntologyDescriptor {
            name: "financial_transaction",
            display_name: "Financial Transactions",
            description: "Bank and credit card transactions from Plaid with merchant and category info",
            domain: "financial",
            table_name: "data_financial_transaction",
            source_streams: vec!["stream_plaid_transactions", "stream_ios_financekit"],
            timestamp_column: "timestamp",
            end_timestamp_column: None,
            embedding: Some(EmbeddingConfig {
                embed_text_sql: "COALESCE(t.merchant_name, t.description, '') || ' ' || COALESCE(t.category, '')",
                content_type: "transaction",
                title_sql: Some("COALESCE(t.merchant_name, t.description)"),
                preview_sql: "COALESCE(t.merchant_name, t.description) || ' - $' || CAST(ABS(t.amount / 100.0) AS TEXT) || ' on ' || t.timestamp",
                author_sql: None,
                timestamp_sql: "t.timestamp",
            }),
            //                    who  whom what when where why  how
            context_weights: [0.0, 0.0, 0.5, 0.0, 0.2, 0.0, 0.0],
        },
        OntologyDescriptor {
            name: "content_bookmark",
            display_name: "Bookmarks",
            description: "Saved/curated items: GitHub stars, browser bookmarks, saved links",
            domain: "content",
            table_name: "data_content_bookmark",
            source_streams: vec!["stream_github_events"],
            timestamp_column: "timestamp",
            end_timestamp_column: None,
            embedding: Some(EmbeddingConfig {
                embed_text_sql: "COALESCE(t.title, '') || '\n\n' || COALESCE(t.description, '')",
                content_type: "bookmark",
                title_sql: Some("t.title"),
                preview_sql: "COALESCE(SUBSTR(t.description, 1, 200), t.url)",
                author_sql: Some("t.author"),
                timestamp_sql: "t.timestamp",
            }),
            //                    who  whom what when where why  how
            context_weights: [0.0, 0.0, 0.6, 0.0, 0.0, 0.3, 0.0],
        },
        // ─────────────────────────────────────────────────────────────
        // App (intra-Virtues activity)
        // ─────────────────────────────────────────────────────────────
        OntologyDescriptor {
            name: "app_chat",
            display_name: "Chat Sessions",
            description: "Conversations with Virtues AI assistant",
            domain: "app",
            table_name: "app_chats",
            source_streams: vec![],
            timestamp_column: "created_at",
            end_timestamp_column: Some("updated_at"),
            embedding: None,
            //                    who  whom what when where why  how
            context_weights: [0.1, 0.1, 0.5, 0.2, 0.0, 0.4, 0.0],
        },
        OntologyDescriptor {
            name: "app_page_edit",
            display_name: "Page Edits",
            description: "Wiki page creations and modifications",
            domain: "app",
            table_name: "app_pages",
            source_streams: vec![],
            timestamp_column: "updated_at",
            end_timestamp_column: None,
            embedding: None,
            //                    who  whom what when where why  how
            context_weights: [0.1, 0.0, 0.6, 0.2, 0.0, 0.3, 0.0],
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
        "calendar",
        "communication",
        "content",
        "financial",
        "activity",
        "app",
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
        assert!(domains.contains(&"communication"));
        assert!(domains.contains(&"calendar"));
        assert!(domains.contains(&"content"));
        assert!(domains.contains(&"financial"));
        assert!(domains.contains(&"app"));
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
        // Should have: email, message, calendar_event, document, conversation, financial_transaction, content_bookmark
        assert!(searchable.len() >= 7);
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
            assert!(
                !ontology.table_name.is_empty(),
                "Ontology {} should have a table_name",
                ontology.name
            );
        }
    }

    /// Validates bidirectional consistency between ontology source_streams
    /// and stream target_ontologies. Catches drift between the two metadata definitions.
    #[test]
    fn test_stream_ontology_consistency() {
        use crate::streams::registered_streams;

        let ontologies = registered_ontologies();
        let streams = registered_streams();
        let mut errors = Vec::new();

        // Check: Every ontology's source_streams should have a corresponding stream
        // that lists this ontology in its target_ontologies
        for ontology in &ontologies {
            for source_stream in &ontology.source_streams {
                let matching_stream = streams
                    .iter()
                    .find(|s| s.table_name == *source_stream);

                match matching_stream {
                    Some(stream) => {
                        if !stream.target_ontologies.contains(&ontology.name) {
                            errors.push(format!(
                                "Ontology '{}' claims source_stream '{}', but stream '{}/{}' doesn't list it in target_ontologies (has: {:?})",
                                ontology.name, source_stream, stream.source, stream.name, stream.target_ontologies
                            ));
                        }
                    }
                    None => {
                        errors.push(format!(
                            "Ontology '{}' claims source_stream '{}' but no stream has that table_name",
                            ontology.name, source_stream
                        ));
                    }
                }
            }
        }

        // Check: Every stream's target_ontologies should exist and list that stream
        for stream in &streams {
            for target_ontology in &stream.target_ontologies {
                match ontologies.iter().find(|o| o.name == *target_ontology) {
                    Some(ontology) => {
                        if !ontology.source_streams.contains(&stream.table_name) {
                            errors.push(format!(
                                "Stream '{}/{}' (table: {}) claims target_ontology '{}', but ontology doesn't list it in source_streams (has: {:?})",
                                stream.source, stream.name, stream.table_name, target_ontology, ontology.source_streams
                            ));
                        }
                    }
                    None => {
                        errors.push(format!(
                            "Stream '{}/{}' claims target_ontology '{}' but no such ontology exists",
                            stream.source, stream.name, target_ontology
                        ));
                    }
                }
            }
        }

        assert!(
            errors.is_empty(),
            "Registry consistency errors:\n{}",
            errors.join("\n")
        );
    }
}

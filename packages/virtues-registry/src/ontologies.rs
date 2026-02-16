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
pub const CTX_HOW: usize = 6; // means/method/process

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

/// Whether an ontology produces discrete events or continuous measurement streams
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum TemporalType {
    /// Individual occurrences with timestamps (e.g., calendar events, messages, workouts)
    Discrete,
    /// Constant measurement stream needing aggregation (e.g., heart rate, HRV, steps)
    Continuous,
}

/// How a discrete ontology contributes to day sources.
/// SQL expressions use `t.` prefix (table aliased as `t`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaySourceConfig {
    /// Static source type label (e.g., "calendar", "email", "workout")
    pub source_type: &'static str,
    /// Optional SQL expression for dynamic source_type (overrides source_type when present)
    pub source_type_sql: Option<&'static str>,
    /// SQL expression for the event label
    pub label_sql: &'static str,
    /// SQL expression for the event preview text
    pub preview_sql: &'static str,
    /// SQL expression for the record ID
    pub id_sql: &'static str,
    /// Optional additional WHERE clause (e.g., confidence filters)
    pub extra_where: Option<&'static str>,
    /// If true, use `date(column) = $1` instead of timestamp range comparison
    pub use_date_filter: bool,
}

/// How a continuous ontology produces per-window summary stats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContinuousAggConfig {
    /// Template string with placeholders: {avg}, {min}, {max}, {std}, {sum}, {count}
    pub summary_template: &'static str,
    /// SQL expression for the numeric value column
    pub value_sql: &'static str,
    /// Aggregation type: "stats" (avg/min/max/std) or "sum"
    pub agg_type: &'static str,
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
    /// Whether this ontology produces discrete events or continuous measurements
    pub temporal_type: TemporalType,
    /// How discrete ontologies contribute to day sources (None for continuous/non-event ontologies)
    pub day_source: Option<DaySourceConfig>,
    /// How continuous ontologies produce aggregated summaries (None for discrete ontologies)
    pub continuous_agg: Option<ContinuousAggConfig>,
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
            temporal_type: TemporalType::Continuous,
            day_source: None,
            continuous_agg: Some(ContinuousAggConfig {
                summary_template: "Heart rate: avg {avg} bpm ({min}-{max})",
                value_sql: "t.bpm",
                agg_type: "stats",
            }),
            //                    who  whom what when where why  how
            context_weights: [0.8, 0.0, 0.0, 0.0, 0.0, 0.0, 0.2],
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
            temporal_type: TemporalType::Continuous,
            day_source: None,
            continuous_agg: Some(ContinuousAggConfig {
                summary_template: "HRV: avg {avg}ms ({min}-{max})",
                value_sql: "t.hrv_ms",
                agg_type: "stats",
            }),
            //                    who  whom what when where why  how
            context_weights: [0.7, 0.0, 0.0, 0.0, 0.0, 0.0, 0.2],
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
            temporal_type: TemporalType::Continuous,
            day_source: None,
            continuous_agg: Some(ContinuousAggConfig {
                summary_template: "Steps: {sum} total",
                value_sql: "t.count",
                agg_type: "sum",
            }),
            //                    who  whom what when where why  how
            context_weights: [0.6, 0.0, 0.0, 0.0, 0.0, 0.0, 0.4],
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
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "sleep",
                source_type_sql: None,
                label_sql: "'Sleep'",
                preview_sql: "CASE WHEN t.duration_minutes IS NOT NULL THEN CAST(t.duration_minutes AS TEXT) || ' min' ELSE NULL END",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: None,
            //                    who  whom what when where why  how
            context_weights: [0.9, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0],
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
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "workout",
                source_type_sql: None,
                label_sql: "COALESCE(t.workout_type, 'Workout')",
                preview_sql: "CASE WHEN t.duration_minutes IS NOT NULL THEN CAST(t.duration_minutes AS TEXT) || ' min' ELSE NULL END",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: None,
            //                    who  whom what when where why  how
            context_weights: [0.7, 0.0, 0.5, 0.0, 0.3, 0.2, 0.6],
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
            temporal_type: TemporalType::Continuous,
            day_source: None,
            continuous_agg: None, // Spatial data — not a numeric aggregate
            //                    who  whom what when where why  how
            context_weights: [0.0, 0.0, 0.0, 0.2, 1.0, 0.0, 0.0],
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
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "location",
                source_type_sql: None,
                label_sql: "COALESCE(t.place_name, 'Unknown location')",
                preview_sql: "CASE WHEN t.duration_minutes IS NOT NULL THEN CAST(t.duration_minutes AS TEXT) || ' min' ELSE NULL END",
                id_sql: "hex(t.id)",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: None,
            //                    who  whom what when where why  how
            context_weights: [0.0, 0.0, 0.3, 0.4, 0.9, 0.0, 0.0],
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
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "email",
                source_type_sql: Some("CASE WHEN t.direction = 'sent' THEN 'email_sent' ELSE 'email' END"),
                label_sql: "COALESCE(t.subject, '(no subject)')",
                preview_sql: "CASE WHEN t.direction = 'sent' THEN 'To: ' ELSE 'From: ' END || COALESCE(t.from_email, 'unknown')",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: None,
            //                    who  whom what when where why  how
            context_weights: [0.0, 0.9, 0.5, 0.0, 0.0, 0.2, 0.0],
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
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "message",
                source_type_sql: Some("'message:' || COALESCE(t.channel, 'unknown')"),
                label_sql: "COALESCE(t.from_name, 'Unknown')",
                preview_sql: "SUBSTR(COALESCE(t.body, ''), 1, 50)",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: None,
            //                    who  whom what when where why  how
            context_weights: [0.0, 1.0, 0.5, 0.0, 0.0, 0.2, 0.0],
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
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "calendar",
                source_type_sql: None,
                label_sql: "COALESCE(t.title, '(no title)')",
                preview_sql: "NULL",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: None,
            //                    who  whom what when where why  how
            context_weights: [0.0, 0.8, 0.8, 0.7, 0.3, 0.3, 0.0],
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
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "app_usage",
                source_type_sql: None,
                label_sql: "COALESCE(t.app_name, 'Unknown app')",
                preview_sql: "t.window_title",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: None,
            //                    who  whom what when where why  how
            context_weights: [0.0, 0.0, 0.4, 0.0, 0.0, 0.0, 0.7],
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
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "web_browsing",
                source_type_sql: None,
                label_sql: "COALESCE(t.page_title, t.url, 'Unknown page')",
                preview_sql: "t.domain",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: None,
            //                    who  whom what when where why  how
            context_weights: [0.0, 0.0, 0.6, 0.0, 0.0, 0.3, 0.4],
        },
        OntologyDescriptor {
            name: "activity_listening",
            display_name: "Listening History",
            description: "Music and audio listening history from Spotify",
            domain: "activity",
            table_name: "data_activity_listening",
            source_streams: vec!["stream_spotify_recently_played"],
            timestamp_column: "played_at",
            end_timestamp_column: None,
            embedding: None,
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "listening",
                source_type_sql: None,
                label_sql: "COALESCE(t.artist_name, 'Unknown') || ' — ' || t.track_name",
                preview_sql: "CASE WHEN t.duration_ms IS NOT NULL THEN CAST(t.duration_ms / 60000 AS TEXT) || ' min' ELSE NULL END",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: None,
            //                    who  whom what when where why  how
            context_weights: [0.7, 0.0, 0.5, 0.0, 0.0, 0.2, 0.3],
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
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "transcription",
                source_type_sql: None,
                label_sql: "COALESCE(t.title, 'Transcription')",
                preview_sql: "SUBSTR(COALESCE(t.text, ''), 1, 60)",
                id_sql: "t.id",
                extra_where: Some("AND (t.confidence IS NULL OR t.confidence > 0.1)"),
                use_date_filter: false,
            }),
            continuous_agg: None,
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
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "document",
                source_type_sql: None,
                label_sql: "COALESCE(t.title, 'Untitled')",
                preview_sql: "SUBSTR(COALESCE(t.content_summary, t.content, ''), 1, 80)",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: None,
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
            temporal_type: TemporalType::Discrete,
            day_source: None, // Individual messages not useful as day sources
            continuous_agg: None,
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
            temporal_type: TemporalType::Discrete,
            day_source: None, // Not events
            continuous_agg: None,
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
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "transaction",
                source_type_sql: None,
                label_sql: "COALESCE(t.merchant_name, t.description, '(no description)')",
                preview_sql: "'$' || CAST(ABS(t.amount / 100.0) AS TEXT)",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: None,
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
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "bookmark",
                source_type_sql: None,
                label_sql: "COALESCE(t.title, t.url, 'Bookmark')",
                preview_sql: "SUBSTR(COALESCE(t.description, t.url, ''), 1, 80)",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: None,
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
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "chat",
                source_type_sql: None,
                label_sql: "COALESCE(t.title, 'Chat')",
                preview_sql: "CAST(t.message_count AS TEXT) || ' messages'",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: true,
            }),
            continuous_agg: None,
            //                    who  whom what when where why  how
            context_weights: [0.1, 0.1, 0.5, 0.2, 0.0, 0.4, 0.0],
        },
        OntologyDescriptor {
            name: "app_page",
            display_name: "Page Edits",
            description: "Wiki page creations and modifications",
            domain: "app",
            table_name: "app_pages",
            source_streams: vec![],
            timestamp_column: "updated_at",
            end_timestamp_column: None,
            embedding: None,
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "page",
                source_type_sql: None,
                label_sql: "COALESCE(t.icon || ' ', '') || COALESCE(t.title, 'Untitled')",
                preview_sql: "NULL",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: true,
            }),
            continuous_agg: None,
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

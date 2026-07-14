//! Ontology registry - Normalized data schema definitions
//!
//! This module defines the metadata for ontology tables (health, location, social, etc.).
//! The actual SQL schema lives in Core migrations.

use serde::{Deserialize, Serialize};

/// Entity-extraction configuration — which ontologies carry PROSE.
///
/// Keyed on the **ontology**, not the source, and that is the whole point:
/// Gmail, Fastmail and an mbox import all normalize into
/// `data_communication_email`, so entity extraction is configured once and
/// every present and future source inherits it. Slack lands in
/// `data_communication_message` and works with no new code at all.
///
/// Of 23 ontologies, five carry prose. The other eighteen — every health,
/// location, financial and activity table — have no free text and never enter
/// extraction. That is the real size of this problem.
///
/// `data_communication_transcription` deliberately has NO config here: its
/// entities are already extracted by the transcription action's own LLM call,
/// so it is drained for free rather than re-extracted (see
/// `entity_resolution::extract`). Paying twice for the same names would be
/// absurd.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionConfig {
    /// SQL expression for the prose to read (`t.` prefix; the query aliases the
    /// table as `t`). Usually the same text we embed — it is the same text.
    pub text_sql: &'static str,
    /// SQL predicate that excludes records not worth reading (`t.` prefix).
    ///
    /// This is where money is saved and precision is bought, and for email it
    /// is not a heuristic: Gmail's own `labelIds` are stored verbatim, so
    /// Google's classifier — the one that fills your Promotions tab — does the
    /// work. Excluded records are NOT deleted; they remain as dust, searchable,
    /// merely never read for names.
    pub filter_sql: Option<&'static str>,
    /// Hard cap on prose sent to the model, in characters.
    ///
    /// Email and documents are where token mass hides: a newsletter is mostly
    /// boilerplate, a reply chain is mostly the quoted reply. Names appear
    /// early. Truncation costs almost no recall and bounds the bill.
    pub max_chars: usize,
}

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
    /// Entity-extraction configuration (None = this ontology carries no prose,
    /// which is true of 18 of the 23 — every health, location, financial and
    /// activity table). See `ExtractionConfig`.
    pub extraction: Option<ExtractionConfig>,
    /// Whether this ontology produces discrete events or continuous measurements
    pub temporal_type: TemporalType,
    /// How discrete ontologies contribute to day sources (None for continuous/non-event ontologies)
    pub day_source: Option<DaySourceConfig>,
    /// How continuous ontologies produce aggregated summaries (None for discrete ontologies)
    pub continuous_agg: Option<ContinuousAggConfig>,
    /// Whether this ontology represents active behavioral signal (for action activation gates).
    /// True for: app_usage, location_visit, calendar, outbound messages, transcription, web_browsing, listening, workout.
    /// False for passive data: heart_rate, hrv, steps, sleep, inbound email, financial records.
    pub is_activation_signal: bool,
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
            extraction: None,
            temporal_type: TemporalType::Continuous,
            // Continuous streams surface as individual day-source rows too, so the
            // day page can list every data point. They are high-frequency, so the
            // UI hides them behind a filter by default (keyed off `temporal_type`).
            day_source: Some(DaySourceConfig {
                source_type: "heart_rate",
                source_type_sql: None,
                label_sql: "'Heart rate'",
                preview_sql: "CAST(t.bpm AS TEXT) || ' bpm'",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: Some(ContinuousAggConfig {
                summary_template: "Heart rate: avg {avg} bpm ({min}-{max})",
                value_sql: "t.bpm",
                agg_type: "stats",
            }),
            //                    who  whom what when where why  how
            is_activation_signal: false,
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
            extraction: None,
            temporal_type: TemporalType::Continuous,
            day_source: Some(DaySourceConfig {
                source_type: "hrv",
                source_type_sql: None,
                label_sql: "'HRV'",
                preview_sql: "CAST(ROUND(t.hrv_ms) AS INT) || ' ms'",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: Some(ContinuousAggConfig {
                summary_template: "HRV: avg {avg}ms ({min}-{max})",
                value_sql: "t.hrv_ms",
                agg_type: "stats",
            }),
            //                    who  whom what when where why  how
            is_activation_signal: false,
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
            extraction: None,
            temporal_type: TemporalType::Continuous,
            day_source: Some(DaySourceConfig {
                source_type: "steps",
                source_type_sql: None,
                label_sql: "'Steps'",
                preview_sql: "CAST(t.step_count AS TEXT) || ' steps'",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: Some(ContinuousAggConfig {
                summary_template: "Steps: {sum} total",
                value_sql: "t.count",
                agg_type: "sum",
            }),
            //                    who  whom what when where why  how
            is_activation_signal: false,
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
            extraction: None,
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
            is_activation_signal: false,
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
            extraction: None,
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
            is_activation_signal: true,
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
            extraction: None,
            temporal_type: TemporalType::Continuous,
            day_source: None,
            continuous_agg: None, // Spatial data — not a numeric aggregate
            //                    who  whom what when where why  how
            is_activation_signal: false,
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
            extraction: None,
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "location",
                source_type_sql: None,
                label_sql: "COALESCE(t.place_name, 'Unknown location')",
                preview_sql: "CASE WHEN t.duration_minutes IS NOT NULL THEN CAST(t.duration_minutes AS TEXT) || ' min' ELSE NULL END",
                id_sql: "encode(t.id, 'hex')",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: None,
            //                    who  whom what when where why  how
            is_activation_signal: true,
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
            // Bodies only. The From:/To: headers are ALREADY resolved
            // structurally (entity_resolution::people) — that is a join, not a
            // guess. Re-extracting the sender's name out of a signature block
            // would build a second, worse path to the same person.
            extraction: Some(ExtractionConfig {
                text_sql: "COALESCE(t.subject, '') || E'\n\n' || COALESCE(t.body, '')",
                // Google already sorted the junk. Its labelIds are stored
                // verbatim, so this is its classifier, not our heuristic.
                // CATEGORY_UPDATES is deliberately KEPT: flight confirmations,
                // bookings and receipts look like noise and are some of the
                // densest real places, orgs and future dates in the mailbox.
                filter_sql: Some(
                    "NOT (t.labels ?| array['CATEGORY_PROMOTIONS','CATEGORY_SOCIAL','CATEGORY_FORUMS','SPAM'])",
                ),
                max_chars: 6000,
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
            is_activation_signal: false,
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
            extraction: Some(ExtractionConfig {
                text_sql: "COALESCE(t.body, '')",
                filter_sql: None,
                max_chars: 2000,
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
            is_activation_signal: true,
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
            extraction: None,
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
            is_activation_signal: true,
        },
        // ===== Activity Ontologies =====
        OntologyDescriptor {
            name: "activity_app_usage",
            display_name: "App Usage",
            description: "Attended, focused time in an app — bounded by idle, lock and sleep",
            domain: "activity",
            table_name: "data_activity_app_usage",
            source_streams: vec!["stream_mac_apps"],
            timestamp_column: "start_time",
            end_timestamp_column: Some("end_time"),
            embedding: None,
            extraction: None,
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "app_usage",
                source_type_sql: None,
                label_sql: "COALESCE(t.app_name, 'Unknown app')",
                preview_sql: "t.window_title",
                id_sql: "t.id",
                // An open session's end_time is provisional (it walks forward with
                // each heartbeat), so it would otherwise show up in a day's timeline
                // as a zero-length blip until it closes.
                extra_where: Some("t.is_open = false"),
                use_date_filter: false,
            }),
            continuous_agg: None,
            //                    who  whom what when where why  how
            is_activation_signal: true,
        },
        // Where the human was — the counterpart to app usage, and the reason app
        // usage can finally be honest. Without it, absence is invisible: walking
        // away with an app focused looked identical to using it, and `loginwindow`
        // (the lock screen) arrived as though it were an app, becoming the box's
        // most-used "application" at 211 of 429 hours.
        OntologyDescriptor {
            name: "activity_presence",
            display_name: "Presence",
            description: "Whether you were at the machine: active, watching, idle, locked, asleep",
            domain: "activity",
            table_name: "data_activity_presence",
            source_streams: vec!["stream_mac_presence"],
            timestamp_column: "started_at",
            end_timestamp_column: Some("ended_at"),
            embedding: None,
            // No prose: presence is a state machine (active/idle/locked), and a
            // state name is not a mention of anyone.
            extraction: None,
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "presence",
                source_type_sql: None,
                label_sql: "t.state",
                preview_sql: "NULL",
                id_sql: "t.id",
                extra_where: Some("t.is_open = false"),
                use_date_filter: false,
            }),
            continuous_agg: None,
            //                    who  whom what when where why  how
            is_activation_signal: false,
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
            extraction: None,
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
            is_activation_signal: true,
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
            extraction: None,
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
            is_activation_signal: true,
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
            extraction: None,
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
            is_activation_signal: true,
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
            extraction: Some(ExtractionConfig {
                text_sql: "COALESCE(t.title, '') || E'\n\n' || COALESCE(t.content, '')",
                filter_sql: None,
                // Documents run long, and names cluster at the top. The tail is
                // mostly body text that names nobody new.
                max_chars: 8000,
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
            is_activation_signal: false,
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
            extraction: Some(ExtractionConfig {
                text_sql: "COALESCE(t.content, '')",
                // The user's own turns only. Names in the ASSISTANT's replies are
                // the model paraphrasing the user back at them — extracting from
                // those manufactures evidence for things the user never said, and
                // inflates every mention count with its own echo.
                filter_sql: Some("t.role = 'user'"),
                max_chars: 4000,
            }),
            temporal_type: TemporalType::Discrete,
            day_source: None, // Individual messages not useful as day sources
            continuous_agg: None,
            //                    who  whom what when where why  how
            is_activation_signal: false,
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
            extraction: None,
            temporal_type: TemporalType::Discrete,
            day_source: None, // Not events
            continuous_agg: None,
            //                    who  whom what when where why  how
            is_activation_signal: false,
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
                // `t.category` is JSONB now (a string array). Cast to text and
                // strip the JSON quoting so embeddings see comma-separated
                // category labels rather than literal `["foo","bar"]`.
                embed_text_sql: "COALESCE(t.merchant_name, t.description, '') || ' ' || COALESCE((SELECT string_agg(value::text, ', ') FROM jsonb_array_elements_text(t.category) AS value), '')",
                content_type: "transaction",
                title_sql: Some("COALESCE(t.merchant_name, t.description)"),
                preview_sql: "COALESCE(t.merchant_name, t.description) || ' - $' || CAST(ABS(t.amount / 100.0) AS TEXT) || ' on ' || t.timestamp::text",
                author_sql: None,
                timestamp_sql: "t.timestamp",
            }),
            extraction: None,
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
            is_activation_signal: false,
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
            extraction: None,
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
            is_activation_signal: false,
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
            extraction: None,
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
            is_activation_signal: false,
        },
        OntologyDescriptor {
            name: "app_chat_message",
            display_name: "Chat Messages",
            description: "Individual messages from Virtues AI conversations (search artifact)",
            domain: "app",
            table_name: "app_chat_messages",
            source_streams: vec![],
            timestamp_column: "created_at",
            end_timestamp_column: None,
            embedding: Some(EmbeddingConfig {
                // Skip compaction checkpoints and onboarding triggers: NULL embed
                // text makes the indexer insert a skip placeholder instead of
                // embedding them. User + assistant turns are both embedded.
                embed_text_sql: "CASE WHEN t.role = 'checkpoint' OR t.subject = 'onboarding_synthetic' THEN NULL ELSE t.content END",
                content_type: "chat_message",
                title_sql: None,
                preview_sql: "SUBSTR(t.content, 1, 200)",
                author_sql: Some("t.role"),
                timestamp_sql: "t.created_at",
            }),
            extraction: None,
            temporal_type: TemporalType::Discrete,
            day_source: None,
            continuous_agg: None,
            //                    who  whom what when where why  how
            is_activation_signal: false,
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
            extraction: None,
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
            is_activation_signal: false,
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
/// The ontologies that carry prose worth reading for names.
///
/// The extractor walks THIS, not a hardcoded list of tables. A new source
/// (Slack, Fastmail, an mbox import) normalizes into an existing ontology and
/// inherits extraction for free — there is nothing to add.
pub fn get_extractable_ontologies() -> Vec<OntologyDescriptor> {
    registered_ontologies()
        .into_iter()
        .filter(|o| o.extraction.is_some())
        .collect()
}

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
    fn extractable_ontologies_are_exactly_the_prose_ones() {
        let names: Vec<&str> = get_extractable_ontologies()
            .iter()
            .map(|o| o.table_name)
            .collect();

        // Prose. These four get their own LLM call.
        assert!(names.contains(&"data_communication_email"));
        assert!(names.contains(&"data_communication_message"));
        assert!(names.contains(&"data_content_document"));
        assert!(names.contains(&"data_content_conversation"));

        // Transcriptions carry prose but are NOT here: the transcription action
        // already extracts their entities in a call we're paying for anyway, so
        // they're drained for free. Adding them here would double-bill the same
        // names.
        assert!(!names.contains(&"data_communication_transcription"));

        // Everything else has no free text at all. If this number grows, someone
        // pointed an LLM at heart-rate samples.
        assert_eq!(names.len(), 4, "unexpected extractable ontologies: {names:?}");
    }

    /// The two filters that are load-bearing, not cosmetic.
    #[test]
    fn prose_filters_hold_the_line() {
        let by = |t: &str| {
            registered_ontologies()
                .into_iter()
                .find(|o| o.table_name == t)
                .and_then(|o| o.extraction)
                .expect("has extraction config")
        };

        // Email: Google's own junk classification, not ours. UPDATES stays in —
        // receipts and confirmations are dense with real orgs and future dates.
        let email = by("data_communication_email");
        let f = email.filter_sql.expect("email must filter junk");
        assert!(f.contains("CATEGORY_PROMOTIONS") && f.contains("SPAM"));
        assert!(
            !f.contains("CATEGORY_UPDATES"),
            "UPDATES must NOT be filtered — it is the best structured signal in the mailbox"
        );

        // AI chats: the user's turns only. The assistant's replies are the model
        // paraphrasing the user back; extracting names from them manufactures
        // evidence for things the user never said.
        let convo = by("data_content_conversation");
        assert_eq!(convo.filter_sql, Some("t.role = 'user'"));
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

}

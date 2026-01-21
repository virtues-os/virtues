//! Ontology descriptor types and builder
//!
//! Defines the configuration for each ontology table including:
//! - Table metadata (name, domain)
//! - Embedding configuration for semantic search

/// Embedding configuration for semantic search
#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    /// SQL expression for text to embed
    /// e.g., "COALESCE(subject, '') || '\n\n' || COALESCE(body_plain, '')"
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

/// A registered ontology definition
#[derive(Debug, Clone)]
pub struct Ontology {
    /// Unique ontology name (e.g., "health_sleep", "praxis_calendar")
    pub name: &'static str,

    /// Human-readable display name
    pub display_name: &'static str,

    /// Description of what this ontology stores
    pub description: &'static str,

    /// Domain grouping (e.g., "health", "location", "social")
    pub domain: &'static str,

    /// Database table name (in data schema)
    pub table_name: &'static str,

    /// Source streams that feed into this ontology
    pub source_streams: Vec<&'static str>,

    /// Primary timestamp column for querying "when did this occur/start"
    /// e.g., "timestamp", "start_time", "arrival_time"
    pub timestamp_column: &'static str,

    /// Optional end timestamp column for span/duration events
    /// e.g., "end_time", "departure_time"
    /// None for point-in-time events like heart rate readings
    pub end_timestamp_column: Option<&'static str>,

    /// Embedding configuration for semantic search (None if not searchable)
    pub embedding: Option<EmbeddingConfig>,
}

/// Builder for Ontology
pub struct OntologyBuilder {
    name: &'static str,
    display_name: &'static str,
    description: &'static str,
    domain: &'static str,
    table_name: &'static str,
    source_streams: Vec<&'static str>,
    timestamp_column: &'static str,
    end_timestamp_column: Option<&'static str>,
    embedding: Option<EmbeddingConfig>,
}

impl OntologyBuilder {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            display_name: name,
            description: "",
            domain: "",
            table_name: name,
            source_streams: vec![],
            timestamp_column: "timestamp", // sensible default
            end_timestamp_column: None,
            embedding: None,
        }
    }

    pub fn display_name(mut self, name: &'static str) -> Self {
        self.display_name = name;
        self
    }

    pub fn description(mut self, desc: &'static str) -> Self {
        self.description = desc;
        self
    }

    pub fn domain(mut self, domain: &'static str) -> Self {
        self.domain = domain;
        self
    }

    pub fn table_name(mut self, name: &'static str) -> Self {
        self.table_name = name;
        self
    }

    pub fn source_streams(mut self, streams: Vec<&'static str>) -> Self {
        self.source_streams = streams;
        self
    }

    /// Set the primary timestamp column (when event occurred/started)
    /// Defaults to "timestamp" if not specified
    pub fn timestamp_column(mut self, column: &'static str) -> Self {
        self.timestamp_column = column;
        self
    }

    /// Set the end timestamp column for span/duration events
    /// Leave unset for point-in-time events
    pub fn end_timestamp_column(mut self, column: &'static str) -> Self {
        self.end_timestamp_column = Some(column);
        self
    }

    /// Configure embedding for semantic search
    pub fn embedding(
        mut self,
        embed_text_sql: &'static str,
        content_type: &'static str,
        title_sql: Option<&'static str>,
        preview_sql: &'static str,
        author_sql: Option<&'static str>,
        timestamp_sql: &'static str,
    ) -> Self {
        self.embedding = Some(EmbeddingConfig {
            embed_text_sql,
            content_type,
            title_sql,
            preview_sql,
            author_sql,
            timestamp_sql,
        });
        self
    }

    pub fn build(self) -> Ontology {
        Ontology {
            name: self.name,
            display_name: self.display_name,
            description: self.description,
            domain: self.domain,
            table_name: self.table_name,
            source_streams: self.source_streams,
            timestamp_column: self.timestamp_column,
            end_timestamp_column: self.end_timestamp_column,
            embedding: self.embedding,
        }
    }
}

/// Trait for ontology modules to implement
pub trait OntologyDescriptor {
    /// Get the ontology definition
    fn descriptor() -> Ontology;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ontology_builder() {
        let ontology = OntologyBuilder::new("health_sleep")
            .display_name("Sleep Sessions")
            .description("Sleep tracking from HealthKit")
            .domain("health")
            .table_name("health_sleep")
            .source_streams(vec!["stream_ios_healthkit"])
            .timestamp_column("start_time")
            .end_timestamp_column("end_time")
            .build();

        assert_eq!(ontology.name, "health_sleep");
        assert_eq!(ontology.domain, "health");
        assert_eq!(ontology.timestamp_column, "start_time");
        assert_eq!(ontology.end_timestamp_column, Some("end_time"));
        assert!(ontology.embedding.is_none());
    }

    #[test]
    fn test_ontology_builder_point_event() {
        let ontology = OntologyBuilder::new("health_heart_rate")
            .domain("health")
            .timestamp_column("timestamp")
            // No end_timestamp_column - point event
            .build();

        assert_eq!(ontology.timestamp_column, "timestamp");
        assert_eq!(ontology.end_timestamp_column, None);
    }

    #[test]
    fn test_ontology_with_embedding() {
        let ontology = OntologyBuilder::new("social_email")
            .display_name("Emails")
            .domain("social")
            .timestamp_column("timestamp")
            .embedding(
                "COALESCE(subject, '') || '\n\n' || COALESCE(body_plain, '')",
                "email",
                Some("subject"),
                "COALESCE(LEFT(snippet, 200), LEFT(body_plain, 200), '')",
                Some("from_name"),
                "timestamp",
            )
            .build();

        assert!(ontology.embedding.is_some());
        assert_eq!(ontology.timestamp_column, "timestamp");
        let emb = ontology.embedding.unwrap();
        assert_eq!(emb.content_type, "email");
        assert_eq!(emb.title_sql, Some("subject"));
    }
}

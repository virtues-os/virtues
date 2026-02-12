//! Source and stream registry for catalog/discovery
//!
//! This module provides a compile-time registry of all available sources and streams.
//! The registry is initialized once at startup and contains static metadata about what
//! data sources and streams are available in the system.
//!
//! **Terminology:**
//! - **RegisteredSource**: A source type in the registry (e.g., "Google", "Notion")
//! - **SourceConnection**: A user's connected account instance (stored in `source_connections` table with auth tokens)
//! - **RegisteredStream**: A stream type that a source offers (e.g., "Calendar", "Gmail")
//! - **StreamConnection**: A user's enabled stream with configuration (stored in `stream_connections` table)
//!
//! **Unified Registry:**
//! This registry is the single source of truth for:
//! - UI metadata (names, descriptions, icons, config schemas)
//! - Transform logic (how stream data maps to ontology tables)
//! - Stream creation logic (how to instantiate stream implementations)
//!
//! By unifying metadata and logic in one place, we eliminate:
//! - Large match statements in StreamFactory
//! - Parallel registries that can drift out of sync
//! - Manual consistency maintenance
//!
//! Frontends and CLIs query this registry (via the catalog API) to discover:
//! - What sources are available (Google, Notion, iOS, Mac, Virtues)
//! - What streams each source provides (Calendar, Gmail, Pages, HealthKit, etc.)
//! - What configuration options each stream accepts
//! - What database schema each stream uses

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::error::Result;
use crate::jobs::TransformContext;
use crate::sources::auth::SourceAuth;
use crate::sources::base::OntologyTransform;
use crate::sources::stream_type::StreamType;
use crate::storage::{stream_writer::StreamWriter, Storage};

use virtues_registry::sources::SourceDescriptor;
use virtues_registry::streams::StreamDescriptor;

// Re-export types from shared registry
pub use virtues_registry::sources::{AuthType, OAuthConfig};

/// Type alias for transform creator functions
///
/// A transform creator is a function that takes a TransformContext and returns
/// a boxed OntologyTransform. This allows transforms to be created lazily with
/// the necessary context (API keys, storage, etc.).
pub type TransformCreator = fn(&TransformContext) -> Result<Box<dyn OntologyTransform>>;

/// Type alias for stream creator functions
///
/// A stream creator is a function that takes a StreamFactoryContext and returns
/// a StreamType (either Pull or Push). This allows streams to be created with
/// all necessary dependencies.
pub type StreamCreator = fn(&StreamFactoryContext) -> Result<StreamType>;

/// Context passed to stream creator functions
///
/// This provides all the dependencies needed to create any type of stream.
pub struct StreamFactoryContext {
    /// Source ID (e.g., "source_google-calendar")
    pub source_id: String,
    /// Database connection pool
    pub db: sqlx::SqlitePool,
    /// Storage backend (S3, local, etc.)
    pub storage: Arc<Storage>,
    /// Stream writer for buffering data
    pub stream_writer: Arc<Mutex<StreamWriter>>,
    /// Authentication for this source
    pub auth: SourceAuth,
}

/// A transform mapping from this stream to an ontology table
#[derive(Clone)]
pub struct StreamTransform {
    /// Target ontology table (e.g., "calendar_event", "communication_email")
    pub target_table: &'static str,
    
    /// Function to create the transform instance
    pub creator: TransformCreator,
}

/// A registered source type (e.g., "Google", "Notion")
/// This defines what sources CAN be connected.
/// For actual user connections, see api::SourceConnection.
#[derive(Debug, Clone, Serialize)]
pub struct RegisteredSource {
    /// Metadata from shared registry
    #[serde(flatten)]
    pub descriptor: SourceDescriptor,

    /// Available streams for this source (with implementation)
    pub streams: Vec<RegisteredStream>,
}

// Custom Deserialize implementation for RegisteredSource
impl<'de> Deserialize<'de> for RegisteredSource {
    fn deserialize<D>(_deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Err(serde::de::Error::custom("RegisteredSource cannot be deserialized - it must be constructed at compile time"))
    }
}

/// A registered stream type (e.g., "Calendar", "Gmail")
/// This defines what streams a source offers.
/// For user stream state, see api::StreamConnection.
///
/// This struct unifies the shared metadata with core-specific implementation logic.
#[derive(Clone)]
pub struct RegisteredStream {
    /// Metadata from shared registry
    pub descriptor: StreamDescriptor,

    /// JSON schema for configuration (serialized as JSON)
    pub config_schema: serde_json::Value,

    /// Example configuration
    pub config_example: serde_json::Value,

    /// Transforms that map this stream's data to ontology tables
    /// 
    /// This unifies the catalog metadata with transform logic - the stream
    /// definition now includes how its data should be transformed.
    /// Note: This field is skipped during serialization (handled by custom Serialize impl)
    pub transforms: Vec<StreamTransform>,

    /// Optional factory function to create the stream implementation
    ///
    /// This unifies the stream factory logic with the catalog - no more match
    /// statements in StreamFactory. If None, the stream cannot be instantiated
    /// dynamically (e.g., disabled or not yet implemented).
    pub stream_creator: Option<StreamCreator>,
}

// Custom Debug implementation to skip function pointer fields
impl std::fmt::Debug for RegisteredStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegisteredStream")
            .field("descriptor", &self.descriptor)
            .field("transforms_count", &self.transforms.len())
            .field("has_stream_creator", &self.stream_creator.is_some())
            .finish()
    }
}

// Custom Serialize implementation that skips transforms
impl Serialize for RegisteredStream {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("RegisteredStream", 3)?;
        state.serialize_field("descriptor", &self.descriptor)?;
        state.serialize_field("config_schema", &self.config_schema)?;
        state.serialize_field("config_example", &self.config_example)?;
        state.end()
    }
}

// Custom Deserialize implementation that sets transforms to empty
impl<'de> Deserialize<'de> for RegisteredStream {
    fn deserialize<D>(_deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Err(serde::de::Error::custom("RegisteredStream cannot be deserialized - it must be constructed at compile time"))
    }
}

impl RegisteredStream {
    /// Create a new stream descriptor builder
    pub fn new(name: &'static str) -> StreamBuilder {
        // Find metadata in shared registry
        let descriptor = virtues_registry::streams::registered_streams()
            .into_iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("Stream '{}' not found in virtues-registry", name));

        StreamBuilder {
            descriptor,
            config_schema: serde_json::json!({}),
            config_example: serde_json::json!({}),
            transforms: vec![],
            stream_creator: None,
        }
    }

    /// Find a transform for a specific target ontology table
    pub fn get_transform(&self, target_table: &str) -> Option<&StreamTransform> {
        self.transforms.iter().find(|t| t.target_table == target_table)
    }

    /// Create a transform instance for a specific target ontology table
    pub fn create_transform(
        &self,
        target_table: &str,
        context: &TransformContext,
    ) -> Result<Box<dyn OntologyTransform>> {
        let transform = self.get_transform(target_table).ok_or_else(|| {
            crate::error::Error::InvalidInput(format!(
                "No transform registered for {} -> {}",
                self.descriptor.table_name, target_table
            ))
        })?;
        (transform.creator)(context)
    }

    /// Create a stream instance using the registered creator
    ///
    /// Returns Error if no stream creator is registered.
    pub fn create_stream(&self, context: &StreamFactoryContext) -> Result<StreamType> {
        let creator = self.stream_creator.ok_or_else(|| {
            crate::error::Error::InvalidInput(format!(
                "No stream creator registered for {}",
                self.descriptor.table_name
            ))
        })?;
        creator(context)
    }
}

/// Builder for RegisteredStream
pub struct StreamBuilder {
    descriptor: StreamDescriptor,
    config_schema: serde_json::Value,
    config_example: serde_json::Value,
    transforms: Vec<StreamTransform>,
    stream_creator: Option<StreamCreator>,
}

impl StreamBuilder {
    pub fn config_schema(mut self, schema: serde_json::Value) -> Self {
        self.config_schema = schema;
        self
    }

    pub fn config_example(mut self, example: serde_json::Value) -> Self {
        self.config_example = example;
        self
    }

    /// Add a transform that maps this stream to an ontology table
    pub fn transform(mut self, target_table: &'static str, creator: TransformCreator) -> Self {
        self.transforms.push(StreamTransform {
            target_table,
            creator,
        });
        self
    }

    /// Set the stream creator function for instantiating this stream
    pub fn stream_creator(mut self, creator: StreamCreator) -> Self {
        self.stream_creator = Some(creator);
        self
    }

    pub fn build(self) -> RegisteredStream {
        RegisteredStream {
            descriptor: self.descriptor,
            config_schema: self.config_schema,
            config_example: self.config_example,
            transforms: self.transforms,
            stream_creator: self.stream_creator,
        }
    }
}

/// Trait for sources to register themselves in the catalog
pub trait SourceRegistry {
    /// Get the source descriptor
    fn descriptor() -> RegisteredSource;
}

/// Global registry of all sources
pub struct Registry {
    /// Internal storage for registered sources (public for internal use)
    pub sources: HashMap<String, RegisteredSource>,
}

impl Registry {
    /// Create a new empty registry
    fn new() -> Self {
        Self {
            sources: HashMap::new(),
        }
    }

    /// Register a source
    fn register(&mut self, descriptor: RegisteredSource) {
        self.sources.insert(descriptor.descriptor.name.to_string(), descriptor);
    }

    /// Get all registered sources
    pub fn list_sources(&self) -> Vec<&RegisteredSource> {
        self.sources
            .values()
            .filter(|s| s.descriptor.enabled)
            .collect()
    }

    /// Get a specific source by name
    pub fn get_source(&self, name: &str) -> Option<&RegisteredSource> {
        self.sources.get(name).filter(|s| s.descriptor.enabled)
    }

    /// Get a specific stream from a source
    pub fn get_stream(&self, source_name: &str, stream_name: &str) -> Option<&RegisteredStream> {
        self.sources
            .get(source_name)
            .filter(|source| source.descriptor.enabled)
            .and_then(|source| {
                source
                    .streams
                    .iter()
                    .find(|s| s.descriptor.name == stream_name && s.descriptor.enabled)
            })
    }

    /// List all streams across all sources
    pub fn list_all_streams(&self) -> Vec<(&str, &RegisteredStream)> {
        self.sources
            .values()
            .filter(|source| source.descriptor.enabled)
            .flat_map(|source| {
                source
                    .streams
                    .iter()
                    .filter(|stream| stream.descriptor.enabled)
                    .map(move |stream| (source.descriptor.name, stream))
            })
            .collect()
    }

    /// List all streams including those that are disabled
    pub fn list_all_streams_including_disabled(&self) -> Vec<(&str, &RegisteredStream)> {
        self.sources
            .values()
            .flat_map(|source| {
                source
                    .streams
                    .iter()
                    .map(move |stream| (source.descriptor.name, stream))
            })
            .collect()
    }
}

// Global registry instance (initialized via lazy_static)
use std::sync::OnceLock;

static REGISTRY: OnceLock<Registry> = OnceLock::new();

/// Initialize the global registry (called once at startup)
fn init_registry() -> Registry {
    let mut registry = Registry::new();

    // Register OAuth sources
    registry.register(crate::sources::github::registry::GitHubSource::descriptor());
    registry.register(crate::sources::google::registry::GoogleSource::descriptor());
    registry.register(crate::sources::notion::registry::NotionSource::descriptor());
    registry.register(crate::sources::plaid::registry::PlaidSource::descriptor());
    registry.register(crate::sources::strava::registry::StravaSource::descriptor());

    // Register device sources
    registry.register(crate::sources::ios::registry::IosSource::descriptor());
    registry.register(crate::sources::mac::registry::MacSource::descriptor());

    registry
}

/// Get the global registry
pub fn registry() -> &'static Registry {
    REGISTRY.get_or_init(init_registry)
}

/// List all available sources
pub fn list_sources() -> Vec<&'static RegisteredSource> {
    registry().list_sources()
}

/// Get information about a specific source
pub fn get_source(name: &str) -> Option<&'static RegisteredSource> {
    registry().get_source(name)
}

/// Get information about a specific stream
pub fn get_stream(source_name: &str, stream_name: &str) -> Option<&'static RegisteredStream> {
    registry().get_stream(source_name, stream_name)
}

/// List all streams across all sources
pub fn list_all_streams() -> Vec<(&'static str, &'static RegisteredStream)> {
    registry().list_all_streams()
}

/// List all streams including those that are disabled
pub fn list_all_streams_including_disabled() -> Vec<(&'static str, &'static RegisteredStream)> {
    registry().list_all_streams_including_disabled()
}

/// Get a stream by its table name (e.g., "stream_google_calendar")
///
/// Returns the source name and stream reference if found.
pub fn get_stream_by_table_name(
    table_name: &str,
) -> Option<(&'static str, &'static RegisteredStream)> {
    registry()
        .list_all_streams()
        .into_iter()
        .find(|(_, stream)| stream.descriptor.table_name == table_name)
}

/// Get a stream by its table name, including disabled streams
///
/// This is useful for transform lookups where we need to find the transform
/// even if the stream is disabled.
pub fn get_stream_by_table_name_including_disabled(
    table_name: &str,
) -> Option<(&'static str, &'static RegisteredStream)> {
    registry()
        .list_all_streams_including_disabled()
        .into_iter()
        .find(|(_, stream)| stream.descriptor.table_name == table_name)
}

/// Find and create a transform for the given source and target tables
///
/// This is the unified transform lookup that uses the registry as the single
/// source of truth for both metadata and transform logic.
///
/// # Arguments
/// * `source_table` - The stream table name (e.g., "stream_google_calendar")
/// * `target_table` - The ontology table name (e.g., "calendar_event")
/// * `context` - Transform context with dependencies
///
/// # Returns
/// A boxed OntologyTransform ready to execute
pub fn find_transform(
    source_table: &str,
    target_table: &str,
    context: &TransformContext,
) -> Result<Box<dyn OntologyTransform>> {
    // Look up the stream by its table name (including disabled streams for transforms)
    let (_, stream) = get_stream_by_table_name_including_disabled(source_table).ok_or_else(|| {
        crate::error::Error::InvalidInput(format!(
            "No registered stream with table_name '{}'",
            source_table
        ))
    })?;

    // Create the transform using the stream's registered creator
    stream.create_transform(target_table, context)
}

/// Normalize stream name from short form to full table name
///
/// ## Naming Convention
///
/// The system uses a three-tier naming architecture:
/// 1. **Stream Name** (registered in data.streams) - e.g., "app_export", "gmail"
/// 2. **Stream Table** (object storage) - e.g., "stream_virtues_ai_chat", "stream_google_gmail"
/// 3. **Ontology Table** (data schema) - e.g., "content_conversation", "communication_email"
///
/// This function maps stream names (tier 1) to stream tables (tier 2).
///
/// ## Usage
///
/// If the name already starts with "stream_", it is returned as-is.
/// Otherwise, short names are expanded to their full stream table names.
pub fn normalize_stream_name(name: &str) -> String {
    if name.starts_with("stream_") {
        return name.to_string();
    }

    // Try to find by short name in registry
    for (_source_name, stream) in list_all_streams() {
        if stream.descriptor.name == name {
            return stream.descriptor.table_name.to_string();
        }
    }

    // Return as-is if not found (caller may have passed a full table name)
    tracing::debug!(
        "Stream name '{}' not found in registry, returning as-is",
        name
    );
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_initialization() {
        let sources = list_sources();
        assert!(!sources.is_empty(), "Registry should have sources");
    }

    #[test]
    fn test_get_source() {
        let google = get_source("google");
        assert!(google.is_some(), "Should find Google source");

        if let Some(g) = google {
            assert_eq!(g.descriptor.name, "google");
            assert!(!g.streams.is_empty(), "Google should have streams");
        }
    }

    #[test]
    fn test_get_stream() {
        let calendar = get_stream("google", "calendar");
        assert!(calendar.is_some(), "Should find Google Calendar stream");

        if let Some(cal) = calendar {
            assert_eq!(cal.descriptor.name, "calendar");
            assert_eq!(cal.descriptor.table_name, "stream_google_calendar");
        }
    }

    #[test]
    fn test_list_all_streams() {
        let streams = list_all_streams();
        assert!(!streams.is_empty(), "Should have streams registered");

        // Check that stream names are properly formatted
        for (source, stream) in streams {
            assert!(stream.descriptor.table_name.starts_with("stream_"));
            assert!(stream.descriptor.table_name.contains(source));
        }
    }
}

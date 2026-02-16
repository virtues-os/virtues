//! Stream factory for creating stream instances at runtime
//!
//! This module provides a unified way to instantiate any stream based on
//! source type and stream name, handling authentication and configuration loading.
//!
//! The factory now delegates to the unified registry for stream creation,
//! eliminating the need for large match statements. Each stream registers
//! its creator function alongside its metadata.

use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::base::TokenManager;
use crate::error::{Error, Result};
use crate::registry::StreamFactoryContext;
use crate::storage::{stream_writer::StreamWriter, Storage};

use super::{auth::SourceAuth, stream_type::StreamType};

/// Factory for creating stream instances
///
/// The StreamFactory handles:
/// - Loading source information from the database
/// - Creating appropriate authentication (OAuth2, Device, etc.)
/// - Instantiating the correct stream implementation
///
/// # Example
///
/// ```rust
/// let factory = StreamFactory::new(db.clone(), storage.clone(), stream_writer.clone());
///
/// // Create a Google Calendar stream (returns StreamType::Pull)
/// let mut stream_type = factory.create_stream_typed(source_id, "calendar").await?;
///
/// // Get mutable access to the PullStream
/// if let Some(pull_stream) = stream_type.as_pull_mut() {
///     pull_stream.load_config(&db, source_id).await?;
///     let result = pull_stream.sync_pull(SyncMode::incremental(None)).await?;
/// }
/// ```
pub struct StreamFactory {
    db: SqlitePool,
    storage: Arc<Storage>,
    stream_writer: Arc<Mutex<StreamWriter>>,
}

impl StreamFactory {
    /// Create a new stream factory
    pub fn new(
        db: SqlitePool,
        storage: Arc<Storage>,
        stream_writer: Arc<Mutex<StreamWriter>>,
    ) -> Self {
        Self {
            db,
            storage,
            stream_writer,
        }
    }

    /// Create a stream instance with the new StreamType enum
    ///
    /// This looks up the source in the database, creates the appropriate
    /// authentication, and instantiates the correct stream implementation
    /// wrapped in either Pull or Push variant.
    ///
    /// # Arguments
    /// * `source_id` - Semantic ID of the source (e.g., "source_google-calendar")
    /// * `stream_name` - Name of the stream (e.g., "calendar", "gmail", "apps")
    ///
    /// # Returns
    /// A StreamType enum (either Pull or Push) containing the stream
    ///
    /// # Errors
    /// - If the source doesn't exist
    /// - If the source type is unknown
    /// - If the stream name is not supported for this source
    pub async fn create_stream_typed(
        &self,
        source_id: &str,
        stream_name: &str,
    ) -> Result<StreamType> {
        // Load source info from database
        let source = self.load_source(source_id).await?;

        // Validate against registry (ensures source and stream are enabled)
        let _stream_desc = crate::registry::get_stream(&source.source, stream_name).ok_or_else(|| {
            Error::Other(format!(
                "Stream {}/{} not found or disabled in registry",
                source.source, stream_name
            ))
        })?;

        // Create auth abstraction
        let auth = self.create_auth(source_id, &source.source).await?;

        // Create the appropriate stream implementation
        self.create_stream_typed_impl(source_id, &source.source, stream_name, auth)
            .await
    }

    /// Load source information from the database
    async fn load_source(&self, source_id: &str) -> Result<SourceInfo> {
        let result = sqlx::query_as::<_, (String, String)>(
            "SELECT source, name FROM elt_source_connections WHERE id = $1 AND is_active = true",
        )
        .bind(source_id)
        .fetch_optional(&self.db)
        .await?;

        match result {
            Some((source, name)) => Ok(SourceInfo { source, name }),
            None => Err(Error::Database(format!(
                "Source not found or inactive: {}",
                source_id
            ))),
        }
    }

    /// Create authentication for a source
    async fn create_auth(&self, source_id: &str, provider: &str) -> Result<SourceAuth> {
        match provider {
            "github" | "google" | "notion" | "plaid" | "spotify" | "strava" => {
                // OAuth2 sources - create TokenManager for token refresh
                let token_manager = Arc::new(TokenManager::new(self.db.clone())?);
                Ok(SourceAuth::oauth2(source_id.to_string(), token_manager))
            }
            "ios" | "mac" => {
                // Device sources don't use traditional auth - they push data
                // The device_id is the source name
                let source = self.load_source(source_id).await?;
                Ok(SourceAuth::device(source.name))
            }
            _ => Err(Error::Other(format!("Unknown provider: {}", provider))),
        }
    }

    /// Create the stream implementation with StreamType enum
    ///
    /// This method now delegates to the unified registry for stream creation.
    /// Each stream registers its creator function alongside its metadata,
    /// eliminating the need for large match statements.
    ///
    /// All streams now implement either PullStream (backend-initiated) or
    /// PushStream (client-initiated) traits for clear architectural boundaries.
    async fn create_stream_typed_impl(
        &self,
        source_id: &str,
        provider: &str,
        stream_name: &str,
        auth: SourceAuth,
    ) -> Result<StreamType> {
        // Look up the stream in the unified registry (including disabled for this internal call)
        let stream_desc = crate::registry::registry()
            .sources
            .get(provider)
            .and_then(|source| source.streams.iter().find(|s| s.descriptor.name == stream_name))
            .ok_or_else(|| {
                Error::Other(format!(
                    "Stream {}/{} not found in registry",
                    provider, stream_name
                ))
            })?;

        // Check if the stream has a creator registered
        if let Some(creator) = stream_desc.stream_creator {
            // Use the unified registry's stream creator
            let context = StreamFactoryContext {
                source_id: source_id.to_string(),
                db: self.db.clone(),
                storage: self.storage.clone(),
                stream_writer: self.stream_writer.clone(),
                auth,
            };
            return creator(&context);
        }

        // Fallback: Stream doesn't have a creator registered yet
        Err(Error::Other(format!(
            "Stream {}/{} has no creator registered in the unified registry",
            provider, stream_name
        )))
    }
}

impl Clone for StreamFactory {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            storage: self.storage.clone(),
            stream_writer: self.stream_writer.clone(),
        }
    }
}

/// Source information loaded from database
struct SourceInfo {
    source: String,
    name: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::stream_writer::StreamWriter;

    #[tokio::test]
    async fn test_factory_creation() {
        let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        let storage = Arc::new(Storage::local("./test_data".to_string()).unwrap());
        let stream_writer = Arc::new(Mutex::new(StreamWriter::new()));
        let _factory = StreamFactory::new(pool, storage, stream_writer);
    }

    #[tokio::test]
    async fn test_factory_clone() {
        let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        let storage = Arc::new(Storage::local("./test_data".to_string()).unwrap());
        let stream_writer = Arc::new(Mutex::new(StreamWriter::new()));
        let factory = StreamFactory::new(pool, storage, stream_writer);
        let _factory2 = factory.clone();
    }

    #[tokio::test]
    async fn test_create_auth_device() {
        let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        let storage = Arc::new(Storage::local("./test_data".to_string()).unwrap());
        let stream_writer = Arc::new(Mutex::new(StreamWriter::new()));
        let factory = StreamFactory::new(pool, storage, stream_writer);

        // Note: This will fail without a source in the database
        // That's expected - this tests the code path, not the database
        let result = factory.create_auth("source_ios-test", "ios").await;

        // Should return error because source doesn't exist
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_auth_unknown_source() {
        let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        let storage = Arc::new(Storage::local("./test_data".to_string()).unwrap());
        let stream_writer = Arc::new(Mutex::new(StreamWriter::new()));
        let factory = StreamFactory::new(pool, storage, stream_writer);

        let result = factory
            .create_auth("source_unknown-test", "unknown_source")
            .await;
        assert!(result.is_err());

        match result {
            Err(Error::Other(msg)) => assert!(msg.contains("Unknown provider")),
            _ => panic!("Expected Error::Other"),
        }
    }

    #[tokio::test]
    async fn test_create_stream_typed_impl_google_calendar() {
        let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        let storage = Arc::new(Storage::local("./test_data".to_string()).unwrap());
        let stream_writer = Arc::new(Mutex::new(StreamWriter::new()));
        let factory = StreamFactory::new(pool.clone(), storage, stream_writer.clone());

        let source_id = "source_google-calendar";
        let tm = Arc::new(TokenManager::new_insecure(pool));
        let auth = SourceAuth::oauth2(source_id.to_string(), tm);

        let result = factory
            .create_stream_typed_impl(source_id, "google", "calendar", auth)
            .await;

        assert!(result.is_ok());
        let stream_type = result.unwrap();
        assert_eq!(stream_type.stream_name(), "calendar");
        assert_eq!(stream_type.source_name(), "google");
        assert_eq!(stream_type.table_name(), "stream_google_calendar");
        assert!(stream_type.is_pull());
    }

    #[tokio::test]
    async fn test_create_stream_typed_impl_unknown_stream() {
        let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        let storage = Arc::new(Storage::local("./test_data".to_string()).unwrap());
        let stream_writer = Arc::new(Mutex::new(StreamWriter::new()));
        let factory = StreamFactory::new(pool.clone(), storage, stream_writer.clone());

        let source_id = "source_google-test";
        let tm = Arc::new(TokenManager::new_insecure(pool));
        let auth = SourceAuth::oauth2(source_id.to_string(), tm);

        let result = factory
            .create_stream_typed_impl(source_id, "google", "nonexistent", auth)
            .await;

        assert!(result.is_err());
        match result {
            Err(Error::Other(msg)) => assert!(
                msg.contains("not found in registry"),
                "Expected 'not found in registry' error, got: {}",
                msg
            ),
            _ => panic!("Expected Error::Other"),
        }
    }
}

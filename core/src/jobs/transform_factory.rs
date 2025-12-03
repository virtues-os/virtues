//! Transform factory for dynamic transform instantiation
//!
//! This module provides centralized transform construction logic using the
//! self-registering transform system. Transforms register themselves via the
//! `inventory` crate, eliminating hardcoded match statements.
//!
//! ## Adding a New Transform
//!
//! To add a new transform, simply implement `TransformRegistration` in your
//! transform file and register it with `inventory::submit!`. No changes to
//! this factory are needed.
//!
//! ```ignore
//! // In your transform file:
//! struct MyTransformRegistration;
//!
//! impl TransformRegistration for MyTransformRegistration {
//!     fn source_table(&self) -> &'static str { "stream_my_source" }
//!     fn target_table(&self) -> &'static str { "my_target_ontology" }
//!     fn create(&self, context: &TransformContext) -> Result<Box<dyn OntologyTransform>> {
//!         Ok(Box::new(MyTransform))
//!     }
//! }
//!
//! inventory::submit! { &MyTransformRegistration as &dyn TransformRegistration }
//! ```

use crate::error::Result;
use crate::jobs::transform_context::TransformContext;
use crate::sources::base::{find_transform, OntologyTransform};

/// Transform factory for creating transform instances
pub struct TransformFactory;

impl TransformFactory {
    /// Create a transform instance for the given source and target tables
    ///
    /// Uses the self-registering transform system to find and instantiate
    /// the appropriate transform. Transforms register themselves via the
    /// `inventory` crate in their respective source files.
    ///
    /// # Arguments
    /// * `source_table` - Source stream table name (e.g., "stream_ios_location")
    /// * `target_table` - Target ontology table name (e.g., "location_point")
    /// * `context` - Transform context with dependencies (storage, API keys)
    ///
    /// # Returns
    /// A boxed transform instance ready to execute
    pub fn create(
        source_table: &str,
        target_table: &str,
        context: &TransformContext,
    ) -> Result<Box<dyn OntologyTransform>> {
        find_transform(source_table, target_table, context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::ApiKeys;
    use crate::storage::Storage;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    fn create_test_context() -> TransformContext {
        let storage = Storage::local("/tmp/test".to_string()).unwrap();
        let stream_writer = Arc::new(Mutex::new(
            crate::storage::stream_writer::StreamWriter::new(),
        ));
        let api_keys = ApiKeys::from_env();

        TransformContext::new(Arc::new(storage), stream_writer, api_keys)
    }

    #[test]
    fn test_create_gmail_transform() {
        let context = create_test_context();
        let result = TransformFactory::create("stream_google_gmail", "social_email", &context);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_location_transform() {
        let context = create_test_context();
        let result = TransformFactory::create("stream_ios_location", "location_point", &context);
        assert!(result.is_ok());
    }

    #[test]
    fn test_unknown_transform() {
        let context = create_test_context();
        let result = TransformFactory::create("invalid_source", "invalid_target", &context);
        assert!(result.is_err());
    }
}

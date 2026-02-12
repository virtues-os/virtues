//! Ontology transformation trait and types
//!
//! Defines the interface for transforming raw stream data into normalized ontology tables.
//! Transformations are idempotent and track progress for incremental processing.
//!
//! ## Self-Registration
//!
//! Transforms register themselves at compile time using the `inventory` crate.
//! Each transform file calls `inventory::submit!` with a `TransformRegistration`
//! implementation, eliminating the need for a central hardcoded factory.
//!
//! ```ignore
//! inventory::submit! {
//!     &GmailTransformRegistration as &dyn TransformRegistration
//! }
//! ```

use async_trait::async_trait;

use crate::database::Database;
use crate::error::Result;
use crate::jobs::TransformContext;

/// Result of a transformation operation
#[derive(Debug, Clone)]
pub struct TransformResult {
    /// Number of source records read/examined
    pub records_read: usize,

    /// Number of ontology records successfully written
    pub records_written: usize,

    /// Number of records that failed transformation
    pub records_failed: usize,

    /// ID of last successfully processed source record (for cursor-based iteration)
    pub last_processed_id: Option<String>,

    /// Follow-up transform configurations for chained transforms
    /// Each tuple is: (source_table, target_tables, domain, source_record_id, transform_stage)
    pub chained_transforms: Vec<ChainedTransform>,
}

/// Configuration for a chained transform
#[derive(Debug, Clone)]
pub struct ChainedTransform {
    pub source_table: String,
    pub target_tables: Vec<String>,
    pub domain: String,
    pub source_record_id: String,
    pub transform_stage: String,
}

/// Trait for transforming stream data into ontology tables
///
/// Each source stream (e.g., Gmail, iMessage) implements this trait to define
/// how its data maps to normalized ontology tables (e.g., communication_email).
///
/// # Example
///
/// ```ignore
/// use async_trait::async_trait;
/// use crate::sources::base::{OntologyTransform, TransformResult};
///
/// pub struct GmailEmailTransform;
///
/// #[async_trait]
/// impl OntologyTransform for GmailEmailTransform {
///     fn source_table(&self) -> &str { "stream_google_gmail" }
///     fn target_table(&self) -> &str { "communication_email" }
///     fn domain(&self) -> &str { "social" }
///
///     async fn transform(&self, db: &Database, source_id: Uuid) -> Result<TransformResult> {
///         // Read from stream_google_gmail
///         // Transform to communication_email schema
///         // Write to communication_email table
///         // Return stats
///     }
/// }
/// ```
#[async_trait]
pub trait OntologyTransform: Send + Sync {
    /// Source stream table name (e.g., "stream_google_gmail")
    fn source_table(&self) -> &str;

    /// Target ontology table name (e.g., "communication_email")
    fn target_table(&self) -> &str;

    /// Domain of the ontology (e.g., "social", "health", "activity")
    fn domain(&self) -> &str;

    /// Transform records from source stream to ontology table
    ///
    /// This method should:
    /// 1. Read from S3 via StreamReader with checkpoint tracking
    /// 2. Map fields from source schema to ontology schema
    /// 3. Insert into ontology table
    /// 4. Update checkpoint after successful processing
    /// 5. Handle errors gracefully and continue processing
    /// 6. Return statistics about the transformation
    ///
    /// **Idempotency**: This uses checkpoint-based processing. StreamReader tracks
    /// progress and only returns new records since last successful transform.
    ///
    /// # Arguments
    ///
    /// * `db` - Database connection
    /// * `context` - Transform context with StreamReader and other dependencies
    /// * `source_id` - ID of the source (for filtering stream data)
    ///
    /// # Returns
    ///
    /// Result containing transformation statistics
    async fn transform(
        &self,
        db: &Database,
        context: &crate::jobs::transform_context::TransformContext,
        source_id: String,
    ) -> Result<TransformResult>;
}

/// Registration for self-registering transforms
///
/// Each transform implements this trait and submits itself to the inventory.
/// The transform factory then iterates over all registered transforms.
///
/// # Example
///
/// ```ignore
/// struct GmailTransformRegistration;
///
/// impl TransformRegistration for GmailTransformRegistration {
///     fn source_table(&self) -> &'static str { "stream_google_gmail" }
///     fn target_table(&self) -> &'static str { "communication_email" }
///     fn create(&self, _context: &TransformContext) -> Result<Box<dyn OntologyTransform>> {
///         Ok(Box::new(GmailEmailTransform))
///     }
/// }
///
/// inventory::submit! {
///     &GmailTransformRegistration as &dyn TransformRegistration
/// }
/// ```
pub trait TransformRegistration: Send + Sync {
    /// Source stream table this transform reads from
    fn source_table(&self) -> &'static str;

    /// Target ontology table this transform writes to
    fn target_table(&self) -> &'static str;

    /// Create a transform instance
    ///
    /// Some transforms are stateless, others require context (API keys, storage).
    fn create(&self, context: &TransformContext) -> Result<Box<dyn OntologyTransform>>;
}

// Collect all registered transforms at compile time
inventory::collect!(&'static dyn TransformRegistration);

/// Get all registered transforms
pub fn registered_transforms() -> impl Iterator<Item = &'static &'static dyn TransformRegistration>
{
    inventory::iter::<&'static dyn TransformRegistration>()
}

/// Find a registered transform by source and target tables
///
/// This function first checks the unified registry (which contains both
/// stream metadata and transform logic in one place). If not found there,
/// it falls back to the legacy inventory-based registration for backward
/// compatibility.
pub fn find_transform(
    source_table: &str,
    target_table: &str,
    context: &TransformContext,
) -> Result<Box<dyn OntologyTransform>> {
    // Primary: Try the unified registry first
    if let Ok(transform) = crate::registry::find_transform(source_table, target_table, context) {
        return Ok(transform);
    }

    // Fallback: Check inventory-based registration for backward compatibility
    for registration in registered_transforms() {
        if registration.source_table() == source_table
            && registration.target_table() == target_table
        {
            return registration.create(context);
        }
    }

    Err(crate::error::Error::InvalidInput(format!(
        "No registered transform for {} -> {}",
        source_table, target_table
    )))
}

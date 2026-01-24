//! Ontology definitions and registry
//!
//! This module provides a unified registry for all ontology tables in the system.
//! Each ontology defines:
//! - Table schema and metadata
//! - Embedding configuration for semantic search
//!
//! ## Architecture
//!
//! Ontologies are organized by domain:
//! - `health/` - Heart rate, HRV, sleep, steps, workouts
//! - `location/` - GPS points, clustered visits
//! - `social/` - Email, messages
//! - `calendar/` - Calendar events
//! - `activity/` - App usage, web browsing
//! - `speech/` - Transcriptions
//! - `knowledge/` - Documents, AI conversations
//!
//! ## Adding a New Ontology
//!
//! 1. Create the table in a migration (e.g., `migrations/00X_*.sql`)
//! 2. Create the ontology module (e.g., `ontologies/health/sleep.rs`)
//! 3. Implement `OntologyDescriptor` trait
//! 4. Register in the domain's `registry.rs`

pub mod descriptor;
pub mod registry;

// Domain modules
pub mod activity;
pub mod financial;
pub mod health;
pub mod knowledge;
pub mod location;
pub mod calendar;
pub mod social;
pub mod speech;
pub mod device;
pub mod environment;

// Re-export main types
pub use descriptor::{EmbeddingConfig, Ontology, OntologyBuilder, OntologyDescriptor};
pub use registry::{get_ontology, list_ontologies, ontology_registry, OntologyRegistry};

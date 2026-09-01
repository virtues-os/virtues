//! Virtues Registry - Shared static configuration data
//!
//! This crate is the single source of truth for all static configuration:
//! - Models (LLM providers and their capabilities)
//! - Agents (assistant personas)
//! - Tools (built-in capabilities like web_search, query_ontology)
//! - Ontologies (normalized data schemas)
//!
//! Source/stream catalog data lives in `actions/` (TOML manifests reconciled
//! into the `app_applets` table), not here — the former `sources`/`streams`
//! modules were removed as dead code.
//!
//! # Design Principles
//!
//! 1. **Registry = Static Data**: All data is compile-time constants
//! 2. **Not in the database**: These are compile-time constants, read directly from functions
//! 3. **Shared**: Used by Core, virtues-api, and other services
//!
//! # Tool Types
//!
//! There are two types of tools:
//! - **Built-in tools** (this registry): web_search, query_ontology, semantic_search

pub mod assistant;
pub mod models;
pub mod ontologies;
pub mod personas;
pub mod tools;

// Re-export main types for convenience
pub use assistant::{assistant_profile_defaults, AssistantProfileDefaults, DEFAULT_THEME};
pub use models::{default_model_for_slot, required_model_ids, ModelSlot};
pub use ontologies::{registered_ontologies, EmbeddingConfig, OntologyDescriptor};
pub use personas::{default_personas, get_persona, PersonaConfig};
pub use tools::{default_tools, ToolConfig};

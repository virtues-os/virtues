//! Virtues Registry - Shared static configuration data
//!
//! This crate is the single source of truth for all static configuration:
//! - Models (LLM providers and their capabilities)
//! - Agents (assistant personas)
//! - Tools (built-in capabilities like web_search, query_ontology)
//! - Sources (data sources like Google, iOS, Mac)
//! - Streams (data streams like calendar, healthkit)
//! - Ontologies (normalized data schemas)
//!
//! # Design Principles
//!
//! 1. **Registry = Static Data**: All data is compile-time constants
//! 2. **No SQLite**: These are not stored in database, read directly from functions
//! 3. **Shared**: Used by Core, Tollbooth, and other services
//!
//! # Tool Types
//!
//! There are two types of tools:
//! - **Built-in tools** (this registry): web_search, query_ontology, semantic_search
//! - **MCP tools** (SQLite `app_mcp_tools`): dynamically discovered from connected MCP servers

pub mod agents;
pub mod assistant;
pub mod models;
pub mod ontologies;
pub mod personas;
pub mod sources;
pub mod streams;
pub mod tools;

// Re-export main types for convenience
pub use agents::{default_agents, AgentConfig};
pub use assistant::{assistant_profile_defaults, AssistantProfileDefaults};
pub use models::{default_models, ModelConfig};
pub use ontologies::{registered_ontologies, EmbeddingConfig, OntologyDescriptor};
pub use sources::{
    get_connection_limit, get_source, is_multi_instance, registered_sources, AuthType,
    ConnectionLimits, ConnectionPolicy, OAuthConfig, SourceDescriptor, SourceTier,
};
pub use streams::{registered_streams, StreamDescriptor};
pub use personas::{default_personas, get_persona, PersonaConfig};
pub use tools::{default_tools, ToolConfig};

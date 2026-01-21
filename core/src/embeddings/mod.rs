//! Local embedding generation via Ollama
//!
//! Provides semantic embedding generation for ontology tables using local
//! Ollama models (e.g., nomic-embed-text). Used for semantic search across
//! emails, messages, calendar events, and AI conversations.

mod client;
mod job;

pub use client::{format_embedding, EmbeddingClient};
pub use job::{EmbeddingJob, EmbeddingJobResult, EmbeddingStats, TableEmbeddingStats};

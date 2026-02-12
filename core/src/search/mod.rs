//! Semantic search module
//!
//! Provides vector-based semantic search over user data using:
//! - fastembed (nomic-embed-text-v1.5) for local embedding
//! - sqlite-vec for vector storage and cosine similarity search
//!
//! # Architecture
//!
//! - `embedder.rs`  - Embedder trait + LocalEmbedder (fastembed/ONNX)
//! - `indexer.rs`   - Background job for embedding new records
//! - `query.rs`     - Vector search engine (query embedding + sqlite-vec lookup)
//! - `reranker.rs`  - Cross-encoder reranker (BGE-reranker-v2-m3)

pub mod embedder;
pub mod indexer;
pub mod query;
pub mod reranker;

pub use embedder::{get_embedder, Embedder, LocalEmbedder};
pub use indexer::run_embedding_job;
pub use query::SemanticSearchEngine;
pub use reranker::{get_reranker, LocalReranker};

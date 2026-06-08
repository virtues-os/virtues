//! Semantic search module
//!
//! Provides vector-based semantic search over user data using:
//! - ORT + tokenizers (embeddinggemma-300m, int8/fp16 per accelerator) for local embedding
//! - ORT + tokenizers (jina-reranker-v2-base-multilingual) for reranking
//! - pgvector (`search_vectors.embedding vector(768)` + HNSW cosine index)
//!   for ANN retrieval
//!
//! # Architecture
//!
//! - `accelerator.rs` - Hardware detection + the EP/precision policy (the brain)
//! - `model_cache.rs` - First-boot HF download + on-disk cache (precision-aware)
//! - `ort_runtime.rs` - Shared ONNX Runtime session construction (EP selection)
//! - `embedder.rs`    - Embedder trait + LocalEmbedder
//! - `indexer.rs`     - Background job for embedding new records
//! - `query.rs`       - Vector search engine (query embedding + pgvector lookup)
//! - `reranker.rs`    - Cross-encoder reranker

pub mod accelerator;
pub mod embedder;
pub mod indexer;
pub mod model_cache;
pub mod ort_runtime;
pub mod query;
pub mod reranker;

pub use embedder::{get_embedder, Embedder, LocalEmbedder};
pub use indexer::run_embedding_job;
pub use query::SemanticSearchEngine;
pub use reranker::{get_reranker, LocalReranker, RerankScore};

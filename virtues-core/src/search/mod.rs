//! Semantic search module.
//!
//! v0.1.0 routes all local ML through a separate Ollama daemon (see
//! `embedder.rs`). The previous ORT-in-process stack — accelerator
//! detection, model cache, ONNX session construction, cross-encoder
//! reranker — was removed because ORT 1.24's prebuilt binaries require
//! glibc 2.38+ which Jetson JetPack 6.x doesn't ship. Ollama owns its
//! own GPU/CPU detection and model pulls, so this module is now a thin
//! Rust shim over its HTTP API.
//!
//! # Architecture
//!
//! - `embedder.rs` - Ollama HTTP client (text → 1024-dim vector via bge-m3)
//! - `indexer.rs`  - Background job that embeds new records
//! - `query.rs`    - Vector search (query embedding + pgvector ANN lookup)
//! - `reranker.rs` - v0.1.0 stub; search auto-falls-back to bi-encoder cosine

pub mod embedder;
pub mod indexer;
pub mod model_cache;
pub mod query;
pub mod reranker;

pub use embedder::{get_embedder, Embedder, LocalEmbedder};
pub use indexer::run_embedding_job;
pub use query::SemanticSearchEngine;
pub use reranker::{get_reranker, LocalReranker, RerankScore};

//! Semantic search module.
//!
//! v0.1.1 routes all local ML through two `llama-server` sidecars that the
//! installer ships, pins, and runs as systemd units (see `embedder.rs` for
//! the full lineage: in-process ORT died on glibc 2.38+ vs JetPack 6.x's
//! 2.35; the v0.1.0 Ollama detour died on the missing rerank endpoint).
//! llama.cpp is compiled per-arch in our own CI — CUDA for the Jetson
//! appliance, CPU for the DIY floor — so this module stays a thin Rust
//! shim over loopback HTTP, with zero inference dependencies in-process.
//!
//! # Architecture
//!
//! - `embedder.rs` - sidecar client, :18181 (text → 256-dim vector, EmbeddingGemma-300M)
//! - `indexer.rs`  - Background job that embeds new records
//! - `query.rs`    - Vector search (query embedding + pgvector ANN lookup)
//! - `reranker.rs` - sidecar client, :18182 (cross-encoder, gte-reranker-modernbert-base;
//!   search falls back to bi-encoder cosine if it's down)

pub mod bm25;
pub mod embedder;
pub mod indexer;
pub mod qnn_client;
pub mod query;
pub mod reranker;

pub use embedder::{get_embedder, Embedder, LocalEmbedder};
pub use indexer::run_embedding_job;
pub use query::SemanticSearchEngine;
pub use reranker::{get_reranker, LocalReranker, RerankScore};

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
//! - `embedder.rs` - inference-contract client, :18181 (`/v1/embeddings`; the
//!   endpoint may be llama-server + EmbeddingGemma, the Dragon NPU daemon
//!   serving gte-small, or any BYO OpenAI-compatible server — one path for all)
//! - `indexer.rs`  - Background job that embeds new records
//! - `query.rs`    - Hybrid retrieval: dense ANN ⊕ BM25, z-fused with a
//!   query-adaptive weight, RRF across phrasings, then a conditional rerank
//! - `reranker.rs` - inference-contract client, :18182 (`/v1/rerank`; the
//!   endpoint may be the Dragon NPU daemon serving answerai-colbert-small-v1@256,
//!   llama-server + gte-reranker-modernbert-base, or any BYO server speaking the
//!   same contract — one path for all). If it is down, search falls back to the
//!   FUSED HYBRID ranking, not to "bi-encoder cosine": the dense arm is only one
//!   of the two arms that produced the order being kept.

pub mod bm25;
pub mod embedder;
pub mod indexer;
pub mod query;
pub mod reranker;

pub use embedder::{get_embedder, Embedder, LocalEmbedder};
pub use indexer::run_embedding_job;
pub use query::{ScopeMode, SemanticSearchEngine};
pub use reranker::{get_reranker, LocalReranker, RerankScore};

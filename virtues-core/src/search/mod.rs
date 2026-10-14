//! Semantic search module.
//!
//! All local ML leaves this module over loopback HTTP, to one embedding
//! endpoint (:18181 by default). What is listening there is not this module's
//! business: the `llama-server` sidecar the installer ships, pins and runs as a
//! systemd unit; `virtues-qnnd` on the Hexagon NPU on Dragon; a BYO server
//! anywhere else. One contract, several backings — the per-flavor detail lives
//! in `embedder.rs`.
//!
//! The point of the seam is that it has zero inference dependencies in-process:
//! whoever answers owns the model, the accelerator and the threading.
//!
//! # Architecture
//!
//! - `embedder.rs` - inference-contract client, :18181 (`/v1/embeddings`; the
//!   endpoint may be llama-server + EmbeddingGemma, the Dragon NPU daemon
//!   serving gte-small, or any BYO OpenAI-compatible server — one path for all)
//! - `indexer.rs`  - Background job that embeds new records
//! - `next_index.rs` - Moves the index to another model in the background, so
//!   search keeps working until the new vectors are complete
//! - `query.rs`    - Hybrid retrieval: dense ANN ⊕ BM25, z-fused with a
//!   query-adaptive weight, RRF across phrasings. No reranker (see query.rs)

pub mod bm25;
pub mod embedder;
pub mod indexer;
pub mod next_index;
pub mod query;
pub mod wiki_first;

pub use embedder::{get_embedder, Embedder, LocalEmbedder};
pub use indexer::run_embedding_job;
pub use query::{ScopeMode, SemanticSearchEngine};

//! Dayline — Shape of Your Day
//!
//! Per-event z-scored signals:
//! - **Novelty (global)** (Novel ↑ / Routine ↓): kernel-weighted centroid distance — "rare in your life at all"
//! - **Novelty (local)** (LOF): density-relative unusualness — "off-pattern for its kind"
//! - **Autonomic** (Stress ↑ / Recovery ↓): embedding-weighted HR comparison, physiological response

pub mod annotate;
pub mod autonomic_scoring;
pub mod context;
pub mod embedding_ops;
pub mod novelty;
pub mod sleep;
pub mod topic_entity_novelty;

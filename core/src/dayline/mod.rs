//! Dayline — Shape of Your Day
//!
//! Two per-event z-scored signals:
//! - **Novelty** (Novel ↑ / Routine ↓): embedding centroid distance, semantic unusualness
//! - **Autonomic** (Stress ↑ / Recovery ↓): embedding-weighted HR comparison, physiological response

pub mod autonomic_scoring;
pub mod context;
pub mod novelty;
pub mod topic_entity_novelty;

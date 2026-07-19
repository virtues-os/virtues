//! Sessionization: rolling sensor-cadence fragments up into the units of a life.
//!
//! Raw capture arrives at the *recorder's* cadence, not yours — 5-minute audio
//! slices, per-message chat rows, per-batch GPS points. None of those is a unit of
//! experience; they are the integrand. Before anything reasons over them they must
//! roll up into **sessions**: a conversation, a stay, a stretch of one context.
//!
//! This module is the family of those rollups. [`changepoint`] is the shared
//! primitive — offline optimal-partitioning over a normalised feature series —
//! and each submodule applies it to one modality: [`audio`] over loudness and
//! speaker count. (Location visits live in `entity_resolution::places` for
//! historical reasons; iMessage will land here next.)
//!
//! Every sessionizer is **mechanical**: it finds boundaries and stitches content.
//! It never labels or summarises — that is the detective's job in the day
//! pipeline, where the full context lives. See `docs/event-timeline.md`.

pub mod audio;
pub mod changepoint;

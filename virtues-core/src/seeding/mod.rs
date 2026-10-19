//! The defaults every box needs at boot. No demo data is seeded from the
//! binary.
//!
//! Models, agents and built-in tools are read from the virtues-registry crate
//! at runtime.

pub mod prod_seed;

pub use prod_seed::seed_production_data;

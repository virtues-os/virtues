//! Seeding module for production defaults
//!
//! Note: Models, agents, and built-in tools are read directly from
//! the virtues-registry crate at runtime.
//! See: packages/virtues-registry/

pub mod prod_seed;

pub use prod_seed::seed_production_data;

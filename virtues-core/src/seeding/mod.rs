//! The defaults every box needs at boot. Demo data is not seeded from the
//! binary: a demo box loads the demo life (deploy/demo/).
//!
//! Models, agents and built-in tools are read from the virtues-registry crate
//! at runtime.

pub mod prod_seed;

pub use prod_seed::seed_production_data;

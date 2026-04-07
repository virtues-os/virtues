//! Pipeline — sync + transform execution
//!
//! Handles the actual work of syncing data and transforming it.
//! Run tracking is in scheduler/tasks.rs. This module is pure execution.

pub mod context;
pub mod entity_resolution;
pub mod executor;
pub mod sync;
pub mod transform;
pub mod transform_trigger;

pub use context::{ApiKeys, TransformContext};
pub use entity_resolution::{chain_to_people_resolution, chain_to_place_resolution};
pub use executor::PipelineExecutor;
pub use transform_trigger::create_transform_job_for_stream;

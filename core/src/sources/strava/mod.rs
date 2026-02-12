//! Strava integration
//!
//! Syncs workout activities from Strava's API into the health_workout ontology.

pub mod activities;
pub mod client;
pub mod registry;
pub mod types;

pub use activities::StravaActivitiesStream;

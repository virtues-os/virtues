//! GitHub integration
//!
//! Syncs activity events from GitHub via the Events API.
//! Currently supports WatchEvent (stars) and ForkEvent, transforming
//! them into the content_bookmark ontology.

pub mod client;
pub mod events;
pub mod registry;
pub mod types;

pub use events::GitHubEventsStream;

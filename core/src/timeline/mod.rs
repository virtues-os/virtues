//! Timeline Module
//!
//! Location-first day view generation. Converts location_visits into a structured
//! day view with chunks (Location, Transit, MissingData) and attached ontology data.
//!
//! ## Architecture
//!
//! - `chunks/`: Location chunking, transit classification, data attachment
//! - Day view is computed on-demand from existing tables (no persistence)
//!
//! ## Data Flow
//!
//! 1. Query location_visits for date
//! 2. Build chunks from visits + gaps
//! 3. Classify transit by speed
//! 4. Attach messages, transcripts, calendar, etc. by temporal intersection

pub mod chunks;

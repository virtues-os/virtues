//! iOS device data sources
//!
//! Processors for data pushed from iOS devices including HealthKit,
//! Location, and Microphone streams.

pub mod barometer;
pub mod battery;
pub mod contacts;
pub mod financekit;
pub mod healthkit;
pub mod location;
pub mod microphone;
pub mod registry;

// PushStream implementations
pub use barometer::IosBarometerStream;
pub use battery::IosBatteryStream;
pub use contacts::IosContactsStream;
pub use financekit::IosFinanceKitStream;
pub use healthkit::IosHealthKitStream;
pub use location::IosLocationStream;
pub use microphone::IosMicrophoneStream;

// Registry
pub use registry::IosSource;

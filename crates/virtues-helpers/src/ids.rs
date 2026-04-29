//! Deterministic ID generation.
//!
//! Matches the format used by `core/src/ids`: `{prefix}_{hex16}` where the hex is
//! the first 8 bytes of a SHA-256 hash over the pipe-separated components.
//!
//! The same input always produces the same ID, enabling idempotent upserts.

use sha2::{Digest, Sha256};

pub const WIKI_PERSON_PREFIX: &str = "person";
pub const WIKI_PLACE_PREFIX: &str = "place";
pub const WIKI_ORG_PREFIX: &str = "org";
pub const WIKI_DAY_PREFIX: &str = "day";
pub const WIKI_EVENT_PREFIX: &str = "event";

/// Generate a deterministic ID from a prefix and components.
///
/// # Example
/// ```
/// use virtues_helpers::ids::{generate_id, WIKI_PERSON_PREFIX};
/// let id = generate_id(WIKI_PERSON_PREFIX, &["john@example.com"]);
/// assert!(id.starts_with("person_"));
/// ```
pub fn generate_id(prefix: &str, components: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for component in components {
        hasher.update(component.as_bytes());
        hasher.update(b"|");
    }
    let hash = hasher.finalize();
    let hash_str = hex::encode(&hash[..8]);
    format!("{}_{}", prefix, hash_str)
}

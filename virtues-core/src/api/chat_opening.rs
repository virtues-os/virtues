//! What is left of the opening sentence an empty new chat used to show.
//!
//! The box no longer writes one, so nothing is ever held and [`text_for`]
//! finds nothing. `api::chat::store_opening` still asks, for a client that
//! sends an opening id.
//!
//! TODO(2026-10-22): delete this module with `store_opening` and the
//! `opening` field in `api/chat.rs`, and the opening plumbing in the web
//! client's ChatView and chatInstances, once those files are free to change.

/// The opening's text by id. There are none.
pub fn text_for(_id: &str) -> Option<String> {
    None
}

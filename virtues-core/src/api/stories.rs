//! The names the `/api/wiki/stories` handlers call for things.
//!
//! A story became a thing in migration 0077, and the code is
//! [`super::things`]. The app calls `/api/wiki/things`
//! (`server/api/things.rs`); the older story routes in `server/api/wiki.rs`
//! still resolve through these names.
//!
//! TODO(2026-11-16): delete this module with the story routes and handlers
//! in `server/api/wiki.rs`.

pub use super::things::{
    create_thing as create_story, delete_thing as delete_story, get_thing as get_story,
    list_things as list_stories, start_article, update_thing as update_story,
    ThingFields as StoryFields,
};

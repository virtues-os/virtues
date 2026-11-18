//! A sample reply in the owner's style, for Settings ("How {name} talks to
//! you"). The preview answers one question about an invented week, so it
//! reads none of the record and shows only manner.
//!
//! It carries the same character lines and style-notes block every chat
//! carries (`agent::prompt`), on the chat's own model (the Standard slot), so
//! what the owner reads here is how the assistant will sound. The notes come
//! from the request, not the stored profile, so the preview follows the field
//! as they type, before anything is saved.

use serde::Serialize;
use sqlx::PgPool;

use crate::agent::prompt::{character_guidelines, style_notes_block};
use crate::error::{Error, Result};

/// The question the preview answers.
const QUESTION: &str = "How did this week go?";

/// An invented week, the only record the preview may draw on. The names are
/// the repo's stock cast.
const WEEK: &str = "- Monday: a long walk by the river in the morning; in the evening, a call with Nick about the garden plans.\n\
- Wednesday: slept a little under six hours; skipped the gym.\n\
- Friday: dinner with David Okafor; finished the first draft of the kitchen plans.";

/// Mirrors `STYLE_NOTES_MAX_CHARS` in assistant_profile.rs.
const MAX_NOTES: usize = 1000;

#[derive(Debug, Serialize)]
pub struct StylePreview {
    pub question: &'static str,
    pub reply: String,
}

fn system_prompt(assistant_name: &str, user_name: &str, notes: &str) -> String {
    format!(
        "You are {assistant_name}, {user_name}'s assistant. How you speak:\n{}{}\n\n\
         This is a preview of your manner on {user_name}'s settings page. Answer the question using only the made-up week \
         below, as if it were their record. Write two to four sentences of prose: no lists, no headings, no greeting. \
         Do not mention that the week is made up.",
        character_guidelines(user_name),
        style_notes_block(Some(notes), user_name),
    )
}

pub async fn preview(pool: &PgPool, notes: &str) -> Result<StylePreview> {
    if notes.chars().count() > MAX_NOTES {
        return Err(Error::InvalidInput(format!("Style notes can be at most {MAX_NOTES} characters.")));
    }
    let assistant_name = crate::api::assistant_profile::get_assistant_name(pool).await?;
    let user_name = crate::api::profile::get_display_name(pool).await?;
    let user = format!("{QUESTION}\n\nThe week:\n{WEEK}");
    let reply = crate::virtues_api::completion::system_completion(
        pool,
        virtues_registry::models::ModelSlot::Standard,
        "style_preview",
        &system_prompt(&assistant_name, &user_name, notes),
        &user,
        crate::virtues_api::request::Thinking::Low,
        0.7,
    )
    .await?;
    Ok(StylePreview { question: QUESTION, reply: reply.trim().to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prompt_carries_the_character_and_the_notes() {
        let prompt = system_prompt("Iris", "Nick", "Short answers. No lists.");
        assert!(prompt.contains("You are Iris, Nick's assistant"));
        assert!(prompt.contains(&character_guidelines("Nick")), "the same lines every chat carries");
        assert!(prompt.contains("<style_notes>\nShort answers. No lists.\n</style_notes>"));
    }

    #[test]
    fn no_notes_means_the_character_alone() {
        let prompt = system_prompt("Iris", "Nick", "  ");
        assert!(!prompt.contains("<style_notes>"));
    }
}

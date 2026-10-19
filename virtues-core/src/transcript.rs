//! Transcript text as the narrators read it.
//!
//! Transcription writes `[Speaker]:` at every change of voice. Rows written by
//! earlier versions carry `[Speaker 1]:` / `[Speaker 2]:` instead, and those
//! numbers are local to one 5-minute chunk: `Speaker 1` is whoever the model
//! heard first, so it can be a different person in the next chunk. A narrator
//! reading numbered tags across chunks takes them as the same people and builds
//! a story on that. Readers that hand transcripts to a model flatten the old
//! tags here, at read time, rather than rewriting stored rows.

use std::borrow::Cow;
use std::sync::LazyLock;

use regex::Regex;

static NUMBERED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[Speaker \d+\]").expect("static regex"));

/// `[Speaker 2]:` → `[Speaker]:`. Borrowed when there is nothing to change.
pub fn flatten_speaker_tags(text: &str) -> Cow<'_, str> {
    NUMBERED.replace_all(text, "[Speaker]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbered_tags_lose_their_numbers() {
        assert_eq!(
            flatten_speaker_tags("[Speaker 1]: left at six. [Speaker 12]: me too."),
            "[Speaker]: left at six. [Speaker]: me too."
        );
    }

    #[test]
    fn current_tags_and_plain_text_are_untouched() {
        for s in ["[Speaker]: fine.", "no tags here", "[inaudible] and [Speaker]"] {
            assert!(matches!(flatten_speaker_tags(s), Cow::Borrowed(_)), "{s}");
        }
    }
}

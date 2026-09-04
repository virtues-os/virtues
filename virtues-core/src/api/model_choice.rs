//! One door for interactive (user-initiated) turns: which model answers.
//!
//! The background writers already have theirs — [`crate::virtues_api::completion`]
//! — and this is the same cure applied to the last hand-roller. The disease
//! there was that "which model" got decided separately at every call site and
//! the sites had to agree. Here it was worse: the decision lived in the
//! BROWSER. `POST /api/chat` took a model id as a required field and the box
//! merely validated it, so a client that had not loaded the catalog sent an
//! empty string and got back a 400 listing 244 ids without naming the one it
//! rejected. That is what a phone did on 2026-09-03, all day, while the same
//! box answered the desktop fine.
//!
//! A model id is an ADDRESS on a specific gateway, not a name (see
//! `model_catalog::slot_for_model`). Addresses are ours to resolve. So the
//! wire carries a CHOICE and never an address, except as a deliberate pin:
//!
//! - **absent** (or empty — see below) — the slot this turn belongs to,
//!   resolved through the owner's pin, then the cloud slot map, then the
//!   compiled floor. This is the ordinary path; a chat is NOT frozen to the
//!   model it opened with, so a slot swap reaches conversations already in
//!   progress.
//! - **present** — the picker. The person chose this model for this turn and
//!   it wins over everything below it.
//!
//! Empty string counts as absent, deliberately. Shipped clients send `""`
//! when their catalog fetch failed, and a phone's bundle cannot be corrected
//! without an App Store round trip — so a box upgrade has to be enough to fix
//! them. Treating `""` as "you did not choose" is what makes that true.

use sqlx::PgPool;
use virtues_registry::models::ModelSlot;

use crate::error::{Error, Result};

/// Which slot a turn belongs to, from the mode the client is in.
///
/// This does not branch yet, and the honest thing is to say so rather than
/// write a match whose arms all agree. Every mode the box knows — `chat`,
/// `deep_research`, `council`, `interview` — answers the same kind of hard
/// turn and differs only in tools and prompt (`tools::get_tools_for_agent_mode`),
/// so they are all the Chat slot; an unknown mode is a client ahead of this
/// box, and Chat is the safe read for it too.
///
/// It is a function anyway because it is the seam the mode/slot question
/// belongs at. The Coding slot is pinnable in Settings while NOTHING in the
/// box asks for it (`get_coding_model` has no callers), so the day applet
/// authoring becomes a mode, this is the one line that wires it up. What the
/// modes DO differ on today is whether they may honor a pin: see
/// [`honors_pin`].
pub fn slot_for_agent_mode(_agent_mode: &str) -> ModelSlot {
    ModelSlot::Chat
}

/// Whether a mode may honor the person's pin, or must ride the slot default.
///
/// The interview may not. Its prompt promises "a no-retention agreement" in as
/// many words, and the Virtues-curated slot map is what keeps that true: a
/// pinned grok (`zdr: none`) or a BYO endpoint would silently void it. Same
/// doctrine as the drafter in `narrative_draft.rs` — a pin governs the chats a
/// person watches, not a room built on a retention promise.
fn honors_pin(agent_mode: &str) -> bool {
    agent_mode != "interview"
}

/// The id the person actually chose for this turn, if any.
///
/// Pure, and the only place the two ways of choosing nothing are collapsed:
/// a field that is absent, and a field that is present but empty. Empty is
/// the one that matters — it is what a shipped client sends when its catalog
/// fetch failed, and reading it as a choice is what turned a flaky fetch on a
/// phone into a hard 400 on every message it sent.
fn wanted_pin<'a>(requested: Option<&'a str>, agent_mode: &str) -> Option<&'a str> {
    requested
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter(|_| honors_pin(agent_mode))
}

/// Does this box's catalog positively contradict the pin?
///
/// `catalog` is `None` when the box has never reached the cloud, and then the
/// answer is always no: we know three ids offline, and refusing a perfectly
/// good pin on the strength of that snapshot would be the same confident lie
/// `model_catalog` declines to tell about context windows. The gateway is the
/// authority; let it answer.
fn pin_is_unknown(id: &str, catalog: Option<&[String]>) -> bool {
    matches!(catalog, Some(known) if !known.iter().any(|k| k == id))
}

/// The model that answers this turn.
///
/// `requested` is what the client sent, if anything. An unknown id is
/// [`Error::InvalidInput`], so the caller can answer 400 and name it.
pub async fn resolve_turn_model(
    pool: &PgPool,
    requested: Option<&str>,
    agent_mode: &str,
) -> Result<String> {
    // The catalog is only consulted to contradict a pin, so the ordinary
    // unpinned turn never pays for it. It used to be built eagerly here:
    // 244 models cloned out of the cache and mapped to 244 Strings, on every
    // message, to answer a question that was not being asked.
    if let Some(id) = wanted_pin(requested, agent_mode) {
        let catalog: Option<Vec<String>> = (!crate::api::model_catalog::is_cold()).then(|| {
            crate::api::model_catalog::models()
                .into_iter()
                .map(|m| m.model_id)
                .collect()
        });
        if pin_is_unknown(id, catalog.as_deref()) {
            return Err(Error::InvalidInput(format!(
                "unknown model \"{id}\" — not in this box's catalog"
            )));
        }
        return Ok(id.to_string());
    }

    // Unpinned: the slot this mode belongs to, through the owner's standing
    // preference for it. A mode that refuses pins refuses the standing one too
    // — the interview's promise is about the curated slot map, not about who
    // typed the id.
    let slot = slot_for_agent_mode(agent_mode);
    if !honors_pin(agent_mode) {
        return Ok(crate::api::model_catalog::model_for_slot(slot));
    }
    match slot {
        ModelSlot::Chat => crate::api::assistant_profile::get_chat_model(pool).await,
        ModelSlot::Coding => crate::api::assistant_profile::get_coding_model(pool).await,
        other => Ok(crate::api::model_catalog::model_for_slot(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_interview_never_honors_a_pin() {
        assert!(!honors_pin("interview"));
        assert!(honors_pin("chat"));
        assert!(honors_pin("deep_research"));
        assert!(honors_pin("council"));
    }

    /// THE regression. A client whose catalog fetch failed sends `""`, and for
    /// one day that was a hard 400 on every message a phone sent, while the
    /// same box answered the desktop fine. Empty is not a choice.
    #[test]
    fn an_empty_model_is_not_a_choice() {
        for sent in [None, Some(""), Some("   "), Some("\t\n")] {
            assert_eq!(
                wanted_pin(sent, "chat"),
                None,
                "sent {sent:?} should fall through to the slot"
            );
        }
    }

    #[test]
    fn a_real_pick_is_honored_and_trimmed() {
        assert_eq!(
            wanted_pin(Some("  anthropic/claude-sonnet-5  "), "chat"),
            Some("anthropic/claude-sonnet-5")
        );
    }

    #[test]
    fn the_interview_falls_through_even_with_a_valid_pick() {
        assert_eq!(
            wanted_pin(Some("anthropic/claude-sonnet-5"), "interview"),
            None,
            "the retention promise is about the curated slot, not the id"
        );
    }

    #[test]
    fn only_a_live_catalog_may_contradict_a_pin() {
        let warm = vec!["anthropic/claude-sonnet-5".to_string()];
        assert!(!pin_is_unknown("anthropic/claude-sonnet-5", Some(&warm)));
        assert!(pin_is_unknown("openai/gpt-9", Some(&warm)));
        // Cold: a box that has never reached the cloud knows three ids, and
        // does not get to refuse models the gateway serves fine.
        assert!(!pin_is_unknown("openai/gpt-9", None));
    }

    /// The DB-backed half, which the pure tests above cannot reach: does the
    /// door actually consult the owner's standing pin, and does the interview
    /// actually refuse it? The catalog is cold in a test, so `model_for_slot`
    /// is the compiled floor and a pin passes through unjudged — which is the
    /// cold-box behaviour these assert alongside.
    ///
    /// The profile row itself is created by migration 0001, so every box that
    /// has migrated has one. Worth knowing, because this door made the ordinary
    /// chat path depend on that row for the first time.
    #[sqlx::test]
    async fn an_unpinned_turn_rides_the_chat_slot(pool: PgPool) {
        let want = crate::api::model_catalog::model_for_slot(ModelSlot::Chat);
        for sent in [None, Some(""), Some("  ")] {
            assert_eq!(
                resolve_turn_model(&pool, sent, "chat").await.unwrap(),
                want,
                "sent {sent:?}"
            );
        }
    }

    #[sqlx::test]
    async fn a_standing_pin_is_what_answers(pool: PgPool) {
        sqlx::query("UPDATE app_assistant_profile SET chat_model_id = $1")
            .bind("example/pinned-model")
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(
            resolve_turn_model(&pool, None, "chat").await.unwrap(),
            "example/pinned-model"
        );
        // And the interview still will not touch it.
        assert_eq!(
            resolve_turn_model(&pool, None, "interview").await.unwrap(),
            crate::api::model_catalog::model_for_slot(ModelSlot::Chat)
        );
    }

    /// The wire contract, end to end, for the three bodies that actually
    /// arrive: a current client that omits the field, a shipped client whose
    /// catalog fetch failed, and a real pick. The middle one is why a box
    /// upgrade alone can fix a phone whose bundle cannot be corrected without
    /// an App Store round trip.
    #[test]
    fn all_three_wire_shapes_parse_and_resolve() {
        use crate::api::chat::ChatRequest;
        let cases = [
            (r#"{"chatId":"c1","messages":[]}"#, None),
            (r#"{"chatId":"c1","messages":[],"model":""}"#, None),
            (
                r#"{"chatId":"c1","messages":[],"model":"anthropic/claude-sonnet-5"}"#,
                Some("anthropic/claude-sonnet-5"),
            ),
        ];
        for (body, want) in cases {
            let req: ChatRequest =
                serde_json::from_str(body).unwrap_or_else(|e| panic!("{body} → {e}"));
            assert_eq!(
                wanted_pin(req.model.as_deref(), &req.agent_mode),
                want,
                "body {body}"
            );
        }
    }
}

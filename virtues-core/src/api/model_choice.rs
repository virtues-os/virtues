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

use crate::api::chat_mode::ChatMode;
use crate::error::{Error, Result};

/// The id the person actually chose for this turn, if any.
///
/// Pure, and the only place the two ways of choosing nothing are collapsed:
/// a field that is absent, and a field that is present but empty. Empty is
/// the one that matters — it is what a shipped client sends when its catalog
/// fetch failed, and reading it as a choice is what turned a flaky fetch on a
/// phone into a hard 400 on every message it sent.
fn wanted_pin<'a>(requested: Option<&'a str>, mode: &ChatMode) -> Option<&'a str> {
    requested
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter(|_| mode.honors_pin())
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
    mode: &ChatMode,
) -> Result<String> {
    // The catalog is only consulted to contradict a pin, so the ordinary
    // unpinned turn never pays for it. It used to be built eagerly here:
    // 244 models cloned out of the cache and mapped to 244 Strings, on every
    // message, to answer a question that was not being asked.
    if let Some(id) = wanted_pin(requested, mode) {
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
    let slot = mode.slot();
    if !mode.honors_pin() {
        return Ok(crate::api::model_catalog::model_for_slot(slot));
    }
    let standing = match slot {
        ModelSlot::Standard => crate::api::assistant_profile::get_standard_model(pool).await?,
        ModelSlot::Deep => crate::api::assistant_profile::get_deep_model(pool).await?,
        other => crate::api::model_catalog::model_for_slot(other),
    };

    // A STORED pin can be empty too, and that one is worse than the wire's:
    // `get_standard_model` falls back on SQL NULL only, and the profile PATCH
    // binds whatever string it is given, so `{"standard_model_id": ""}` persists
    // an empty pin that outlives the request. Guarding only the wire would
    // leave the same empty model id reaching the gateway, from a value the
    // person cannot see or clear from the picker. This door promises a
    // non-empty model; it has to mean it wherever the id came from.
    Ok(non_empty(standing).unwrap_or_else(|| crate::api::model_catalog::model_for_slot(slot)))
}

/// `Some` only for a string with something in it. One definition of "empty",
/// used for both the wire field and the stored pin.
fn non_empty(s: String) -> Option<String> {
    (!s.trim().is_empty()).then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// THE regression. A client whose catalog fetch failed sends `""`, and for
    /// one day that was a hard 400 on every message a phone sent, while the
    /// same box answered the desktop fine. Empty is not a choice.
    #[test]
    fn an_empty_model_is_not_a_choice() {
        for sent in [None, Some(""), Some("   "), Some("\t\n")] {
            assert_eq!(
                wanted_pin(sent, &ChatMode::Chat),
                None,
                "sent {sent:?} should fall through to the slot"
            );
        }
    }

    #[test]
    fn a_real_pick_is_honored_and_trimmed() {
        assert_eq!(
            wanted_pin(Some("  anthropic/claude-sonnet-5  "), &ChatMode::Chat),
            Some("anthropic/claude-sonnet-5")
        );
    }

    #[test]
    fn the_interview_falls_through_even_with_a_valid_pick() {
        assert_eq!(
            wanted_pin(Some("anthropic/claude-sonnet-5"), &ChatMode::Interview),
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
    async fn an_unpinned_turn_rides_the_standard_slot(pool: PgPool) {
        let want = crate::api::model_catalog::model_for_slot(ModelSlot::Standard);
        for sent in [None, Some(""), Some("  ")] {
            assert_eq!(
                resolve_turn_model(&pool, sent, &ChatMode::Chat).await.unwrap(),
                want,
                "sent {sent:?}"
            );
        }
    }

    #[sqlx::test]
    async fn a_standing_pin_is_what_answers(pool: PgPool) {
        sqlx::query("UPDATE app_assistant_profile SET standard_model_id = $1")
            .bind("example/pinned-model")
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(
            resolve_turn_model(&pool, None, &ChatMode::Chat).await.unwrap(),
            "example/pinned-model"
        );
        // And the interview still will not touch it.
        assert_eq!(
            resolve_turn_model(&pool, None, &ChatMode::Interview).await.unwrap(),
            crate::api::model_catalog::model_for_slot(ModelSlot::Standard)
        );
    }

    #[sqlx::test]
    async fn an_empty_stored_pin_cannot_reach_the_gateway(pool: PgPool) {
        // The profile PATCH binds the string it is handed, so this row state
        // is reachable from the API even though the web UI maps "" to NULL.
        sqlx::query("UPDATE app_assistant_profile SET standard_model_id = ''")
            .execute(&pool)
            .await
            .unwrap();

        let got = resolve_turn_model(&pool, None, &ChatMode::Chat).await.unwrap();
        assert!(!got.trim().is_empty(), "resolved an empty model id");
        assert_eq!(got, crate::api::model_catalog::model_for_slot(ModelSlot::Standard));
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
                wanted_pin(req.model.as_deref(), &ChatMode::from_wire(&req.agent_mode)),
                want,
                "body {body}"
            );
        }
    }
}

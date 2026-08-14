//! Drafting "In your own words" from the interview answers.
//!
//! Distinct from `narrative_identity_gen`, which drafts a short paragraph from
//! OBSERVED data — recurring people, places, recent days. This one reads only
//! what the person wrote themselves, and the difference is the whole point:
//! values, wounds and direction cannot be derived from behaviour. A machine
//! guessing at someone's telos from their message volume would be both wrong
//! and, on the subjects this covers, insulting.
//!
//! TWO ARTIFACTS FROM ONE CALL:
//!
//!   document — past / present / future, in their voice, for them to read and
//!              correct. Long, and never injected wholesale.
//!   core     — 80-120 words, which IS injected, on every message they send.
//!              It exists because a few thousand words cannot ride along on
//!              every prompt, and something has to be the short version.
//!
//! THE DRAFT IS A MIRROR, NOT A VERDICT. It arranges what someone wrote and
//! hands it back for correction. Everything below is aimed at keeping it from
//! doing anything more than that — no diagnosis, no invention, no flattery, no
//! psychologising a person out of their own words.

use serde::Serialize;
use sqlx::PgPool;

use crate::error::{Error, Result};

const SYSTEM_PROMPT: &str = r#"You are arranging a person's own answers into a document they will read and correct. It is called "In your own words", and that is the standard: their words, ordered — not your reading of them.

You are given answers to an interview about their life: chapters, a high point, a low point, the people in it, what they have lost, who they admire, what they are proud of, what makes them unusual, which pull is strongest, what they believe, what is live now, what they want, and what they fear becoming.

WRITE TWO THINGS, separated by a line containing only ---CORE---.

FIRST, the document. Three sections with these exact headings:

## Where you have been
## Who you are
## Where you are going

Rules for the document:
- Use THEIR words. Keep their phrases, their names, their turns of speech. You are arranging, not translating. If a sentence of theirs is good, use it.
- Second person throughout ("you"), present tense for who they are, past for what happened.
- Ground every sentence in something they actually wrote. If they did not say it, it does not appear. No inference about motives, no "this suggests", no filling gaps with what people are usually like.
- Never diagnose, never psychologise, never explain someone to themselves. "You lost your father in 2019 and the year after was the worst of your life" is right. "This loss clearly shapes your fear of commitment" is a violation.
- Leave the unanswered alone. If they skipped a question, that section is simply shorter. Do not note the absence, do not prompt them, do not compensate.
- Do not flatter, do not console, do not summarise their life as a lesson. No redemptive arc unless they wrote one.
- Aspirations are marked as aspirations. "You want to be more patient" — never "you are patient".
- Plain prose. No lists, no bold, no headings beyond the three above.

SECOND, after the ---CORE--- line: 80-120 words, plain text, no heading. This is what an assistant carries into every conversation, so it holds only what would change how to speak to them: what they are working toward, what they are up against, what they believe, and anything they are sensitive about. Not biography. Not their history. What a thoughtful friend keeps in mind, not what they could recite.

Output the document, then ---CORE---, then the core. Nothing else."#;

#[derive(Debug, Serialize)]
pub struct Draft {
    pub document: String,
    pub core: String,
}

/// Read the answers, write the document.
///
/// Refuses on an empty interview rather than producing a document about
/// nobody — an invented identity handed to someone as their own would be the
/// worst single output this product could generate.
pub async fn draft_from_interview(pool: &PgPool) -> Result<Draft> {
    let answers = crate::api::narrative_interview::list_answers(pool).await?;
    let written: Vec<_> = answers
        .into_iter()
        .filter(|a| !a.answer.trim().is_empty())
        .collect();

    if written.is_empty() {
        return Err(Error::Other(
            "nothing written yet — answer a question or two first".into(),
        ));
    }

    let mut prompt = String::from("Their answers:\n");
    for a in &written {
        // The question text stays on the client, which owns the set; the id is
        // enough for the model to know what was asked, and keeping the wording
        // out of here means rewording a question never invalidates a draft.
        prompt.push_str(&format!("\n## {}\n{}\n", a.question_id, a.answer.trim()));
    }

    let raw = call_model(pool, &prompt).await?;
    let (document, core) = split_draft(&raw);

    if document.trim().is_empty() {
        return Err(Error::ExternalApi("draft came back empty".into()));
    }

    sqlx::query(
        "INSERT INTO wiki_narrative_identity (id, content, document, drafted_at) \
         VALUES ('nar_identity_001', $2, $1, now()) \
         ON CONFLICT (id) DO UPDATE SET \
           document = EXCLUDED.document, \
           -- The core is only replaced when we have one. A short version that
           -- failed to parse must never blank the paragraph the assistant is
           -- already carrying.
           content = CASE WHEN $2 <> '' THEN EXCLUDED.content ELSE wiki_narrative_identity.content END, \
           drafted_at = now(), \
           updated_at = now()",
    )
    .bind(&document)
    .bind(&core)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("save narrative draft: {e}")))?;

    tracing::info!(
        answers = written.len(),
        document_chars = document.len(),
        core_chars = core.len(),
        "narrative draft written from the interview"
    );

    Ok(Draft { document, core })
}

/// Split on the sentinel. A model that forgets it gives us a document and no
/// core, which is recoverable; treating the whole reply as a core would inject
/// three thousand words into every prompt, which is not.
fn split_draft(raw: &str) -> (String, String) {
    match raw.split_once("---CORE---") {
        Some((doc, core)) => (doc.trim().to_string(), core.trim().to_string()),
        None => (raw.trim().to_string(), String::new()),
    }
}

async fn call_model(pool: &PgPool, user_prompt: &str) -> Result<String> {
    let chat_model = crate::api::assistant_profile::get_chat_model(pool).await?;

    let client = crate::virtues_api::client::BearerClient::from_env(pool.clone())
        .with_purpose(crate::virtues_api::client::Purpose::System)
        .with_feature("narrative_draft");

    let response = client
        .post_json(
            "/v1/ai/chat/completions",
            &serde_json::json!({
                "model": chat_model,
                "messages": [
                    {"role": "system", "content": SYSTEM_PROMPT},
                    {"role": "user", "content": user_prompt}
                ],
                // Room for a real document. The interview can run to thousands
                // of words and a truncated life story is worse than none.
                "max_tokens": 4000,
                // Low: this is arrangement, not composition. Invention is the
                // failure mode being guarded against everywhere else here.
                "temperature": 0.3
            }),
        )
        .await
        .map_err(|e| Error::Network(format!("virtues-api request failed: {e}")))?;

    if !response.is_success() {
        return Err(Error::ExternalApi(match response.status {
            402 => "Usage limit reached".to_string(),
            429 => "Rate limited — try again in a moment.".to_string(),
            s => format!("virtues-api error {s}: {}", response.body),
        }));
    }

    // `body` is already parsed JSON on this client — the same shape
    // narrative_identity_gen reads.
    Ok(response.body["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string())
}

// ─── handlers ───────────────────────────────────────────────────────────────

pub async fn draft_handler(
    axum::extract::State(state): axum::extract::State<crate::server::AppState>,
    _user: crate::middleware::auth::AuthUser,
) -> impl axum::response::IntoResponse {
    use axum::Json;
    use axum::response::IntoResponse as _;
    match draft_from_interview(state.db.pool()).await {
        Ok(d) => (axum::http::StatusCode::OK, Json(d)).into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "narrative draft failed");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_the_sentinel() {
        let (doc, core) = split_draft("## Where you have been\nBoston.\n---CORE---\nShort version.");
        assert_eq!(doc, "## Where you have been\nBoston.");
        assert_eq!(core, "Short version.");
    }

    #[test]
    fn a_missing_sentinel_keeps_the_document_and_drops_the_core() {
        // The alternative — treating the whole reply as the core — would inject
        // the entire document into every prompt the person ever sends.
        let (doc, core) = split_draft("## Where you have been\nBoston.");
        assert_eq!(doc, "## Where you have been\nBoston.");
        assert!(core.is_empty());
    }
}

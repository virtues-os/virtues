//! Drafting "In your own words" from the interview answers.
//!
//! THE ONLY WRITER of narrative identity. There used to be a second —
//! `narrative_identity_gen`, which drafted a portrait from OBSERVED data
//! (recurring people, places, recent days) and overwrote this one's work.
//! Deleted 2026-08-26 as a doctrine violation, not just a race: values,
//! wounds and direction cannot be derived from behaviour. A machine guessing
//! at someone's telos from their message volume would be both wrong and, on
//! the subjects this covers, insulting. This module reads only what the
//! person wrote themselves.
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

THIRD, after a line containing only ---RULES---: any instruction they gave about what NOT to raise, one per line, as a short imperative in their own terms ("never suggest bars", "do not mention my father unless I do"). These are drawn ONLY from what they explicitly asked for — usually the last answer. Never invent one, never infer one from a sad story, never turn an observation into a rule. If they asked for nothing, write nothing after this line. Being told about a loss is not the same as being asked never to mention it.

Output the document, then ---CORE---, then the core, then ---RULES---, then the rules. Nothing else."#;

#[derive(Debug, Serialize)]
pub struct Draft {
    pub document: String,
    pub core: String,
    /// PROPOSED, not saved. Nothing here binds the assistant until the person
    /// confirms it — a rule the box invented and then obeyed would be worse
    /// than no rules at all, because it would be invisible and permanent.
    pub proposed_rules: Vec<String>,
}

/// The singleton subject id — the one narrative-identity article a box has.
const NAR_IDENTITY_ID: &str = "nar_identity_001";

/// Read the answers, write the document.
///
/// Refuses on an empty interview rather than producing a document about
/// nobody — an invented identity handed to someone as their own would be the
/// worst single output this product could generate.
///
/// ONE WRITER, ONCE. The document lands as a wiki article (a real page: the
/// editor, history, marginalia), and from that moment the person owns it —
/// this function never runs the model again while the article exists. That is
/// the entire ownership model: the machine may write this document only while
/// it is empty. (The platform enforces it too: a pool-only write to a
/// CRDT-backed page is silently discarded.) A repeat call hands back what
/// stands, so a lost response or a retry cannot double-spend or overwrite.
pub async fn draft_from_interview(pool: &PgPool) -> Result<Draft> {
    if let Some(prose) = crate::api::wiki_articles::get_article_prose(
        pool,
        "narrative_identity",
        NAR_IDENTITY_ID,
    )
    .await?
    {
        let core: String =
            sqlx::query_scalar("SELECT content FROM wiki_narrative_identity LIMIT 1")
                .fetch_optional(pool)
                .await
                .map_err(|e| Error::Database(format!("read narrative core: {e}")))?
                .unwrap_or_default();
        return Ok(Draft {
            document: prose.content,
            core,
            proposed_rules: Vec::new(),
        });
    }

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
    let (document, core, proposed_rules) = split_draft(&raw);

    if document.trim().is_empty() {
        return Err(Error::ExternalApi("draft came back empty".into()));
    }

    // The document becomes the singleton narrative-identity article — a page
    // seeded with this markdown, which the person edits from here on.
    // (`create_article` is idempotent: a concurrent first draft cannot strand
    // a second page.)
    crate::api::wiki_articles::create_article(
        pool,
        "narrative_identity",
        NAR_IDENTITY_ID,
        "In your own words",
        &document,
    )
    .await?;

    // The core stays apparatus, not article: it is what rides into every chat.
    sqlx::query(
        "INSERT INTO wiki_narrative_identity (id, content, drafted_at) \
         VALUES ($2, $1, now()) \
         ON CONFLICT (id) DO UPDATE SET \
           -- The core is only replaced when we have one. A short version that
           -- failed to parse must never blank the paragraph the assistant is
           -- already carrying.
           content = CASE WHEN $1 <> '' THEN EXCLUDED.content ELSE wiki_narrative_identity.content END, \
           drafted_at = now(), \
           updated_at = now()",
    )
    .bind(&core)
    .bind(NAR_IDENTITY_ID)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("save narrative draft: {e}")))?;

    tracing::info!(
        answers = written.len(),
        document_chars = document.len(),
        core_chars = core.len(),
        "narrative draft written from the interview"
    );

    Ok(Draft {
        document,
        core,
        proposed_rules,
    })
}

/// Split on the sentinel. A model that forgets it gives us a document and no
/// core, which is recoverable; treating the whole reply as a core would inject
/// three thousand words into every prompt, which is not.
fn split_draft(raw: &str) -> (String, String, Vec<String>) {
    let (doc, rest) = match raw.split_once("---CORE---") {
        Some((d, r)) => (d, r),
        None => (raw, ""),
    };
    let (core, rules_block) = match rest.split_once("---RULES---") {
        Some((c, r)) => (c, r),
        None => (rest, ""),
    };
    let rules = rules_block
        .lines()
        .map(|l| l.trim().trim_start_matches(['-', '*', '•']).trim())
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    (doc.trim().to_string(), core.trim().to_string(), rules)
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
    // entity_article_gen reads.
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

// ─── rules ──────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Rule {
    pub id: String,
    pub rule: String,
    pub kind: String,
    pub active: bool,
}

/// One confirmed rule.
///
/// `kind` arrives from the client because only the person knows which it is:
/// "never mention my father" and "help me hold my fast" are both rules and they
/// are opposites. It defaults to `avoid` — the reading that cannot cause harm if
/// a client omits it.
#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub enum RuleInput {
    /// The wire shape every client used before `kind` existed.
    Bare(String),
    Kinded {
        rule: String,
        #[serde(default = "default_kind")]
        kind: String,
    },
}

fn default_kind() -> String {
    "avoid".to_string()
}

impl RuleInput {
    fn parts(&self) -> (&str, &str) {
        match self {
            RuleInput::Bare(r) => (r.as_str(), "avoid"),
            RuleInput::Kinded { rule, kind } => (rule.as_str(), kind.as_str()),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct SaveRules {
    /// Exactly what the person confirmed, in the wording they left it in. The
    /// proposals are thrown away; only this is stored.
    pub rules: Vec<RuleInput>,
}

pub async fn list_rules(pool: &PgPool) -> Result<Vec<Rule>> {
    sqlx::query_as::<_, Rule>(
        "SELECT id, rule, kind, active FROM wiki_rules WHERE active ORDER BY created_at",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("list rules: {e}")))
}

pub async fn rules_handler(
    axum::extract::State(state): axum::extract::State<crate::server::AppState>,
    _user: crate::middleware::auth::AuthUser,
) -> impl axum::response::IntoResponse {
    use axum::{response::IntoResponse as _, Json};
    match list_rules(state.db.pool()).await {
        Ok(rules) => (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({ "rules": rules })),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// Replace the rule set with exactly what was confirmed.
///
/// A replace rather than an append: this is the screen where someone reviews
/// every rule their box obeys, so leaving it must mean the list now says what
/// they saw. An append would let a rule they unticked survive invisibly, which
/// is the one failure this table exists to prevent.
pub async fn save_rules_handler(
    axum::extract::State(state): axum::extract::State<crate::server::AppState>,
    _user: crate::middleware::auth::AuthUser,
    axum::Json(req): axum::Json<SaveRules>,
) -> impl axum::response::IntoResponse {
    use axum::{response::IntoResponse as _, Json};
    let pool = state.db.pool();

    let mut tx = match pool.begin().await {
        Ok(t) => t,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    };

    if let Err(e) = sqlx::query("DELETE FROM wiki_rules").execute(&mut *tx).await {
        tracing::error!(error = %e, "rules: clear failed");
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    for (i, input) in req.rules.iter().enumerate() {
        let (rule, kind) = input.parts();
        let rule = rule.trim();
        if rule.is_empty() {
            continue;
        }
        // The column has a CHECK; sending anything else would fail the whole
        // transaction and lose the rules that were fine.
        let kind = if kind == "defend" { "defend" } else { "avoid" };
        if let Err(e) = sqlx::query("INSERT INTO wiki_rules (id, rule, kind) VALUES ($1, $2, $3)")
            .bind(format!("rule_{i:03}"))
            .bind(rule)
            .bind(kind)
            .execute(&mut *tx)
            .await
        {
            tracing::error!(error = %e, "rules: insert failed");
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    }

    if let Err(e) = tx.commit().await {
        tracing::error!(error = %e, "rules: commit failed");
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    tracing::info!(count = req.rules.len(), "rules saved");
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({ "saved": req.rules.len() })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_document_core_and_rules() {
        let (doc, core, rules) = split_draft(
            "## Where you have been\nBoston.\n---CORE---\nShort version.\n---RULES---\n- never suggest bars\n- do not mention my father",
        );
        assert_eq!(doc, "## Where you have been\nBoston.");
        assert_eq!(core, "Short version.");
        assert_eq!(rules, vec!["never suggest bars", "do not mention my father"]);
    }

    #[test]
    fn a_missing_sentinel_keeps_the_document_and_drops_the_core() {
        // The alternative — treating the whole reply as the core — would inject
        // the entire document into every prompt the person ever sends.
        let (doc, core, rules) = split_draft("## Where you have been\nBoston.");
        assert_eq!(doc, "## Where you have been\nBoston.");
        assert!(core.is_empty());
        assert!(rules.is_empty());
    }

    #[test]
    fn no_rules_asked_for_means_no_rules_proposed() {
        // Silence here must stay silence. Someone who wrote about a loss and
        // asked for nothing has not asked for a rule, and manufacturing one
        // would put words in their mouth that then govern the assistant.
        let (_, core, rules) = split_draft("Doc.\n---CORE---\nCore.\n---RULES---\n");
        assert_eq!(core, "Core.");
        assert!(rules.is_empty());
    }
}

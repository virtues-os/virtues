//! Narrative-identity draft generation — the onboarding "reveal".
//!
//! Gathers the box's earliest-formed sense of the user — the people and places
//! that recur, recent day biographies — and asks the local AI to draft a short,
//! grounded, second-person "narrative identity": who you seem to be, from the
//! data so far. It is the first issue of a recurring examined-self document; its
//! distilled core is exactly what the agent reads (`wiki_narrative_identity`).
//!
//! Mirrors `day_summary.rs`: a data-richness gate so we never spend on an empty
//! box, then `BearerClient` + `Purpose::System` → `/v1/ai/chat/completions`,
//! honest about how little is known. Writes via `update_narrative_identity`, so
//! the agent picks it up automatically on the next chat.

use sqlx::PgPool;

use crate::error::{Error, Result};

use super::wiki::{update_narrative_identity, UpdateNarrativeIdentityRequest};

const SYSTEM_PROMPT: &str = r#"You are drafting a person's "narrative identity" — a short, honest portrait of who they are, for them to read and correct. You are given only what their private box has gathered so far: the people and places that recur in their data, and recent daily summaries. This is an early draft from thin evidence — a first impression, not a verdict.

WRITE:
- One paragraph, ~60-110 words. Plain text only — no markdown, no headings, no lists, no quotation marks around the whole thing.
- Second person ("you"). Present tense — who they are now, the chapter they're in.
- Ground every claim in the data you were given: name the people, places, and patterns that actually appear. If you can't ground it, don't write it.
- Describe patterns, never fixed essence. Write "lately you've been…", "your weeks tend to…", "the people who recur are…" — never "you ARE an introvert" or "you're the kind of person who…". Verbs and patterns, not nouns and types.
- Be honest about how little is known. Name what the data does NOT yet show ("the box hasn't seen your work, your reading, or your mornings yet"). Absence of data is not data — never invent feelings, motives, or events to fill space.
- Warm but precise, like a perceptive friend who has read the record and won't flatter you. Never clinical, never saccharine, never a horoscope (nothing that could be true of anyone).
- End open, not closed — this is a draft that will deepen as more arrives, not a final word.

Output only the paragraph."#;

/// Max characters of gathered material in the user prompt.
const MAX_TOTAL_CHARS: usize = 12000;

/// Below this many strong signals (strong people + strong places + narrated
/// days) we still draft, but tell the model to keep it to an honest sketch.
const RICH_THRESHOLD: i64 = 5;

/// Outcome of a draft attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftOutcome {
    /// A full draft was written from rich-enough data.
    Generated,
    /// A short, honest sketch was written from thin data.
    Thin,
    /// Too little data to draw anyone — nothing written, no LLM call.
    Deferred,
}

/// A cheap read of how much the box knows yet.
struct Richness {
    strong_people: i64,
    strong_places: i64,
    narrated_days: i64,
}

impl Richness {
    /// Essentially nothing indexed — don't even call the model.
    fn is_empty(&self) -> bool {
        self.strong_people == 0 && self.strong_places == 0 && self.narrated_days == 0
    }
    fn signal_total(&self) -> i64 {
        self.strong_people + self.strong_places + self.narrated_days
    }
}

/// Generate (and persist) a draft narrative identity from early indexed data.
///
/// Returns `Deferred` without spending on the wallet when the box is essentially
/// empty; `Thin`/`Generated` once it has written `wiki_narrative_identity`.
pub async fn generate_narrative_identity_draft(pool: &PgPool) -> Result<DraftOutcome> {
    let gate = assess_richness(pool).await;
    if gate.is_empty() {
        tracing::info!("narrative identity: insufficient data, deferring draft");
        return Ok(DraftOutcome::Deferred);
    }
    let thin = gate.signal_total() < RICH_THRESHOLD;

    let prompt = build_prompt(pool, thin).await;

    tracing::info!(
        prompt_chars = prompt.len(),
        thin,
        "generating narrative identity draft"
    );

    let raw = call_virtues_api(pool, &prompt).await?;
    let paragraph = parse_paragraph(&raw);
    if paragraph.is_empty() {
        return Err(Error::ExternalApi(
            "LLM returned empty narrative identity".to_string(),
        ));
    }

    update_narrative_identity(pool, UpdateNarrativeIdentityRequest { content: paragraph }).await?;

    Ok(if thin {
        DraftOutcome::Thin
    } else {
        DraftOutcome::Generated
    })
}

/// One aggregate query: how many entities have *repeated* evidence (so we don't
/// surface entities still mid-dedup), plus how many days have a written
/// biography (the highest-signal, dedup-tolerant input).
async fn assess_richness(pool: &PgPool) -> Richness {
    let row: (i64, i64, i64) = sqlx::query_as(
        r#"
        SELECT
            (SELECT count(*) FROM wiki_people WHERE interaction_count >= 3),
            (SELECT count(*) FROM wiki_places WHERE visit_count >= 2),
            (SELECT count(*) FROM wiki_days WHERE autobiography IS NOT NULL AND length(trim(autobiography)) > 0)
        "#,
    )
    .fetch_one(pool)
    .await
    .unwrap_or((0, 0, 0));

    Richness {
        strong_people: row.0,
        strong_places: row.1,
        narrated_days: row.2,
    }
}

/// Assemble the material the model draws from.
async fn build_prompt(pool: &PgPool, thin: bool) -> String {
    let mut p = String::from("Here is what the box has gathered about this person so far.\n");

    // People who recur (by interaction volume).
    let people: Vec<(String, Option<String>, i64)> = sqlx::query_as(
        "SELECT canonical_name, relationship_category, interaction_count \
         FROM wiki_people WHERE interaction_count > 0 \
         ORDER BY interaction_count DESC LIMIT 8",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    if !people.is_empty() {
        let lines: Vec<String> = people
            .iter()
            .map(|(name, rel, count)| match rel {
                Some(r) => format!("- {} ({}) — {} interactions", name, r, count),
                None => format!("- {} — {} interactions", name, count),
            })
            .collect();
        append(&mut p, "People who recur", &lines.join("\n"));
    }

    // Places they return to (by visit count).
    let places: Vec<(String, Option<String>, i64)> = sqlx::query_as(
        "SELECT name, category, visit_count \
         FROM wiki_places WHERE visit_count > 0 \
         ORDER BY visit_count DESC LIMIT 6",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    if !places.is_empty() {
        let lines: Vec<String> = places
            .iter()
            .map(|(name, cat, count)| match cat {
                Some(c) => format!("- {} ({}) — {} visits", name, c, count),
                None => format!("- {} — {} visits", name, count),
            })
            .collect();
        append(&mut p, "Places they return to", &lines.join("\n"));
    }

    // Organizations.
    let orgs: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT canonical_name, organization_type \
         FROM wiki_orgs ORDER BY interaction_count DESC LIMIT 5",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    if !orgs.is_empty() {
        let lines: Vec<String> = orgs
            .iter()
            .map(|(name, kind)| match kind {
                Some(k) => format!("- {} ({})", name, k),
                None => format!("- {}", name),
            })
            .collect();
        append(&mut p, "Organizations", &lines.join("\n"));
    }

    // Recent day biographies — already-distilled meaning, the richest signal.
    let days: Vec<(chrono::NaiveDate, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT date, epigraph, autobiography FROM wiki_days \
         WHERE autobiography IS NOT NULL AND length(trim(autobiography)) > 0 \
         ORDER BY date DESC LIMIT 5",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    if !days.is_empty() {
        let lines: Vec<String> = days
            .iter()
            .map(|(date, epigraph, bio)| {
                let bio_txt = bio.clone().unwrap_or_default();
                let bio_preview: String = bio_txt.chars().take(400).collect();
                match epigraph {
                    Some(e) => format!("- {} — \"{}\"\n  {}", date, e, bio_preview),
                    None => format!("- {}\n  {}", date, bio_preview),
                }
            })
            .collect();
        append(&mut p, "Recent days, in their own record", &lines.join("\n"));
    }

    if thin {
        p.push_str(
            "\n(Only a little data has arrived so far — keep the draft to a short, honest sketch, and say plainly how early it is.)\n",
        );
    }

    if p.len() > MAX_TOTAL_CHARS {
        p.truncate(MAX_TOTAL_CHARS);
        p.push_str("\n\n(data truncated)");
    }

    p
}

fn append(prompt: &mut String, heading: &str, body: &str) {
    prompt.push_str(&format!("\n## {}\n{}\n", heading, body));
}

/// Call virtues-api for the draft — same bearer/System-purpose path as the day
/// summary (debits the OS reserve, not the user's chat budget).
async fn call_virtues_api(pool: &PgPool, user_prompt: &str) -> Result<String> {
    let chat_model = crate::api::assistant_profile::get_chat_model(pool).await?;

    let client = crate::virtues_api::client::BearerClient::from_env(pool.clone())
        .with_purpose(crate::virtues_api::client::Purpose::System)
        .with_feature("narrative_identity");
    let response = client
        .post_json(
            "/v1/ai/chat/completions",
            &serde_json::json!({
                "model": chat_model,
                "messages": [
                    {"role": "system", "content": SYSTEM_PROMPT},
                    {"role": "user", "content": user_prompt}
                ],
                "max_tokens": 600,
                "temperature": 0.4
            }),
        )
        .await
        .map_err(|e| Error::Network(format!("virtues-api request failed: {e}")))?;

    if !response.is_success() {
        let error_msg = match response.status {
            402 => "Usage limit reached for narrative identity".to_string(),
            429 => "Rate limited. Please try again later.".to_string(),
            _ => format!("virtues-api error {}: {}", response.status, response.body),
        };
        return Err(Error::ExternalApi(error_msg));
    }

    let content = response.body["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();

    Ok(content)
}

/// Strip an optional forward-compat marker + code fences and return the prose.
fn parse_paragraph(raw: &str) -> String {
    let mut s = raw.trim();
    if let Some(idx) = s.find("---PARAGRAPH---") {
        s = s[idx + "---PARAGRAPH---".len()..].trim();
    }
    s.trim_start_matches("```markdown")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_paragraph() {
        assert_eq!(parse_paragraph("  You tend to…  "), "You tend to…");
    }

    #[test]
    fn strips_marker_and_fences() {
        assert_eq!(
            parse_paragraph("preamble\n---PARAGRAPH---\n```\nYou tend to…\n```"),
            "You tend to…"
        );
    }

    #[test]
    fn richness_empty_when_no_signal() {
        let r = Richness {
            strong_people: 0,
            strong_places: 0,
            narrated_days: 0,
        };
        assert!(r.is_empty());
    }

    #[test]
    fn richness_not_empty_with_one_signal() {
        let r = Richness {
            strong_people: 0,
            strong_places: 0,
            narrated_days: 1,
        };
        assert!(!r.is_empty());
        assert_eq!(r.signal_total(), 1);
    }
}

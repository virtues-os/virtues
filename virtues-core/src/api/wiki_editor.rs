//! The wiki editor: one role, one constitution, one brief per subject kind.
//!
//! Design: `agents/plan/article-resolution-plan.md`. How the prompts are
//! organized and why: `agents/build/wiki-editor.md`.
//!
//! This module holds the parts that are pure functions of text — prompt
//! assembly, the lede, provenance, and the invariant the server enforces on
//! every machine edit. They are here rather than inside the applet because
//! they are the half of article resolution that can be tested without a model,
//! a CRDT or a scheduler, and because the invariant is the thing that makes
//! two pens on one article safe.

use crate::error::{Error, Result};

/// The rules that never vary, shared by every article. Compiled in, so the
/// file that is reviewed is the text that runs.
pub const CONSTITUTION: &str = include_str!("../../prompts/wiki/constitution.md");

/// What one KIND of page is. The constitution says how to write; a brief says
/// what this page is for.
pub const YEAR_BRIEF: &str = include_str!("../../prompts/wiki/year.md");
pub const ENTITY_BRIEF: &str = include_str!("../../prompts/wiki/entity.md");

/// Which brief a subject type gets.
///
/// `day` is absent deliberately: the day narrator is released, tuned, and
/// writes its first draft from its own prompt. It joins this door when its
/// REVISION does, which is the attention plan's work, not this one.
pub fn brief_for(subject_type: &str) -> Option<&'static str> {
    match subject_type {
        "year" => Some(YEAR_BRIEF),
        "person" | "place" | "organization" => Some(ENTITY_BRIEF),
        _ => None,
    }
}

/// The editor's system prompt: constitution, then brief, then the person's
/// standing rules.
///
/// The rules ride here for the same reason they ride in chat
/// (`api::chat`): without them the year article can name the thing the
/// assistant is forbidden to raise, which is worse than the assistant naming
/// it — prose persists, and a person meets it again every time they open the
/// page.
pub fn system_prompt(subject_type: &str, rules: &[String]) -> Result<String> {
    let brief = brief_for(subject_type).ok_or_else(|| {
        Error::InvalidInput(format!("No wiki brief for subject type {subject_type}"))
    })?;
    let mut p = String::with_capacity(CONSTITUTION.len() + brief.len() + 512);
    p.push_str(CONSTITUTION);
    p.push_str("\n\n---\n\n");
    p.push_str(brief);
    if !rules.is_empty() {
        p.push_str("\n\n---\n\n## Standing rules from the owner\n\n");
        p.push_str(
            "These are instructions, not context. They are absolute, they \
             outrank anything you infer from the record, and they apply to \
             this article as they apply to every other surface.\n\n",
        );
        for r in rules {
            p.push_str("- ");
            p.push_str(r.trim());
            p.push('\n');
        }
    }
    Ok(p)
}

/// The LEDE: an article's opening paragraph, which is the short form every
/// rung above it reads.
///
/// The first block that is neither blank nor a markdown heading. The narrate
/// prompt requires an article to open with a lede carrying no heading; the
/// skip exists because a human edit that adds a heading above it must not turn
/// that heading into the summary.
///
/// The same rule is written twice more, and all three must agree:
/// `api::wiki::day_lede_sql` for SQL callers, and `ledeOf` in the overview.
pub fn lede(article: &str) -> Option<&str> {
    article
        .split("\n\n")
        .map(str::trim)
        .find(|b| !b.is_empty() && !b.starts_with('#'))
}

/// Split prose into sentences, for provenance.
///
/// Deliberately crude, and biased toward splitting too little rather than too
/// much: a sentence that is really two costs nothing (it is compared whole),
/// while a sentence wrongly split at "Dr." or "3 p.m." would make the
/// invariant fail on text nobody touched. A known abbreviation or a one-letter
/// token therefore never ends a sentence, even when what follows looks like the
/// start of one.
pub fn sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        cur.push(c);
        if matches!(c, '.' | '!' | '?') {
            // A terminator only ends a sentence when whitespace and something
            // that looks like a new sentence follow it — and when the word it
            // closes is not an abbreviation. "Met Dr. Vance" is one sentence;
            // so is "at 3.30 p.m. The train was quiet", because a period after
            // a one-letter token is ambiguous even to a person and the safe
            // reading is the one that does not split.
            let word: String = cur
                .trim_end_matches(['.', '!', '?'])
                .chars()
                .rev()
                .take_while(|c| c.is_alphanumeric())
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            const ABBREV: &[&str] = &[
                "dr", "mr", "mrs", "ms", "prof", "st", "ave", "rd", "no", "vs",
                "jan", "feb", "mar", "apr", "jun", "jul", "aug", "sep", "sept",
                "oct", "nov", "dec", "approx", "etc", "eg", "ie",
            ];
            let abbrev = c == '.'
                && (word.chars().count() <= 1 || ABBREV.contains(&word.to_lowercase().as_str()));
            let mut look = chars.clone();
            let ws = matches!(look.peek(), Some(c) if c.is_whitespace());
            if ws && !abbrev {
                while matches!(look.peek(), Some(c) if c.is_whitespace()) {
                    look.next();
                }
                let starts_new = look
                    .peek()
                    .is_none_or(|n| n.is_uppercase() || n.is_ascii_digit() || *n == '[' || *n == '#');
                if starts_new {
                    let s = cur.trim().to_string();
                    if !s.is_empty() {
                        out.push(s);
                    }
                    cur.clear();
                }
            }
        }
    }
    let s = cur.trim().to_string();
    if !s.is_empty() {
        out.push(s);
    }
    out
}

/// What the person has done to an article since the editor last left it.
///
/// `machine_text` is the article exactly as the editor wrote it. Anything in
/// the live text that is not in it, they added; anything in it that is no
/// longer in the live text, they removed. That is the whole of provenance, and
/// it needs no per-character attribution and no version history.
///
/// Returns `(added, removed)` as sentences.
pub fn provenance(machine_text: &str, live_text: &str) -> (Vec<String>, Vec<String>) {
    let before = sentences(machine_text);
    let after = sentences(live_text);
    let added = after
        .iter()
        .filter(|s| !before.contains(s))
        .cloned()
        .collect();
    let removed = before
        .iter()
        .filter(|s| !after.contains(s))
        .cloned()
        .collect();
    (added, removed)
}

/// Fold a new observation of the person's edits into the stored sets.
///
/// Both sets are pruned against the live text so they cannot grow without
/// bound: a sentence they wrote and later deleted stops being theirs and
/// becomes a removal; a removal they typed back in themselves stops being a
/// removal.
pub fn fold_provenance(
    theirs: &[String],
    removed: &[String],
    added_now: &[String],
    removed_now: &[String],
    live_text: &str,
) -> (Vec<String>, Vec<String>) {
    let live = sentences(live_text);
    let mut t: Vec<String> = theirs
        .iter()
        .chain(added_now.iter())
        .filter(|s| live.contains(s))
        .cloned()
        .collect();
    t.dedup();
    let mut r: Vec<String> = removed
        .iter()
        .chain(removed_now.iter())
        .filter(|s| !live.contains(s))
        .cloned()
        .collect();
    r.dedup();
    (t, r)
}

/// Why a proposed edit was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum Violation {
    /// A sentence the person wrote is not present verbatim in the new text.
    LostTheirs(String),
    /// A sentence the person deleted has come back.
    RestoredRemoved(String),
    /// The article no longer opens with a paragraph.
    NoLede,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Violation::LostTheirs(s) => {
                write!(f, "a sentence the owner wrote did not survive: {s:?}")
            }
            Violation::RestoredRemoved(s) => {
                write!(f, "a sentence the owner deleted was restored: {s:?}")
            }
            Violation::NoLede => write!(f, "the article no longer opens with a paragraph"),
        }
    }
}

/// The invariant, checked on a scratch copy before anything reaches the live
/// document.
///
/// This is what makes two pens on one article safe, and it is the reason the
/// claim flip could be removed. The constitution ASKS the editor to leave
/// their words alone; this REFUSES the edit if it did not. A rule a prompt
/// merely states is a rule that holds most of the time.
pub fn check_edit(new_text: &str, theirs: &[String], removed: &[String]) -> Result<()> {
    if lede(new_text).is_none() {
        return Err(Error::InvalidInput(Violation::NoLede.to_string()));
    }
    for s in theirs {
        if !new_text.contains(s.as_str()) {
            return Err(Error::InvalidInput(
                Violation::LostTheirs(s.clone()).to_string(),
            ));
        }
    }
    let after = sentences(new_text);
    for s in removed {
        if after.contains(s) {
            return Err(Error::InvalidInput(
                Violation::RestoredRemoved(s.clone()).to_string(),
            ));
        }
    }
    Ok(())
}

/// The mechanical half of an edit summary: what changed, counted rather than
/// described.
///
/// The editor writes the "why" in its own words, and a caption can be wrong or
/// flattering about its own work ("improved the article" over a rewrite). This
/// line cannot be. Both go on the version, and the person reads them together.
pub fn change_line(before: &str, after: &str) -> String {
    let (sb, sa) = (sentences(before), sentences(after));
    let added = sa.iter().filter(|s| !sb.contains(s)).count();
    let cut = sb.iter().filter(|s| !sa.contains(s)).count();
    let para = |t: &str| t.split("\n\n").filter(|b| !b.trim().is_empty()).count() as i64;
    let dp = para(after) - para(before);
    let mut parts = Vec::new();
    if added > 0 {
        parts.push(format!("+{added} sentence{}", if added == 1 { "" } else { "s" }));
    }
    if cut > 0 {
        parts.push(format!("−{cut} sentence{}", if cut == 1 { "" } else { "s" }));
    }
    if dp != 0 {
        parts.push(format!("{dp:+} paragraph{}", if dp.abs() == 1 { "" } else { "s" }));
    }
    if parts.is_empty() {
        "no change".to_string()
    } else {
        parts.join(", ")
    }
}

/// An article the editor should look at on this run.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DueArticle {
    pub id: String,
    pub subject_type: String,
    pub subject_id: String,
    pub page_id: String,
    /// The article exactly as the editor last left it (§ provenance).
    pub machine_text: Option<String>,
    pub input_fingerprint: Option<String>,
}

/// At most this many articles per run. The ceiling is the cost control: there
/// is no per-day spend limit anywhere in the system, so the editor must carry
/// its own.
pub const MAX_PER_RUN: i64 = 3;

/// Articles whose rest is over, oldest edition first.
///
/// Three gates, and this is only the cheap two. **Eligible**: the person has
/// not switched maintenance off, and nobody has been editing it in the last
/// six hours — safe under the CRDT, still jarring to watch a paragraph change
/// under your cursor. **Ready**: the minimum interval has passed, or they
/// pressed update now.
///
/// The third gate — **drift**, whether the article's inputs actually moved —
/// is not here. It needs the fold input built, which costs a query per
/// article, so it is checked per candidate by the caller. An article whose
/// inputs have not changed is not rewritten; it just gets its fingerprint
/// stored and waits.
///
/// Note this selects from articles that EXIST. Whether an entity deserves one
/// in the first place is a separate question with a separate budget, because
/// "every entity on the box" is 226 articles nobody asked for on a five-month
/// record.
pub async fn due_articles(pool: &sqlx::PgPool, limit: i64) -> Result<Vec<DueArticle>> {
    sqlx::query_as::<_, DueArticle>(
        r#"
        SELECT id, subject_type, subject_id, page_id, machine_text, input_fingerprint
        FROM wiki_articles
        WHERE maintenance <> 'never'
          AND (last_human_edit_at IS NULL OR last_human_edit_at < now() - interval '6 hours')
          AND (
                update_requested_at IS NOT NULL
             OR last_written_at IS NULL
             OR last_written_at < now() - make_interval(days =>
                  CASE WHEN subject_type IN ('year', 'story') THEN 7 ELSE 30 END)
          )
        ORDER BY update_requested_at NULLS LAST, last_written_at NULLS FIRST
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to select due articles: {e}")))
}

/// A hash of what an article rests on.
///
/// The third gate. `due_articles` answers "has it rested long enough"; this
/// answers "did anything actually change". An article whose evidence is
/// identical to its last edition is not rewritten, which is what lets the
/// schedule be hourly without the cost being hourly.
///
/// Counts and latest timestamps rather than content: the same shape
/// `wiki_days.sources_fingerprint` has used to gate re-segmentation since it
/// shipped, and cheap enough to run on every candidate every hour. It moves
/// when a ref is added, a note is accepted, or the person edits an authored
/// field — which is exactly the set of things that should earn a new edition.
pub async fn evidence_fingerprint(
    pool: &sqlx::PgPool,
    article: &DueArticle,
) -> Result<String> {
    use sha2::Digest;

    // Refs touching the subject, and notes about it. Both are counted with
    // their latest timestamp, so a retraction moves the hash as surely as an
    // addition.
    let (refs, last_ref): (i64, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        "SELECT count(*), max(occurred_at) FROM wiki_refs WHERE entity_id = $1",
    )
    .bind(&article.subject_id)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to count refs: {e}")))?;

    let (notes, last_note): (i64, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        "SELECT count(*), max(resolved_at) FROM wiki_notes \
         WHERE subject_type = $1 AND subject_id = $2 AND resolved_at IS NOT NULL",
    )
    .bind(&article.subject_type)
    .bind(&article.subject_id)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to count notes: {e}")))?;

    let mut h = sha2::Sha256::new();
    h.update(
        format!(
            "refs:{refs}:{};notes:{notes}:{};",
            last_ref.map(|t| t.timestamp()).unwrap_or(0),
            last_note.map(|t| t.timestamp()).unwrap_or(0),
        )
        .as_bytes(),
    );
    Ok(format!("{:x}", h.finalize()))
}

/// Record that the editor has looked at an article.
///
/// Called on EVERY outcome, including a refusal: an article whose edit failed
/// its checks must still store the fingerprint it was refused for, or it comes
/// back next hour, fails the same way, and burns a model call an hour until
/// its interval elapses.
pub async fn record_pass(
    pool: &sqlx::PgPool,
    article_id: &str,
    fingerprint: &str,
    wrote: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "UPDATE wiki_articles \
         SET input_fingerprint = $2, \
             update_requested_at = NULL, \
             machine_text = COALESCE($3, machine_text), \
             last_written_at = CASE WHEN $3 IS NULL THEN last_written_at ELSE now() END, \
             updated_at = now() \
         WHERE id = $1",
    )
    .bind(article_id)
    .bind(fingerprint)
    .bind(wrote)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to record editor pass: {e}")))?;
    Ok(())
}

/// Store what the person has done to an article since the editor last wrote.
pub async fn record_provenance(
    pool: &sqlx::PgPool,
    article_id: &str,
    theirs: &[String],
    removed: &[String],
) -> Result<()> {
    sqlx::query("UPDATE wiki_articles SET theirs = $2, removed = $3 WHERE id = $1")
        .bind(article_id)
        .bind(serde_json::json!(theirs))
        .bind(serde_json::json!(removed))
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to record provenance: {e}")))?;
    Ok(())
}

/// The maintenance state of one article, as the editor needs it.
#[derive(Debug, Clone, sqlx::FromRow)]
struct EditorArticle {
    id: String,
    page_id: String,
    maintenance: String,
    machine_text: Option<String>,
    theirs: serde_json::Value,
    removed: serde_json::Value,
}

impl EditorArticle {
    fn provenance_sets(&self) -> (Vec<String>, Vec<String>) {
        let as_vec = |v: &serde_json::Value| -> Vec<String> {
            v.as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|s| s.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };
        (as_vec(&self.theirs), as_vec(&self.removed))
    }
}

/// Apply the editor's revision of one article.
///
/// This is the whole write path, and it runs on the server rather than in the
/// agent because every step after "here is the new text" is a guarantee the
/// agent must not be able to skip:
///
/// 1. Read the live document. If the person has edited it since the editor
///    last wrote, cut a `user` version first — otherwise their change would
///    sit invisibly inside the machine's next version — and fold what they did
///    into `theirs` / `removed`.
/// 2. Check the proposed text against those sets. A refusal returns the reason
///    rather than raising: the caller is an agent loop, and the reason is
///    something it can act on in the same turn.
/// 3. Apply only what changed, through the CRDT, with the staleness guard.
/// 4. Cut the machine's version, carrying the editor's own summary and a
///    mechanical count that cannot flatter itself.
///
/// Returns the line the agent should report.
pub async fn revise_article(
    pool: &sqlx::PgPool,
    yjs: &crate::server::yjs::YjsState,
    subject_type: &str,
    subject_id: &str,
    new_text: &str,
    summary: &str,
) -> Result<String> {
    // The editor's own columns, read directly: `wiki_articles::get_article`
    // is a compile-time-checked macro over a shared struct, and widening that
    // for one caller would put maintenance state on every reader of articles.
    let article: EditorArticle = sqlx::query_as(
        "SELECT id, page_id, maintenance, machine_text, theirs, removed \
         FROM wiki_articles WHERE subject_type = $1 AND subject_id = $2",
    )
    .bind(subject_type)
    .bind(subject_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to load article: {e}")))?
    .ok_or_else(|| Error::NotFound(format!("No article for {subject_type} {subject_id}")))?;

    if article.maintenance == "never" {
        return Err(Error::InvalidInput(
            "the owner has turned maintenance off for this article".into(),
        ));
    }
    let (theirs_stored, removed_stored) = article.provenance_sets();

    let live = yjs
        .read_text(&article.page_id)
        .await
        .map_err(|e| Error::Other(format!("could not read the article: {e}")))?;

    // ── 1. What the person has done since the editor last wrote ──
    let machine_text = article.machine_text.clone().unwrap_or_default();
    let (added, removed_now) = provenance(&machine_text, &live);
    let (theirs, removed) = fold_provenance(
        &theirs_stored,
        &removed_stored,
        &added,
        &removed_now,
        &live,
    );
    if !machine_text.is_empty() && live != machine_text {
        // Their edit becomes its own version, before the machine's, so the
        // history reads in the order the writing happened.
        if let Ok(state) = yjs.encoded_state(&article.page_id).await {
            let _ = crate::api::pages::create_version_from_snapshot(
                pool, &article.page_id, &state, &live, "user", Some("edited"),
            )
            .await;
        }
        record_provenance(pool, &article.id, &theirs, &removed).await?;
    }

    // ── 2. The invariant, before anything reaches the document ──
    check_edit(new_text, &theirs, &removed)?;

    // ── 3. Only what changed ──
    let applied = yjs
        .apply_text_diff(&article.page_id, &live, new_text)
        .await
        .map_err(Error::Other)?;

    // ── 4. The machine's version, with a summary that cannot flatter itself ──
    let change = change_line(&live, &applied);
    let description = format!("{} — {}", summary.trim(), change);
    if let Ok(state) = yjs.encoded_state(&article.page_id).await {
        let _ = crate::api::pages::create_version_from_snapshot(
            pool,
            &article.page_id,
            &state,
            &applied,
            "ai",
            Some(&description),
        )
        .await;
    }
    record_provenance(pool, &article.id, &theirs, &removed).await?;
    Ok(change)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_prompt_is_constitution_then_brief_then_rules() {
        let p = system_prompt("year", &["Never mention my father unless I do.".into()]).unwrap();
        let c = p.find("observe, never infer").expect("constitution present");
        let b = p.find("ONE YEAR of this person's life").expect("year brief");
        let r = p.find("Never mention my father").expect("rules");
        assert!(c < b && b < r, "constitution, then brief, then rules");
        assert!(
            p.contains("They are absolute"),
            "a rule must not read as context the model may weigh"
        );
    }

    #[test]
    fn a_subject_without_a_brief_is_refused_rather_than_written_blind() {
        // The day is the case that matters: its narrator is released and tuned,
        // and must not be silently handed the generic door.
        assert!(brief_for("day").is_none());
        assert!(system_prompt("day", &[]).is_err());
    }

    #[test]
    fn the_lede_skips_a_heading_the_person_added_above_it() {
        assert_eq!(lede("The day began early.\n\nThen more."), Some("The day began early."));
        assert_eq!(
            lede("# My own title\n\nThe day began early."),
            Some("The day began early."),
            "a heading a human typed above the lede must not become the summary"
        );
        assert_eq!(lede("   \n\n  "), None);
    }

    #[test]
    fn sentences_do_not_split_on_abbreviations_or_decimals() {
        let s = sentences("Met Dr. Vance at 3.30 p.m. The train was quiet.");
        assert_eq!(
            s,
            vec!["Met Dr. Vance at 3.30 p.m. The train was quiet."],
            "splitting too little is safe; splitting wrongly fails the invariant on \
             text nobody touched"
        );
        let two = sentences("The train was quiet. You walked from the station.");
        assert_eq!(two.len(), 2);
    }

    #[test]
    fn provenance_is_the_difference_from_what_the_editor_last_wrote() {
        let machine = "You went to the coast. The water was cold.";
        let live = "You went to the coast. I proposed there.";
        let (added, removed) = provenance(machine, live);
        assert_eq!(added, vec!["I proposed there."]);
        assert_eq!(removed, vec!["The water was cold."]);
    }

    #[test]
    fn folding_drops_a_sentence_they_later_deleted_and_a_removal_they_typed_back() {
        // Their own sentence, later deleted by them: stops being theirs, and
        // becomes something the editor must not restore.
        let (t, r) = fold_provenance(
            &["I proposed there.".into()],
            &[],
            &[],
            &["I proposed there.".into()],
            "You went to the coast.",
        );
        assert!(t.is_empty());
        assert_eq!(r, vec!["I proposed there."]);

        // A removal they typed back in themselves is no longer a removal.
        let (t2, r2) = fold_provenance(
            &[],
            &["The water was cold.".into()],
            &[],
            &[],
            "You went to the coast. The water was cold.",
        );
        assert!(r2.is_empty());
        assert!(t2.is_empty());
    }

    #[test]
    fn an_edit_that_loses_their_sentence_is_refused() {
        let theirs = vec!["I proposed there.".to_string()];
        assert!(check_edit("You went to the coast. I proposed there.", &theirs, &[]).is_ok());

        let err = check_edit("You went to the coast.", &theirs, &[])
            .unwrap_err()
            .to_string();
        assert!(err.contains("did not survive"), "{err}");

        // Paraphrase is loss. This is the whole point: the constitution asks,
        // and this refuses.
        let err = check_edit("You went to the coast. You proposed there.", &theirs, &[])
            .unwrap_err()
            .to_string();
        assert!(err.contains("did not survive"), "{err}");
    }

    #[test]
    fn an_edit_that_restores_what_they_deleted_is_refused() {
        let removed = vec!["The water was cold.".to_string()];
        let err = check_edit("You went to the coast. The water was cold.", &[], &removed)
            .unwrap_err()
            .to_string();
        assert!(err.contains("was restored"), "{err}");
        assert!(check_edit("You went to the coast.", &[], &removed).is_ok());
    }

    /// Seed an article with a page, and set the maintenance columns directly.
    #[cfg(test)]
    async fn seed(
        pool: &sqlx::PgPool,
        subject_id: &str,
        maintenance: &str,
        written_days_ago: Option<i32>,
    ) -> String {
        let a = crate::api::wiki_articles::create_article(
            pool, "person", subject_id, subject_id, "A first draft.",
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE wiki_articles SET maintenance = $2, \
             last_written_at = CASE WHEN $3::int IS NULL THEN NULL \
                               ELSE now() - make_interval(days => $3::int) END \
             WHERE id = $1",
        )
        .bind(&a.id)
        .bind(maintenance)
        .bind(written_days_ago)
        .execute(pool)
        .await
        .unwrap();
        a.id
    }

    #[sqlx::test]
    async fn due_skips_what_is_switched_off_and_what_is_still_resting(pool: sqlx::PgPool) {
        for (id, name) in [("person_a", "Ada"), ("person_b", "Bo"), ("person_c", "Cy")] {
            sqlx::query("INSERT INTO wiki_people (id, name) VALUES ($1, $2)")
                .bind(id)
                .bind(name)
                .execute(&pool)
                .await
                .unwrap();
        }
        let off = seed(&pool, "person_a", "never", Some(400)).await;
        let resting = seed(&pool, "person_b", "auto", Some(3)).await;
        let due = seed(&pool, "person_c", "auto", Some(90)).await;

        let got: Vec<String> = due_articles(&pool, 10)
            .await
            .unwrap()
            .into_iter()
            .map(|a| a.id)
            .collect();
        assert!(!got.contains(&off), "maintenance 'never' means never");
        assert!(!got.contains(&resting), "inside its interval");
        assert!(got.contains(&due));
    }

    #[sqlx::test]
    async fn update_now_beats_the_interval_and_a_recent_human_edit_defers(pool: sqlx::PgPool) {
        sqlx::query("INSERT INTO wiki_people (id, name) VALUES ('person_d', 'Di')")
            .execute(&pool)
            .await
            .unwrap();
        let id = seed(&pool, "person_d", "auto", Some(1)).await;
        assert!(due_articles(&pool, 10).await.unwrap().is_empty());

        sqlx::query("UPDATE wiki_articles SET update_requested_at = now() WHERE id = $1")
            .bind(&id)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(due_articles(&pool, 10).await.unwrap().len(), 1, "asked for");

        // Somebody is in the page right now. Even "update now" waits: the CRDT
        // makes it safe, not unsurprising.
        sqlx::query("UPDATE wiki_articles SET last_human_edit_at = now() WHERE id = $1")
            .bind(&id)
            .execute(&pool)
            .await
            .unwrap();
        assert!(due_articles(&pool, 10).await.unwrap().is_empty());
    }

    #[sqlx::test]
    async fn a_refused_pass_still_stores_its_fingerprint(pool: sqlx::PgPool) {
        sqlx::query("INSERT INTO wiki_people (id, name) VALUES ('person_e', 'Eli')")
            .execute(&pool)
            .await
            .unwrap();
        let id = seed(&pool, "person_e", "auto", Some(90)).await;

        let before: Option<chrono::DateTime<chrono::Utc>> =
            sqlx::query_scalar("SELECT last_written_at FROM wiki_articles WHERE id = $1")
                .bind(&id)
                .fetch_one(&pool)
                .await
                .unwrap();

        // A pass that wrote nothing: the article must not come back next hour
        // to fail the same way and burn a model call an hour.
        record_pass(&pool, &id, "fp-1", None).await.unwrap();
        let (fp, written): (Option<String>, Option<chrono::DateTime<chrono::Utc>>) =
            sqlx::query_as("SELECT input_fingerprint, last_written_at FROM wiki_articles WHERE id = $1")
                .bind(&id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(fp.as_deref(), Some("fp-1"), "the refusal is remembered");
        assert_eq!(
            written, before,
            "nothing was written, so the edition date must not move — \
             otherwise a refusal reads as an edit and resets the interval"
        );

        record_pass(&pool, &id, "fp-2", Some("The new text.")).await.unwrap();
        let (mt, written): (Option<String>, Option<chrono::DateTime<chrono::Utc>>) =
            sqlx::query_as("SELECT machine_text, last_written_at FROM wiki_articles WHERE id = $1")
                .bind(&id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(mt.as_deref(), Some("The new text."));
        assert!(written.is_some());
    }

    #[test]
    fn the_change_line_counts_rather_than_describes() {
        assert_eq!(
            change_line("A thing happened.", "A thing happened.\n\nAnother thing did."),
            "+1 sentence, +1 paragraph"
        );
        assert_eq!(change_line("A thing happened.", "A thing happened."), "no change");
    }
}

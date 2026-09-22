//! An entity's FIRST DRAFT — the opening article on a person, place or
//! organization page.
//!
//! **This module writes once.** It is the one-shot half of article resolution
//! (`agents/record/article-resolution.md`): there is no existing text to
//! respect and nothing to go looking for, so one call with a dossier in the
//! prompt is the whole job. Everything after the first draft — deciding the
//! article is due, researching what changed, and making a surgical edit that
//! cannot lose a sentence the person wrote — belongs to the `wiki_editor`
//! applet and its agent phase, which is the only place a Yjs-aware writer can
//! run.
//!
//! It is an *article*, not a summary: it summarizes nothing, because the thing
//! it would summarize (the records) is rendered directly beneath it on the
//! page. It is the prose the page is written in.
//!
//! Shape: dossier → `BearerClient` → `create_article`. The prose lands in the
//! article's own page, where the person's edits and the editor's share one
//! document.
//!
//! **The gate is consent.** An entity has no article until someone asks for
//! one, and none is maintained until they set `maintenance` on it. Writing
//! once and maintaining forever are different decisions and get different
//! switches. The thresholds that used to decide this for them
//! (`MIN_REFS_TO_WRITE`, `MIN_NEW_REFS`) are gone: on the real box they
//! cleared 226 entities on five months of records — hundreds of unrequested
//! model calls, recurring forever, with nothing in the UI to say the box was
//! spending on them.

use sqlx::PgPool;

use crate::error::{Error, Result};

/// Most recent records shown to the model.
const DOSSIER_RECORDS: usize = 40;

/// Hard cap on dossier characters.
const MAX_TOTAL_CHARS: usize = 14000;

/// Two measured failures shaped this prompt, and both were the prompt's own
/// doing rather than the model's — see the note above `call_virtues_api`.
///
/// **"the owner" appeared in half the articles** (10 of the first 20 written on
/// a real box). The old first paragraph said `"You"/"your" in the article
/// always refers to the wiki's owner`, and the word leaked straight through:
/// "Maya is a recurring presence in the owner's digital life". An instruction
/// about a word teaches the word. The rule is now stated without ever naming
/// the reader as a role, and carries the wrong/right pair instead.
///
/// **"the record" appeared in 20 of 20**, and in 8 of 8 from every model
/// benched against it — so it was never a model tic. Two lines asked for it:
/// "then how it shows up in the record", and "if the record is one-sided …
/// say so plainly". A slot gets filled; that is what a slot is for. Both are
/// gone, replaced by slots that are about the subject, and the reason the
/// article must not describe its own evidence is now stated, because a rule
/// with a reason survives paraphrase and a bare prohibition gets routed
/// around.
const SYSTEM_PROMPT: &str = r#"You are the editor of a private wiki about one person's life — their own personal wikipedia, readable only by them. You are writing the article for ONE subject in that wiki: a person they know, a place they go, or an organization in their life.

WHO IS SPEAKING, AND TO WHOM:
- The article is written TO the person whose wiki this is. In every sentence they are "you" and "your". They are never described in the third person and never named as a role.
- The subject is written about in the third person: she, he, they, it.
- Not "Maya is a recurring presence in the owner's digital life." Instead: "Maya writes to you most mornings, usually before you are up."

You are given the subject's structured facts, the raw records that reference it (messages, emails, calendar events, visits, transactions), and narrated days it appears in.

WRITE:
- Plain paragraphs in the register of a well-edited encyclopedia that happens to be about a private life: precise, warm, unhurried. Markdown is allowed but no headings and no lists.
- LENGTH FOLLOWS THE EVIDENCE. A subject with years of material behind it earns three or four paragraphs; one with a handful of traces earns a few sentences and stops. Never pad a thin subject — and never compress a rich one, because the person most present in a life must not come out with the shortest article.
- Open with who or what the subject is to you. Then what recurs: when, where, and what about. Then what has changed, where the material shows a change.
- Write about the SUBJECT, never about the evidence. The records are printed on the page directly beneath this article, so a sentence describing them is a sentence the reader is about to read twice. Nothing about data, sources, volumes, messages-as-a-category, or what is and is not documented.
- Concrete over general, every time: a thing she actually said, the street you actually walk, the hour you are usually there. A sentence that could be true of a hundred people is a wasted sentence.
- Claim only what the material carries. Never invent feelings, motives, or events. No flattery, no horoscope lines.
- The messages, the exchanges, the texts, the log: these are not objects the article may talk about. Write what was said and what was done.
- Patterns, never essence: "your lunches with her tend to…", never "she is the kind of person who…".
- LINK entities: when you mention an entity listed under "Entities you may link", link it by copying its exact markdown link, e.g. [Maya](/person/person_ab12) or [March 3, 2026](/day/day_2026-03-03) for a listed day. Link each once, on first mention. Never invent a link or link anything not listed.
- This is the article's FIRST edition. Write it whole. It is maintained afterwards by editing, not by rewriting, so do not write anything that would have to be replaced wholesale to stay true.

Output only the article."#;

/// One due entity: id + which table it lives in.
#[derive(Debug, Clone)]
struct DueEntity {
    id: String,
    kind: String, // 'person' | 'place' | 'organization'
    refs: i64,
}


/// Write a subject's first article, now, because someone asked for it.
///
/// This is the create path, and it is the one an applet subprocess can safely
/// take: a page with `content` and no `yjs_state` seeds its CRDT correctly on
/// first open. Re-writing an existing article is refused rather than silently
/// dropped — see `refresh_due_entity_articles`.
pub async fn write_entity_article_now(
    pool: &PgPool,
    subject_type: &str,
    subject_id: &str,
) -> Result<crate::api::wiki_articles::Article> {
    if let Some(existing) = crate::api::wiki_articles::get_article(pool, subject_type, subject_id).await? {
        return Err(Error::InvalidInput(format!(
            "{subject_type} already has an article (page {}). Rewriting an existing              article needs the agent phase, so it is not available yet.",
            existing.page_id
        )));
    }

    let refs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wiki_refs WHERE entity_id = $1",
    )
    .bind(subject_id)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to count refs: {}", e)))?;

    let entity = DueEntity {
        id: subject_id.to_string(),
        kind: subject_type.to_string(),
        refs,
    };

    let (prompt, allowed) = build_dossier(pool, &entity).await?;
    tracing::info!(
        entity = %entity.id, kind = %entity.kind, refs,
        prompt_chars = prompt.len(),
        "writing first entity article (user-requested)"
    );

    let raw = call_virtues_api(pool, &prompt).await?;
    let article = parse_article(&raw);
    // The allowlist is enforced here, not requested in the prompt. See
    // `sanitize_links`: asked nicely, the model got 0 of 3 entity links right.
    let self_href = format!(
        "/{}/{}",
        if subject_type == "organization" { "org" } else { subject_type },
        subject_id
    );
    let (article, stripped) = sanitize_links(&article, &allowed, &self_href);
    if stripped > 0 {
        tracing::warn!(
            entity = %entity.id, kind = %entity.kind, stripped,
            "unwrapped links the article was not entitled to make"
        );
    }
    if article.is_empty() {
        return Err(Error::ExternalApi(
            "LLM returned an empty entity article".to_string(),
        ));
    }

    let title = entity_title(pool, subject_type, subject_id).await?;
    let created =
        crate::api::wiki_articles::create_article(pool, subject_type, subject_id, &title, &article)
            .await?;

    // Record what the editor just wrote. Without this the article has no
    // `machine_text`, and the first revision cannot tell the machine's own
    // first draft from something the person typed — it would mark the whole
    // article as theirs and then be forbidden from ever editing it.
    crate::api::wiki_editor::record_edition(pool, &created.id, &article).await?;

    Ok(created)
}

/// The subject's display name, for the article page's title.
/// Named `entity_*` and taking `entity_*` because that is what it means: the
/// three ENTITY-shaped subjects and no others. A subject is the wider word —
/// a day and a year are subjects too — and this function has nothing to say
/// about them. See `agents/build/glossary.md`.
async fn entity_title(pool: &PgPool, entity_type: &str, entity_id: &str) -> Result<String> {
    let sql = match entity_type {
        "person" => "SELECT name FROM wiki_people WHERE id = $1",
        "place" => "SELECT name FROM wiki_places WHERE id = $1",
        "organization" => "SELECT name FROM wiki_orgs WHERE id = $1",
        other => {
            return Err(Error::InvalidInput(format!(
                "Not an entity: {other}. A day or a year is a subject with an \
                 article, but it is not something this writer can title."
            )))
        }
    };
    sqlx::query_scalar(sql)
        .bind(entity_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to load the entity: {}", e)))?
        .ok_or_else(|| Error::NotFound(format!("No {entity_type}: {entity_id}")))
}

/// Assemble everything the editor reads: header facts, the recent record,
/// co-occurring entities (the link allowlist), and narrated days.
///
/// It deliberately reads NO never-written column. `seen_count`, `first_seen`,
/// `last_seen`, `article` and `wiki_people.notes` have no writer anywhere, so
/// every one of them either fed the model a zero or fed it nothing — and
/// "Interactions on record: 0" was reaching the prompt for entities with
/// thousands of refs. The honest count is `entity.refs`, straight from
/// `wiki_refs`, and it is already in the header line.
async fn build_dossier(pool: &PgPool, entity: &DueEntity) -> Result<(String, LinkAllowlist)> {
    use sqlx::Row;

    let mut p = String::new();
    let mut allowed = LinkAllowlist::default();

    // ── Header facts + previous edition, per kind ──
    let (name, facts): (String, String) = match entity.kind.as_str() {
        "person" => {
            let row = sqlx::query(
                "SELECT name, relationship_category, nickname \
                 FROM wiki_people WHERE id = $1",
            )
            .bind(&entity.id)
            .fetch_one(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to load person: {}", e)))?;
            let name: String = row.get("name");
            let mut f = String::new();
            if let Ok(Some(v)) = row.try_get::<Option<String>, _>("relationship_category") {
                f.push_str(&format!("- Relationship: {}\n", v));
            }
            if let Ok(Some(v)) = row.try_get::<Option<String>, _>("nickname") {
                f.push_str(&format!("- Nickname: {}\n", v));
            }
            (name, f)
        }
        "place" => {
            let row = sqlx::query(
                "SELECT name, category, address FROM wiki_places WHERE id = $1",
            )
            .bind(&entity.id)
            .fetch_one(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to load place: {}", e)))?;
            let name: String = row.get("name");
            let mut f = String::new();
            if let Ok(Some(v)) = row.try_get::<Option<String>, _>("category") {
                f.push_str(&format!("- Category: {}\n", v));
            }
            if let Ok(Some(v)) = row.try_get::<Option<String>, _>("address") {
                f.push_str(&format!("- Address: {}\n", v));
            }
            (name, f)
        }
        _ => {
            let row = sqlx::query(
                "SELECT name, organization_type, relationship_type, role_title \
                 FROM wiki_orgs WHERE id = $1",
            )
            .bind(&entity.id)
            .fetch_one(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to load org: {}", e)))?;
            let name: String = row.get("name");
            let mut f = String::new();
            if let Ok(Some(v)) = row.try_get::<Option<String>, _>("organization_type") {
                f.push_str(&format!("- Type: {}\n", v));
            }
            if let Ok(Some(v)) = row.try_get::<Option<String>, _>("relationship_type") {
                f.push_str(&format!("- Relationship: {}\n", v));
            }
            if let Ok(Some(v)) = row.try_get::<Option<String>, _>("role_title") {
                f.push_str(&format!("- Owner's role: {}\n", v));
            }
            (name, f)
        }
    };

    p.push_str(&format!(
        "The article is about: {} ({}), total records referencing it: {}\n",
        name, entity.kind, entity.refs
    ));
    if !facts.is_empty() {
        p.push_str(&format!("\n## Structured facts\n{}", facts));
    }

    // ── The recent record ──
    let page = super::wiki::get_entity_records_page(
        pool,
        &entity.id,
        0,
        DOSSIER_RECORDS as i64,
        "",
        &[],
        true,
    )
    .await?;
    if !page.items.is_empty() {
        let lines: Vec<String> = page
            .items
            .iter()
            .map(|r| {
                let role = r.role.as_deref().map(|x| format!(" [{}]", x)).unwrap_or_default();
                let preview = r
                    .preview
                    .as_deref()
                    .map(|x| format!(" — {}", cap(x, 160)))
                    .unwrap_or_default();
                format!(
                    "- {} {}{}: {}{}",
                    r.timestamp.format("%Y-%m-%d"),
                    r.source_type,
                    role,
                    cap(&r.label, 120),
                    preview
                )
            })
            .collect();
        p.push_str(&format!(
            "\n## The record (most recent {} of {})\n{}\n",
            lines.len(),
            page.total,
            lines.join("\n")
        ));
    }

    // ── Link allowlist: co-occurring entities + narrated days ──
    let mut links: Vec<String> = Vec::new();
    let co: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT DISTINCT er2.entity_type, er2.entity_id
        FROM wiki_refs er1
        JOIN wiki_refs er2
          ON er1.source_table = er2.source_table AND er1.source_id = er2.source_id
        WHERE er1.entity_id = $1 AND er2.entity_id <> $1
        LIMIT 12
        "#,
    )
    .bind(&entity.id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for (etype, eid) in &co {
        let (route, name_sql) = match etype.as_str() {
            "person" => ("person", "SELECT name FROM wiki_people WHERE id = $1"),
            "place" => ("place", "SELECT name FROM wiki_places WHERE id = $1"),
            "organization" => ("org", "SELECT name FROM wiki_orgs WHERE id = $1"),
            _ => continue,
        };
        if let Ok(Some(n)) = sqlx::query_scalar::<_, String>(name_sql)
            .bind(eid)
            .fetch_optional(pool)
            .await
        {
            // Offered to the model AND remembered, so the answer can be held
            // to the same list rather than trusted to have read it.
            allowed.push_entity(format!("/{route}/{eid}"), n.clone());
            links.push(format!("- [{}](/{}/{})", n, route, eid));
        }
    }
    // The day's LEDE, not its `epigraph`: that column is NULL on every row of
    // every box, because the narrate prompt forbids the model to write one.
    // Six narrated days were being offered to the editor as bare dates.
    let days: Vec<(chrono::NaiveDate, Option<String>)> = sqlx::query_as(&format!(
        r#"
        SELECT DISTINCT d.date, {lede} AS lede
        FROM wiki_days d
        JOIN wiki_day_prose dp ON dp.day_id = d.id AND dp.prose IS NOT NULL
        JOIN wiki_refs er ON date(er.occurred_at) = d.date
        WHERE er.entity_id = $1
        ORDER BY d.date DESC
        LIMIT 6
        "#,
        lede = crate::api::wiki_editor::lede_sql("dp.prose")
    ))
    .bind(&entity.id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for (date, lede) in &days {
        let label = date.format("%B %-d, %Y");
        allowed.push_day(format!("/day/day_{}", date.format("%Y-%m-%d")));
        match lede {
            Some(e) => links.push(format!(
                "- [{}](/day/day_{}) — narrated day: \"{}\"",
                label,
                date.format("%Y-%m-%d"),
                cap(e, 200)
            )),
            None => links.push(format!("- [{}](/day/day_{})", label, date.format("%Y-%m-%d"))),
        }
    }
    if !links.is_empty() {
        p.push_str(&format!(
            "\n## Entities you may link (copy the exact markdown link)\n{}\n",
            links.join("\n")
        ));
    }

    if p.len() > MAX_TOTAL_CHARS {
        p.truncate(MAX_TOTAL_CHARS);
        p.push_str("\n\n(material truncated)");
    }

    Ok((p, allowed))
}

fn cap(s: &str, n: usize) -> String {
    let t = s.trim();
    if t.chars().count() <= n {
        return t.to_string();
    }
    let mut out: String = t.chars().take(n).collect();
    out.push('…');
    out
}

/// Runs on the **Lite** slot, not Chat.
///
/// Background writing must not ride the slot the owner picked for
/// conversation. It silently did, and the effect was that choosing a premium
/// model to talk to made every applet premium too — the same call costing 15×
/// more without anyone deciding that. There is also no per-day spend ceiling
/// anywhere in the system, so the model slot is the actual cost control.
///
/// The shared completion helper resolves the Lite slot through the owner's
/// background pin — that pin exists exactly for jobs like this one.
async fn call_virtues_api(pool: &PgPool, user_prompt: &str) -> Result<String> {
    crate::virtues_api::completion::system_completion(
        pool,
        virtues_registry::models::ModelSlot::Lite,
        "entity_article",
        SYSTEM_PROMPT,
        user_prompt,
        // Arrangement of the person's own words: thinking off, no cap. The
        // 900-token literal that sat here was a guess that stopped being
        // true the day the model started thinking inside it.
        crate::virtues_api::request::Thinking::Off,
        0.4,
    )
    .await
}

/// The links an article is allowed to carry, collected while the dossier is
/// built — the same list the prompt prints under "Entities you may link".
///
/// It exists because the prompt asking nicely does not work. Audited across 24
/// generated articles on a real box, `[Name](/person/…)` links were **0
/// correct and 3 mismatched**: one article labelled the SUBJECT'S OWN NAME
/// with a different person's id, then invented a second name for that same
/// wrong id. Day links were fine, because a date describes itself and the
/// model copies it.
///
/// A ref that points at the wrong person is worse than no ref: the prose reads
/// as sourced, the chip opens someone else's page, and nothing about it looks
/// broken.
#[derive(Debug, Default, Clone)]
struct LinkAllowlist {
    /// `/person/<id>` → the name that id actually has.
    entities: std::collections::HashMap<String, String>,
    /// `/day/day_YYYY-MM-DD` hrefs that were offered.
    days: std::collections::HashSet<String>,
}

impl LinkAllowlist {
    fn push_entity(&mut self, href: String, name: String) {
        self.entities.insert(href, name);
    }
    fn push_day(&mut self, href: String) {
        self.days.insert(href);
    }
}

/// Unwrap every link the article was not entitled to make, keeping the words.
///
/// Three ways a link dies, and all of them leave the sentence intact — the
/// prose is usually right about the person and wrong only about the pointer,
/// so `[Maya](/person/wrong_id)` becomes `Maya` rather than disappearing.
///
///   1. The href was never offered.
///   2. The href was offered, but under a different name. This is the observed
///      failure: the right words over someone else's id.
///   3. The href is the article's own subject. A page does not link itself.
///
/// A day link is checked for membership only. Its label is a rendering of the
/// date in the href — "September 2, 2026", "September 2" — and requiring one
/// spelling would strip correct links.
///
/// External links are left alone; the prompt does not ask for them and the
/// renderer already treats them as citations rather than refs.
fn sanitize_links(article: &str, allowed: &LinkAllowlist, self_href: &str) -> (String, usize) {
    static LINK: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = LINK.get_or_init(|| {
        regex::Regex::new(r"\[([^\]]*)\]\((/[^)\s]*)\)").expect("static link regex")
    });

    let mut stripped = 0usize;
    let out = re
        .replace_all(article, |c: &regex::Captures<'_>| {
            let label = &c[1];
            let href = &c[2];
            let keep = if href == self_href {
                false
            } else if href.starts_with("/day/") {
                allowed.days.contains(href)
            } else {
                allowed
                    .entities
                    .get(href)
                    .is_some_and(|name| name.trim().eq_ignore_ascii_case(label.trim()))
            };
            if keep {
                c[0].to_string()
            } else {
                stripped += 1;
                label.to_string()
            }
        })
        .into_owned();
    (out, stripped)
}

/// Strip code fences and return the article prose.
fn parse_article(raw: &str) -> String {
    raw.trim()
        .trim_start_matches("```markdown")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_strips_fences() {
        assert_eq!(parse_article("```markdown\nAn article.\n```"), "An article.");
    }

    #[test]
    fn cap_appends_ellipsis() {
        assert_eq!(cap("abcdef", 3), "abc…");
        assert_eq!(cap("abc", 3), "abc");
    }

    /// THE TEST THAT WOULD HAVE CAUGHT THIS. The header queries here are raw
    fn allowlist() -> LinkAllowlist {
        let mut a = LinkAllowlist::default();
        a.push_entity("/person/person_real".into(), "Nick".into());
        a.push_entity("/org/org_real".into(), "Example Ltd".into());
        a.push_day("/day/day_2026-09-02".into());
        a
    }

    /// The failure this function exists for, reproduced from a real box: the
    /// article named one person and pointed the link at another. The words are
    /// right and the pointer is wrong, so the words stay.
    #[test]
    fn a_right_name_over_the_wrong_id_loses_its_link() {
        let (out, n) = sanitize_links(
            "[Nick](/person/person_someone_else) writes to you most mornings.",
            &allowlist(),
            "/person/person_subject",
        );
        assert_eq!(n, 1);
        assert_eq!(out, "Nick writes to you most mornings.");
    }

    /// The other half of the same failure: an invented name on an id that IS
    /// on the list. Membership alone would have passed this.
    #[test]
    fn an_invented_name_on_a_listed_id_loses_its_link() {
        let (out, n) = sanitize_links(
            "You met [David Okafor](/person/person_real) for lunch.",
            &allowlist(),
            "/person/person_subject",
        );
        assert_eq!(n, 1);
        assert_eq!(out, "You met David Okafor for lunch.");
    }

    #[test]
    fn a_listed_entity_under_its_own_name_survives() {
        let a = allowlist();
        for text in [
            "[Nick](/person/person_real) called.",
            "[nick](/person/person_real) called.",
            "[Example Ltd](/org/org_real) invoiced you.",
        ] {
            let (out, n) = sanitize_links(text, &a, "/person/person_subject");
            assert_eq!(n, 0, "stripped a good link: {text}");
            assert_eq!(out, text);
        }
    }

    /// A day's label is a rendering of the date already in its href, so the
    /// spelling is not checked — only whether the day was offered.
    #[test]
    fn a_listed_day_survives_any_rendering_of_its_date() {
        let a = allowlist();
        for text in [
            "on [September 2, 2026](/day/day_2026-09-02)",
            "on [September 2](/day/day_2026-09-02)",
            "on [that Wednesday](/day/day_2026-09-02)",
        ] {
            let (out, n) = sanitize_links(text, &a, "/person/person_subject");
            assert_eq!(n, 0, "stripped a good day link: {text}");
            assert_eq!(out, text);
        }
        let (out, n) = sanitize_links(
            "on [September 9, 2026](/day/day_2026-09-09)",
            &a,
            "/person/person_subject",
        );
        assert_eq!(n, 1, "a day that was never offered is not a link");
        assert_eq!(out, "on September 9, 2026");
    }

    #[test]
    fn a_page_does_not_link_itself() {
        let (out, n) = sanitize_links(
            "[Nick](/person/person_subject) is your brother.",
            &allowlist(),
            "/person/person_subject",
        );
        assert_eq!(n, 1);
        assert_eq!(out, "Nick is your brother.");
    }

    #[test]
    fn external_links_and_plain_prose_are_left_alone() {
        let text = "See [their site](https://example.com) — otherwise plain prose with [brackets] and (parens).";
        let (out, n) = sanitize_links(text, &allowlist(), "/person/person_subject");
        assert_eq!(n, 0);
        assert_eq!(out, text);
    }

    /// `sqlx::query`, so a renamed column breaks at runtime, not build time —
    /// which is how a phantom `ref_count` (renamed to `seen_count` in 0002)
    /// silently killed person and place article generation while org survived.
    /// This runs all three header paths against the real migration-built
    /// schema; a future rename fails here instead of on a fielded box.
    #[sqlx::test]
    async fn dossier_header_sql_matches_schema(pool: sqlx::PgPool) {
        for (table, id, kind) in [
            ("wiki_people", "person_t1", "person"),
            ("wiki_places", "place_t1", "place"),
            ("wiki_orgs", "org_t1", "organization"),
        ] {
            sqlx::query(&format!(
                "INSERT INTO {table} (id, name) VALUES ($1, 'Dossier Subject')"
            ))
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();

            let entity = DueEntity { id: id.to_string(), kind: kind.to_string(), refs: 1 };
            let (dossier, _allowed) = build_dossier(&pool, &entity)
                .await
                .unwrap_or_else(|e| panic!("{kind} dossier failed against live schema: {e}"));
            assert!(dossier.contains("Dossier Subject"), "{kind} dossier missing subject name");
        }
    }
}

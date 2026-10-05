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
//! **The gate is consent, and it is ONE gate.** An entity has no article until
//! someone asks for one. The thresholds that used to decide this for them
//! (`MIN_REFS_TO_WRITE`, `MIN_NEW_REFS`) are gone: on the real box they
//! cleared 226 entities on five months of records — hundreds of unrequested
//! model calls, recurring forever, with nothing in the UI to say the box was
//! spending on them.
//!
//! Asking for the article DOES enroll it in maintenance: the column defaults
//! to `auto` and nothing here overrides it. There is one switch, because an
//! article worth writing is worth keeping true; a stale article about someone
//! you now see weekly is a WRONG article, which is the failure this design
//! fears most.
//!
//! What makes that safe is not a second consent but four small gates, all in
//! `wiki_editor`: `maintenance <> 'never'`, no human edit inside six hours,
//! rested past its interval (30 days for an entity, 7 for a year or story),
//! and the evidence fingerprint actually moved — then one article per applet
//! run, at most one run an hour. And it is disclosed where it is read rather
//! than in a settings page: every article carries a colophon saying "The
//! record wrote this and keeps it current", with the off switch in that line.

use sqlx::PgPool;

use crate::error::{Error, Result};

/// Most recent records shown to the model.
const DOSSIER_RECORDS: usize = 40;

/// Dossier size, in characters. The records fill whatever the rest leaves.
const MAX_TOTAL_CHARS: usize = 14000;

/// The leading lines whose total, newlines included, fits in `room` characters.
fn lines_that_fit(lines: &[String], room: usize) -> Vec<String> {
    let mut used = 0;
    lines
        .iter()
        .take_while(|l| {
            used += l.chars().count() + 1;
            used <= room
        })
        .cloned()
        .collect()
}
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
/// dropped: a subprocess has no `YjsState`, so its rewrite of a page with live
/// CRDT state would be discarded by the next save. Revisions go through the
/// wiki_editor applet's AGENT phase.
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

    // The constitution and the entity brief, plus the owner's standing rules:
    // the same law the editor revises under, so a draft and its later
    // revisions cannot be written to different rules.
    let system = crate::api::wiki_editor::system_prompt(
        subject_type,
        &crate::api::wiki_editor::standing_rules(pool).await?,
    )?;
    let raw = call_virtues_api(pool, &system, &prompt).await?;
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
    if !matches!(entity_type, "person" | "place" | "organization") {
        return Err(Error::InvalidInput(format!(
            "Not an entity: {entity_type}. A day or a year is a subject with an \
             article, but it is not something this writer can title."
        )));
    }
    crate::api::wiki_articles::subject_name(pool, entity_type, entity_id)
        .await?
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
    .map_err(|e| Error::Database(format!("Failed to read co-occurring subjects: {e}")))?;
    for (etype, eid) in &co {
        let (route, name_sql) = match etype.as_str() {
            "person" => ("person", "SELECT name FROM wiki_people WHERE id = $1"),
            "place" => ("place", "SELECT name FROM wiki_places WHERE id = $1"),
            "organization" => ("org", "SELECT name FROM wiki_orgs WHERE id = $1"),
            _ => continue,
        };
        let name = sqlx::query_scalar::<_, String>(name_sql)
            .bind(eid)
            .fetch_optional(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to read a linked subject's name: {e}")))?;
        if let Some(n) = name {
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
    .map_err(|e| Error::Database(format!("Failed to read the subject's days: {e}")))?;
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
        let all: Vec<String> = page
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
        // Records come last and take what room is left: newest first, whole
        // lines only, so the facts and the link list the answer is checked
        // against are never what gets cut.
        let room = MAX_TOTAL_CHARS.saturating_sub(p.chars().count() + 200);
        let lines = lines_that_fit(&all, room);
        p.push_str(&format!(
            "\n## The record (most recent {} of {})\n{}\n",
            lines.len(),
            page.total,
            lines.join("\n")
        ));
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
async fn call_virtues_api(pool: &PgPool, system_prompt: &str, user_prompt: &str) -> Result<String> {
    crate::virtues_api::completion::system_completion(
        pool,
        virtues_registry::models::ModelSlot::Lite,
        "entity_article",
        system_prompt,
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
///   2. The href was offered, but under a name the label does not fit. This is
///      the observed failure: the right words over someone else's id. The test
///      is `wiki_editor::label_fits`, shared with the revision path.
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
                // Same comparison the editor's `check_links` makes on the
                // revision path, so the two halves of the wiki cannot drift
                // into disagreeing about which links are honest. It is
                // deliberately generous: "Soph" fits "Soph Auciello".
                allowed.entities.get(href).is_some_and(|name| {
                    crate::api::wiki_editor::label_fits(label, std::slice::from_ref(name))
                })
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

    #[test]
    fn records_fit_by_whole_lines_whatever_their_characters() {
        let lines: Vec<String> = (0..50)
            .map(|i| format!("- 2026-03-0{} message — “déjà vu” at the café ☕ {i}", i % 9))
            .collect();
        let kept = super::lines_that_fit(&lines, 300);
        let used: usize = kept.iter().map(|l| l.chars().count() + 1).sum();
        assert!(!kept.is_empty() && used <= 300, "{used}");
        assert_eq!(kept[..], lines[..kept.len()], "newest first, in order, unbroken");
        assert!(super::lines_that_fit(&lines, 0).is_empty());
    }
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

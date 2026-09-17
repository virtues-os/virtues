//! THE SUBJECT REGISTRY — one row per kind of thing the wiki writes about.
//!
//! Every subject kind has the same handful of facts attached to it: a word the
//! schema calls it, a prefix its ids carry, a table its row lives in, a route
//! the client displays it at, and a brief the editor writes it from. Those
//! facts were spelled out separately in ten places, each enumerating one facet,
//! and the cost is not abstract:
//!
//! * `check_links` validated links to five kinds and silently skipped the rest,
//!   which is the exact hole it was added to close;
//! * `get_subject_backlinks` carried a comment calling itself "the one place
//!   that mapping happens" while three other matches did the same mapping;
//! * `wiki_editor.rs` held two subject-to-table maps forty lines apart;
//! * adding the chapter rung meant touching seven sites, and three were missed.
//!
//! So: one table here, and every site derives. **Adding a rung is a row.**
//!
//! Two vocabularies meet in this file and must not be confused. `kind` is the
//! schema's word — `subject_type = 'organization'` — and `prefix` is what its
//! ids start with — `org_ab12`. They differ for exactly one kind, and that
//! difference is why a lookup keyed on the wrong one returns nothing rather
//! than failing.

use crate::api::wiki_editor::{CHAPTER_BRIEF, ENTITY_BRIEF, STORY_BRIEF, YEAR_BRIEF};

/// One kind of subject, and everything that varies by kind.
#[derive(Debug, Clone, Copy)]
pub struct Subject {
    /// The schema's word: `wiki_articles.subject_type`.
    pub kind: &'static str,
    /// What this kind's ids begin with, before the underscore.
    pub prefix: &'static str,
    /// The table holding the subject's own row, when it has one. The life has
    /// none — it is an article and nothing else.
    pub table: Option<&'static str>,
    /// The client route, `/{route}/{id}`. `None` means nothing can display
    /// this subject on its own page yet, which is a fact about the product
    /// rather than an oversight — and the reason a link to one must not be
    /// written into prose.
    pub route: Option<&'static str>,
    /// The editor's brief.
    ///
    /// **`None` is a refusal, not a gap**, and the two cases are the load
    /// bearing ones: the life page is the person's, in the first person, and
    /// the editor may never touch it; the day has its own released narrator
    /// and joins this door when its revision does.
    pub brief: Option<&'static str>,
    /// The id carries the title — `day_2026-03-03` needs no lookup to know
    /// what it is called.
    pub id_is_title: bool,
    /// Whether the person can write fields on the subject row itself, so that
    /// editing one is new evidence for the article.
    pub authored_fields: bool,
}

/// Every subject the wiki writes about.
///
/// Mirrors the `subject_type` CHECK that migration 0022 aligned across
/// `wiki_articles`, `wiki_notes` and `wiki_rules` after the four lists had
/// drifted apart. A kind here and not there fails at the database; a kind
/// there and not here fails at `create_article`. `the_registry_matches_the_schema`
/// is the test that keeps the two honest.
pub const SUBJECTS: &[Subject] = &[
    Subject {
        kind: "person",
        prefix: "person",
        table: Some("wiki_people"),
        route: Some("person"),
        brief: Some(ENTITY_BRIEF),
        id_is_title: false,
        authored_fields: false,
    },
    Subject {
        kind: "place",
        prefix: "place",
        table: Some("wiki_places"),
        route: Some("place"),
        brief: Some(ENTITY_BRIEF),
        id_is_title: false,
        authored_fields: false,
    },
    Subject {
        // The one kind whose prefix and word differ. The table is `wiki_orgs`
        // and ids read `org_ab12`, but every join on `subject_type` uses the
        // long form — a short-form lookup there returns zero rows rather than
        // an error, which is the worst way for a mismatch to behave.
        kind: "organization",
        prefix: "org",
        table: Some("wiki_orgs"),
        route: Some("org"),
        brief: Some(ENTITY_BRIEF),
        id_is_title: false,
        authored_fields: false,
    },
    Subject {
        kind: "day",
        prefix: "day",
        table: Some("wiki_days"),
        route: Some("day"),
        brief: None,
        id_is_title: true,
        authored_fields: false,
    },
    Subject {
        kind: "year",
        prefix: "year",
        table: Some("wiki_years"),
        route: Some("year"),
        brief: Some(YEAR_BRIEF),
        id_is_title: true,
        authored_fields: true,
    },
    Subject {
        // A chapter has an article and a page, and no route of its own: the
        // chapters room lists them and opens their pages. Recorded rather than
        // quietly fixed, because giving it a route is building something.
        kind: "chapter",
        prefix: "chapter",
        table: Some("wiki_chapters"),
        route: None,
        brief: Some(CHAPTER_BRIEF),
        id_is_title: false,
        authored_fields: true,
    },
    Subject {
        kind: "story",
        prefix: "story",
        table: Some("wiki_stories"),
        route: None,
        brief: Some(STORY_BRIEF),
        id_is_title: false,
        authored_fields: true,
    },
    Subject {
        // The life. No row, no route of its own — the owner's page is the
        // `/wiki/identity` room — and no brief, on purpose.
        kind: "narrative_identity",
        prefix: "nar",
        table: None,
        route: None,
        brief: None,
        id_is_title: false,
        authored_fields: false,
    },
];

/// Look a subject up by the schema's word.
pub fn by_kind(kind: &str) -> Option<&'static Subject> {
    SUBJECTS.iter().find(|s| s.kind == kind)
}

/// Look a subject up by an id's prefix.
///
/// Takes the whole id and reads the prefix off it, because every caller had
/// the id and was splitting it by hand — and one of them split on the LAST
/// underscore, which breaks on `nar_identity_001`.
pub fn by_id(id: &str) -> Option<&'static Subject> {
    let prefix = id.split_once('_')?.0;
    SUBJECTS.iter().find(|s| s.prefix == prefix)
}

/// Is this a subject type that may carry an article?
pub fn is_subject(kind: &str) -> bool {
    by_kind(kind).is_some()
}

/// Where a subject is displayed, or `None` when nothing can show it.
pub fn route_for(kind: &str, id: &str) -> Option<String> {
    by_kind(kind)?.route.map(|r| format!("/{r}/{id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The registry and the database must agree about what a subject is.
    ///
    /// A kind in the schema and not here cannot be created — `create_article`
    /// checks this list. A kind here and not in the schema passes that check
    /// and then fails at the CHECK constraint, which is a 500 rather than a
    /// refusal. Neither is catchable by reading one file, which is why the
    /// test reads the constraint itself.
    #[sqlx::test]
    async fn the_registry_matches_the_schema(pool: sqlx::PgPool) {
        let clause: String = sqlx::query_scalar(
            "SELECT pg_get_constraintdef(oid) FROM pg_constraint \
             WHERE conname = 'wiki_articles_subject_type_check'",
        )
        .fetch_one(&pool)
        .await
        .expect("the subject_type CHECK exists");

        for s in SUBJECTS {
            assert!(
                clause.contains(&format!("'{}'", s.kind)),
                "{} is in the registry but not in the schema's CHECK",
                s.kind
            );
        }
        // And nothing in the CHECK is missing here: count the quoted literals.
        let in_schema = clause.matches("::text").count();
        assert_eq!(
            in_schema,
            SUBJECTS.len(),
            "the schema allows {in_schema} subject types and the registry has {}",
            SUBJECTS.len()
        );
    }

    #[test]
    fn a_kind_and_a_prefix_are_different_vocabularies() {
        assert_eq!(by_kind("organization").unwrap().prefix, "org");
        assert_eq!(by_id("org_ab12").unwrap().kind, "organization");
        assert!(
            by_kind("org").is_none(),
            "the short form is an id prefix, never a subject_type — and a \
             lookup that accepted both would hide the mismatch it exists to \
             catch"
        );
    }

    #[test]
    fn an_id_with_underscores_after_the_prefix_still_resolves() {
        // `nar_identity_001`: splitting on the LAST underscore gives
        // `nar_identity`, which matches nothing.
        assert_eq!(by_id("nar_identity_001").unwrap().kind, "narrative_identity");
        assert_eq!(by_id("day_2026-03-03").unwrap().kind, "day");
        assert!(by_id("no-underscore").is_none());
    }

    #[test]
    fn a_subject_with_no_route_says_so_rather_than_guessing_one() {
        assert_eq!(route_for("person", "person_ab").as_deref(), Some("/person/person_ab"));
        assert_eq!(route_for("organization", "org_ab").as_deref(), Some("/org/org_ab"));
        assert_eq!(
            route_for("chapter", "chapter_ab"), None,
            "a chapter has no page of its own to link to, and inventing \
             /chapter/… would put a dead link in an article"
        );
    }

    #[test]
    fn the_two_briefless_kinds_are_the_documented_refusals() {
        let briefless: Vec<&str> = SUBJECTS
            .iter()
            .filter(|s| s.brief.is_none())
            .map(|s| s.kind)
            .collect();
        assert_eq!(
            briefless,
            vec!["day", "narrative_identity"],
            "a third kind without a brief is an article nothing will ever \
             write — which is how chapters sat with a seeded page and no \
             second sentence"
        );
    }

    /// The client's route table must say what this registry says.
    ///
    /// There is no codegen between Rust and TypeScript here, so the two halves
    /// are kept honest the way the plugin ACL is: one side reads the other's
    /// source and refuses to differ. Without it a rung added here is a rung
    /// the client cannot link — which is exactly how chapters ended up with an
    /// article, a room, and no href anything could write.
    #[test]
    fn the_client_route_table_matches_the_registry() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../apps/web/src/lib/wiki/links.ts");
        let src = std::fs::read_to_string(path).expect("links.ts is where the client map lives");
        let table = src
            .split_once("const SUBJECT_ROUTES")
            .and_then(|(_, rest)| rest.split_once('{'))
            .and_then(|(_, rest)| rest.split_once("};"))
            .map(|(body, _)| body.to_string())
            .expect("SUBJECT_ROUTES is an object literal");

        // Comments inside the literal carry prose about prefixes; strip them
        // or `organization` in a sentence reads as an entry.
        let body: String = table
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        for s in SUBJECTS {
            let expected = match s.route {
                Some(r) => format!("{}: '{}'", s.prefix, r),
                None => format!("{}: null", s.prefix),
            };
            assert!(
                body.contains(&expected),
                "links.ts is missing `{expected}` — the registry has {} at {:?}",
                s.kind,
                s.route
            );
        }

        let entries = body.matches(':').count();
        assert_eq!(
            entries,
            SUBJECTS.len(),
            "links.ts lists {entries} subject prefixes and the registry has {}",
            SUBJECTS.len()
        );
    }
}

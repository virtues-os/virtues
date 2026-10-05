//! What the wiki already knows about a search, found by match rather than by
//! similarity.
//!
//! The wiki is the index, but a similarity search almost never returned it: on
//! one box, one wiki chunk in 200 searches. Articles are long and few, and a
//! hundred thousand short messages outscore them. So the parts of a search
//! that NAME something are answered deterministically, ahead of the search:
//!
//! - a subject named in the queries, or passed as `entities` → its article,
//!   or a short card when it has none;
//! - a date range of a week or less → the articles for those days.
//!
//! Everything else is left to the search.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use sqlx::PgPool;

use crate::api::subjects;

/// Article text across the whole section, in characters. Results for a
/// question that names a person ran to a median of 68 KB, so this is cheap,
/// and an article often answers the question outright.
const BUDGET: usize = 4000;
/// One subject's article; the rest of the budget goes to days.
const SUBJECT_CAP: usize = 2500;
/// A day's article. Several days share the budget, so each gets its opening.
const DAY_CAP: usize = 600;
/// Below this an article is not worth the space; its ref still is.
const MIN_TEXT: usize = 200;
const MAX_SUBJECTS: usize = 3;
/// The widest range answered day by day. Wider is a question for the search.
const MAX_DAYS: i64 = 7;

/// The section, and the pages in it, so the search results can drop them.
pub struct WikiFirst {
    pub entries: Vec<serde_json::Value>,
    pub page_ids: HashSet<String>,
}

/// A subject and every name it answers to.
struct Named {
    kind: String,
    id: String,
    name: String,
    relation: Option<String>,
    /// Name, nickname and aliases, each as lowercase words.
    phrases: Vec<Vec<String>>,
    /// A person's first name, when their name has more than one word. Weaker
    /// than a full match, because first names repeat and some are also places.
    first: Option<String>,
}

pub async fn wiki_first(
    pool: &PgPool,
    queries: &[String],
    entities: &[String],
    range: Option<(NaiveDate, NaiveDate)>,
) -> anyhow::Result<WikiFirst> {
    let mut out = WikiFirst { entries: Vec::new(), page_ids: HashSet::new() };
    let mut budget = BUDGET;

    let named = load_names(pool).await?;
    let picked = pick_subjects(&named, queries, entities);
    if !picked.is_empty() {
        let ids: Vec<String> = picked.iter().map(|n| n.id.clone()).collect();
        let articles = subject_articles(pool, &ids).await?;
        let seen = seen_stats(pool, &ids).await?;
        for n in picked {
            let route = subjects::route_for(&n.kind, &n.id);
            let mut entry = serde_json::json!({
                "kind": n.kind,
                "id": n.id,
                "name": n.name,
                "ref": route,
            });
            if let Some(rel) = &n.relation {
                entry["relationship"] = rel.clone().into();
            }
            if let Some((count, first, last)) = seen.get(&n.id) {
                entry["records"] = (*count).into();
                entry["first_seen"] = first.map(|d| d.date_naive().to_string()).into();
                entry["last_seen"] = last.map(|d| d.date_naive().to_string()).into();
            }
            if let Some((page_id, text)) = articles.get(&n.id) {
                out.page_ids.insert(page_id.clone());
                if let Some(t) = clip(text, SUBJECT_CAP.min(budget)) {
                    budget -= t.chars().count();
                    entry["article"] = t.into();
                }
            }
            out.entries.push(entry);
        }
    }

    if let Some((from, until)) = range {
        let days = day_articles(pool, from, until).await?;
        let n = days.len().max(1);
        for (date, page_id, text) in days {
            out.page_ids.insert(page_id);
            let mut entry = serde_json::json!({
                "kind": "day",
                "date": date.to_string(),
                "ref": subjects::route_for("day", &format!("day_{date}")),
            });
            if let Some(t) = clip(&text, DAY_CAP.min(budget / n)) {
                budget -= t.chars().count();
                entry["article"] = t.into();
            }
            out.entries.push(entry);
        }
    }

    Ok(out)
}

/// The day range a search's date filters describe, when it is narrow enough to
/// answer day by day: `[from, until)`, at most [`MAX_DAYS`] long. A range open
/// at the end runs to `today`.
pub fn day_range(
    after: Option<chrono::DateTime<chrono::Utc>>,
    before: Option<chrono::DateTime<chrono::Utc>>,
    today: NaiveDate,
) -> Option<(NaiveDate, NaiveDate)> {
    let from = after?.date_naive();
    let until = match before {
        Some(b) => b.date_naive(),
        None => today.succ_opt()?,
    };
    let span = (until - from).num_days();
    (1..=MAX_DAYS).contains(&span).then_some((from, until))
}

/// Lowercase words, split on anything that is not a letter or digit, so an
/// emoji after a name or a hyphen inside one does not stop it matching.
fn words(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn contains_phrase(hay: &[String], phrase: &[String]) -> bool {
    !phrase.is_empty() && hay.windows(phrase.len()).any(|w| w == phrase)
}

/// A name too short to trust: "Al", "Mo", "ER" match ordinary words.
fn too_short(phrase: &[String]) -> bool {
    phrase.iter().map(|w| w.chars().count()).sum::<usize>() <= 3
}

/// The subjects a search names, strongest first: ids the agent passed, then
/// full names, ties going to the subject with more records (the order
/// `load_names` returns).
///
/// A first name alone counts only as a last resort: when nothing matched in
/// full, and when exactly one person has it. Replayed over one box's real
/// searches, first names were the noise — one matched six different people,
/// one was also the city the question was about, and one was the owner's own.
fn pick_subjects<'a>(named: &'a [Named], queries: &[String], entities: &[String]) -> Vec<&'a Named> {
    let hay: Vec<Vec<String>> = queries.iter().map(|q| words(q)).collect();
    let mut scored: Vec<(u8, usize, &Named)> = Vec::new();
    for (order, n) in named.iter().enumerate() {
        let strength = if entities.iter().any(|e| e == &n.id) {
            2
        } else if n
            .phrases
            .iter()
            .any(|p| !too_short(p) && hay.iter().any(|h| contains_phrase(h, p)))
        {
            1
        } else {
            continue;
        };
        scored.push((strength, order, n));
    }
    if scored.is_empty() {
        let mut firsts: HashMap<&str, usize> = HashMap::new();
        for f in named.iter().filter_map(|n| n.first.as_deref()) {
            *firsts.entry(f).or_default() += 1;
        }
        for (order, n) in named.iter().enumerate() {
            let Some(f) = n.first.as_deref() else { continue };
            if f.chars().count() > 3
                && firsts[f] == 1
                && hay.iter().any(|h| h.iter().any(|w| w == f))
            {
                scored.push((0, order, n));
            }
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().take(MAX_SUBJECTS).map(|(_, _, n)| n).collect()
}

/// Every person, place and organization with the names it answers to, most
/// referenced first. Under a thousand rows on a real box, read per search.
async fn load_names(pool: &PgPool) -> anyhow::Result<Vec<Named>> {
    #[allow(clippy::type_complexity)]
    let rows: Vec<(String, String, String, Option<String>, Option<serde_json::Value>, Option<String>)> =
        sqlx::query_as(
            "WITH s AS (
               SELECT 'person' AS kind, id, name, nickname, aliases,
                      relationship_category AS relation FROM wiki_people
               UNION ALL
               SELECT 'place', id, name, NULL, aliases, category FROM wiki_places
               UNION ALL
               SELECT 'organization', id, name, NULL, aliases,
                      COALESCE(relationship_type, organization_type) FROM wiki_orgs
             )
             SELECT s.kind, s.id, s.name, s.nickname, s.aliases, s.relation
             FROM s
             LEFT JOIN (SELECT entity_id, count(*) AS n FROM wiki_refs GROUP BY entity_id) r
               ON r.entity_id = s.id
             ORDER BY COALESCE(r.n, 0) DESC, s.id",
        )
        .fetch_all(pool)
        .await?;

    Ok(rows
        .into_iter()
        .map(|(kind, id, name, nickname, aliases, relation)| {
            let mut phrases = vec![words(&name)];
            phrases.extend(nickname.as_deref().map(words));
            if let Some(serde_json::Value::Array(items)) = aliases {
                phrases.extend(items.iter().filter_map(|v| v.as_str()).map(words));
            }
            phrases.retain(|p| !p.is_empty());
            let name_words = words(&name);
            let first = (kind == "person" && name_words.len() > 1).then(|| name_words[0].clone());
            Named { kind, id, name, relation, phrases, first }
        })
        .collect())
}

/// Each subject's article: `subject_id → (page_id, text)`.
async fn subject_articles(
    pool: &PgPool,
    ids: &[String],
) -> anyhow::Result<HashMap<String, (String, String)>> {
    let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT a.subject_id, p.id, p.content
         FROM wiki_articles a JOIN app_pages p ON p.id = a.page_id
         WHERE a.subject_id = ANY($1) AND p.deleted_at IS NULL",
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(sid, pid, text)| Some((sid, (pid, text.filter(|t| !t.trim().is_empty())?))))
        .collect())
}

type Seen = (i64, Option<chrono::DateTime<chrono::Utc>>, Option<chrono::DateTime<chrono::Utc>>);

/// How many records reference each subject, and over what span.
async fn seen_stats(pool: &PgPool, ids: &[String]) -> anyhow::Result<HashMap<String, Seen>> {
    #[allow(clippy::type_complexity)]
    let rows: Vec<(String, i64, Option<chrono::DateTime<chrono::Utc>>, Option<chrono::DateTime<chrono::Utc>>)> =
        sqlx::query_as(
            "SELECT entity_id, count(*), min(occurred_at), max(occurred_at)
             FROM wiki_refs WHERE entity_id = ANY($1) GROUP BY entity_id",
        )
        .bind(ids)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(id, n, a, b)| (id, (n, a, b))).collect())
}

/// The days in `[from, until)` that have an article, oldest first.
async fn day_articles(
    pool: &PgPool,
    from: NaiveDate,
    until: NaiveDate,
) -> anyhow::Result<Vec<(NaiveDate, String, String)>> {
    let rows: Vec<(NaiveDate, String, String)> = sqlx::query_as(
        "SELECT d.date, p.id, p.content
         FROM wiki_days d
         JOIN wiki_articles a ON a.subject_type = 'day' AND a.subject_id = d.id
         JOIN app_pages p ON p.id = a.page_id
         WHERE d.date >= $1 AND d.date < $2
           AND p.deleted_at IS NULL AND btrim(COALESCE(p.content, '')) <> ''
         ORDER BY d.date",
    )
    .bind(from)
    .bind(until)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// At most `cap` characters, cut at a word boundary and marked as cut. `None`
/// when there is too little room for the text to be worth sending.
fn clip(text: &str, cap: usize) -> Option<String> {
    let text = text.trim();
    if text.chars().count() <= cap {
        return Some(text.to_string());
    }
    if cap < MIN_TEXT {
        return None;
    }
    let cut: String = text.chars().take(cap - 1).collect();
    let cut = cut.rsplit_once(char::is_whitespace).map_or(cut.as_str(), |(head, _)| head);
    Some(format!("{}…", cut.trim_end()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(id: &str, name: &str, aliases: &[&str]) -> Named {
        let mut phrases = vec![words(name)];
        phrases.extend(aliases.iter().map(|a| words(a)));
        let w = words(name);
        Named {
            kind: "person".into(),
            id: id.into(),
            name: name.into(),
            relation: None,
            phrases,
            first: (w.len() > 1).then(|| w[0].clone()),
        }
    }

    fn ids(picked: Vec<&Named>) -> Vec<&str> {
        picked.into_iter().map(|n| n.id.as_str()).collect()
    }

    #[test]
    fn a_name_matches_whole_words_and_survives_an_emoji() {
        let named = [person("person_d", "David Okafor 🌷", &["Dave"])];
        let q = |s: &str| vec![s.to_string()];
        assert_eq!(ids(pick_subjects(&named, &q("dinner with David Okafor"), &[])), ["person_d"]);
        assert_eq!(ids(pick_subjects(&named, &q("what did dave say"), &[])), ["person_d"]);
        assert_eq!(ids(pick_subjects(&named, &q("David's birthday"), &[])), ["person_d"]);
        assert!(pick_subjects(&named, &q("davidson street"), &[]).is_empty());
    }

    #[test]
    fn a_first_name_counts_only_alone_and_unshared() {
        let named = [
            person("person_dk", "David Kim", &[]),
            person("person_do", "David Okafor", &[]),
            person("person_nm", "Nick Moreau", &[]),
        ];
        let q = |s: &str| vec![s.to_string()];
        assert_eq!(ids(pick_subjects(&named, &q("David Okafor and the move"), &[])), ["person_do"]);
        assert!(pick_subjects(&named, &q("what did david say"), &[]).is_empty(), "two Davids");
        assert_eq!(ids(pick_subjects(&named, &q("what did nick say"), &[])), ["person_nm"]);
        assert_eq!(
            ids(pick_subjects(&named, &q("nick and David Okafor"), &[])),
            ["person_do"],
            "a full match elsewhere silences first names"
        );
    }

    #[test]
    fn short_names_and_passed_ids() {
        let named = [person("person_al", "Al", &[]), person("person_n", "Nick", &[])];
        assert!(pick_subjects(&named, &["al fresco lunch".into()], &[]).is_empty());
        let picked = pick_subjects(&named, &["lunch".into()], &["person_n".into()]);
        assert_eq!(ids(picked), ["person_n"]);
    }

    #[test]
    fn only_a_narrow_range_is_answered_day_by_day() {
        let at = |s: &str| Some(format!("{s}T00:00:00Z").parse().unwrap());
        let today = NaiveDate::from_ymd_opt(2026, 9, 29).unwrap();
        let d = |m, d| NaiveDate::from_ymd_opt(2026, m, d).unwrap();
        assert_eq!(day_range(at("2026-09-22"), at("2026-09-23"), today), Some((d(9, 22), d(9, 23))));
        assert_eq!(day_range(at("2026-09-27"), None, today), Some((d(9, 27), d(9, 30))));
        assert_eq!(day_range(at("2026-08-01"), at("2026-09-01"), today), None);
        assert_eq!(day_range(None, at("2026-09-01"), today), None);
    }

    #[test]
    fn clipping_cuts_at_a_word_and_refuses_a_stub() {
        let long = "word ".repeat(200);
        let c = clip(&long, 300).unwrap();
        assert!(c.chars().count() <= 300 && c.ends_with("word…"));
        assert_eq!(clip("short", 50).as_deref(), Some("short"));
        assert!(clip(&long, 100).is_none());
    }

    #[sqlx::test]
    async fn a_named_person_and_a_narrow_range_lead_with_their_articles(pool: PgPool) {
        for sql in [
            "INSERT INTO wiki_people (id, name, relationship_category)
               VALUES ('person_do', 'David Okafor', 'friend'), ('person_n', 'Nick', NULL)",
            "INSERT INTO wiki_days (id, date) VALUES ('day_2026-03-03', '2026-03-03'),
                                                     ('day_2026-03-04', '2026-03-04')",
            "INSERT INTO app_pages (id, title, kind, content) VALUES
               ('page-do', 'David Okafor', 'article', 'David writes most mornings.'),
               ('page-d3', 'Tuesday, March 3', 'article', 'A long lunch with David.')",
            "INSERT INTO wiki_articles (id, subject_type, subject_id, page_id) VALUES
               ('art-do', 'person', 'person_do', 'page-do'),
               ('art-d3', 'day', 'day_2026-03-03', 'page-d3')",
        ] {
            sqlx::query(sql).execute(&pool).await.unwrap();
        }
        let range = Some((
            NaiveDate::from_ymd_opt(2026, 3, 3).unwrap(),
            NaiveDate::from_ymd_opt(2026, 3, 5).unwrap(),
        ));
        let got = wiki_first(&pool, &["lunch with david okafor".into()], &["person_n".into()], range)
            .await
            .unwrap();

        let kinds: Vec<(&str, &str)> = got
            .entries
            .iter()
            .map(|e| (e["kind"].as_str().unwrap(), e["ref"].as_str().unwrap()))
            .collect();
        assert_eq!(
            kinds,
            [("person", "/person/person_n"), ("person", "/person/person_do"), ("day", "/day/day_2026-03-03")],
            "passed id, then the named person, then the one day with an article"
        );
        assert_eq!(got.entries[1]["article"], "David writes most mornings.");
        assert_eq!(got.entries[1]["relationship"], "friend");
        assert!(got.entries[0].get("article").is_none(), "no article: a card");
        assert_eq!(got.page_ids, HashSet::from(["page-do".to_string(), "page-d3".to_string()]));
    }
}

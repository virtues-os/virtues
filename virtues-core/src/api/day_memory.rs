//! What the record already says about a day's people and the days before it,
//! assembled from the day pages, never stored.
//!
//! A day page's first paragraph is its Abstract: a short, cited account of
//! the day. Joined by the people a page links and by recency, the Abstracts
//! are a history of the life the system already keeps. A writer reading a day
//! gets the earlier days that link the people in it, however far back, and
//! the last two weeks; chat's circumstances gets the recent days. Nothing here
//! is generated or written: it is a query over pages, so it is always as true
//! as the pages are, and a better Abstract is a better memory.

use chrono::NaiveDate;
use sqlx::PgPool;

/// Every read here is one query; its only failure is the database's.
type Result<T> = std::result::Result<T, sqlx::Error>;

/// A page's Abstract: its first paragraph, as written.
pub fn abstract_of(page: &str) -> &str {
    page.split("\n\n").next().unwrap_or("").trim()
}

/// An Abstract as a prompt reads it: footnote markers, veil marks and link
/// targets removed, so a name is a name.
pub fn plain(md: &str) -> String {
    let footnotes = regex::Regex::new(r"\[\^[a-z]{2}-\d+\]").expect("static regex");
    let links = regex::Regex::new(r"\[([^\]]+)\]\(/[a-z]+/[^)]+\)").expect("static regex");
    let s = footnotes.replace_all(md, "");
    let s = links.replace_all(&s, "$1");
    s.replace(['⟦', '⟧'], "").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The Abstracts of the `days` days before `before`, newest first.
pub async fn recent(pool: &PgPool, before: NaiveDate, days: i64) -> Result<Vec<(NaiveDate, String)>> {
    let rows: Vec<(NaiveDate, String)> = sqlx::query_as(
        "SELECT date, prose FROM wiki_day_prose \
         WHERE prose IS NOT NULL AND date < $1 AND date >= $1 - $2::int \
         ORDER BY date DESC",
    )
    .bind(before)
    .bind(days as i32)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(d, p)| (d, plain(abstract_of(&p)))).collect())
}

/// One person's history in the pages before a day.
#[derive(Debug, Clone)]
pub struct PersonHistory {
    pub id: String,
    pub name: String,
    pub relationship: Option<String>,
    pub died_on: Option<NaiveDate>,
    /// Day pages before `before` that link them.
    pub pages: i64,
    pub first: Option<NaiveDate>,
}

/// An earlier day, and which of the asked-about people its page links.
#[derive(Debug, Clone)]
pub struct LinkedDay {
    pub date: NaiveDate,
    pub abstract_text: String,
    pub people: Vec<String>,
}

/// For each person, how much of the record they are in, and the latest
/// `per_person` earlier days whose pages link them, each day listed once.
pub async fn linked(
    pool: &PgPool,
    person_ids: &[String],
    before: NaiveDate,
    per_person: i64,
) -> Result<(Vec<PersonHistory>, Vec<LinkedDay>)> {
    if person_ids.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    // A page links a person as `](/person/<id>)`; the closing parenthesis
    // keeps one id from matching another that it prefixes.
    let people: Vec<(String, String, Option<String>, Option<NaiveDate>, i64, Option<NaiveDate>)> = sqlx::query_as(
        "SELECT p.id, p.name, p.relationship_category, p.died_on, \
                count(d.date), min(d.date) \
         FROM unnest($1::text[]) WITH ORDINALITY AS ask(id, ord) \
         JOIN wiki_people p ON p.id = ask.id \
         LEFT JOIN wiki_day_prose d ON d.prose IS NOT NULL AND d.date < $2 \
              AND strpos(d.prose, '/person/' || p.id || ')') > 0 \
         GROUP BY p.id, p.name, p.relationship_category, p.died_on, ask.ord \
         ORDER BY ask.ord",
    )
    .bind(person_ids)
    .bind(before)
    .fetch_all(pool)
    .await?;
    let days: Vec<(String, NaiveDate, String)> = sqlx::query_as(
        "SELECT ask.id, d.date, d.prose \
         FROM unnest($1::text[]) AS ask(id) \
         CROSS JOIN LATERAL ( \
             SELECT date, prose FROM wiki_day_prose \
             WHERE prose IS NOT NULL AND date < $2 \
               AND strpos(prose, '/person/' || ask.id || ')') > 0 \
             ORDER BY date DESC LIMIT $3) d \
         ORDER BY d.date DESC, ask.id",
    )
    .bind(person_ids)
    .bind(before)
    .bind(per_person)
    .fetch_all(pool)
    .await?;

    // absent-ok: a lookup in the rows above, not a query; an id with no person row has no name to show.
    let name_of = |id: &str| people.iter().find(|p| p.0 == id).map(|p| p.1.clone()).unwrap_or_default();
    let mut linked: Vec<LinkedDay> = Vec::new();
    for (id, date, prose) in days {
        match linked.iter_mut().find(|l| l.date == date) {
            Some(l) => l.people.push(name_of(&id)),
            None => linked.push(LinkedDay { date, abstract_text: plain(abstract_of(&prose)), people: vec![name_of(&id)] }),
        }
    }
    let history = people
        .into_iter()
        .map(|(id, name, relationship, died_on, pages, first)| PersonHistory { id, name, relationship, died_on, pages, first })
        .collect();
    Ok((history, linked))
}

/// The memory a day writer reads: the people the day was most with and their
/// earlier days, then the last two weeks not already listed. `start`/`end`
/// bound the day; `skip` holds dates the writer already has in full.
pub async fn for_writer(
    pool: &PgPool,
    person_ids: &[String],
    date: NaiveDate,
    start: &str,
    end: &str,
    skip: &[NaiveDate],
) -> Result<String> {
    /// The people whose history is pulled. A day's record names everyone in
    /// every group thread it touched; past the first few, their earlier days
    /// are other people's news.
    const PEOPLE: i64 = 6;
    /// Earlier days per person: enough to carry a story across a gap, few
    /// enough that the people seen every day do not crowd out the rest.
    const PER_PERSON: i64 = 3;
    const RECENT_DAYS: i64 = 14;
    const ABSTRACT_CHARS: usize = 420;

    // Most records in the day first: who the day was most with.
    let ranked: Vec<String> = sqlx::query_scalar(
        "SELECT entity_id FROM wiki_refs \
         WHERE entity_type = 'person' AND entity_id = ANY($1) \
           AND occurred_at >= $2::timestamptz AND occurred_at < $3::timestamptz \
         GROUP BY entity_id ORDER BY count(*) DESC, entity_id LIMIT $4",
    )
    .bind(person_ids)
    .bind(start)
    .bind(end)
    .bind(PEOPLE)
    .fetch_all(pool)
    .await?;
    let (people, linked) = linked(pool, &ranked, date, PER_PERSON).await?;
    let recent = recent(pool, date, RECENT_DAYS).await?;
    let cut = |s: &str| -> String {
        if s.chars().count() <= ABSTRACT_CHARS {
            s.to_string()
        } else {
            format!("{}…", s.chars().take(ABSTRACT_CHARS - 1).collect::<String>())
        }
    };

    let mut out = String::new();
    if !people.is_empty() {
        out.push_str("The people this day was most with, in the pages before it:\n");
        for p in &people {
            let mut facts = Vec::new();
            if let Some(r) = p.relationship.as_deref().filter(|r| !r.trim().is_empty()) {
                facts.push(r.to_string());
            }
            if let Some(d) = p.died_on {
                facts.push(format!("died {d}"));
            }
            let on = match (p.pages, p.first) {
                (0, _) | (_, None) => "on no earlier day page".to_string(),
                (n, Some(first)) => format!("on {n} earlier day pages since {first}"),
            };
            let facts = if facts.is_empty() { String::new() } else { format!(" ({})", facts.join(", ")) };
            out.push_str(&format!("- {}{facts}: {on}\n", p.name));
        }
    }
    let mut listed: Vec<NaiveDate> = skip.to_vec();
    let earlier: Vec<&LinkedDay> = linked.iter().filter(|l| !skip.contains(&l.date)).collect();
    if !earlier.is_empty() {
        out.push_str("\nEarlier days that link them, newest first:\n");
        for l in earlier {
            out.push_str(&format!("- {} ({}): {}\n", l.date, l.people.join(", "), cut(&l.abstract_text)));
            listed.push(l.date);
        }
    }
    let rest: Vec<&(NaiveDate, String)> = recent.iter().filter(|(d, a)| !listed.contains(d) && !a.is_empty()).collect();
    if !rest.is_empty() {
        out.push_str("\nThe rest of the last two weeks:\n");
        for (d, a) in rest {
            out.push_str(&format!("- {d}: {}\n", cut(a)));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_abstract_is_the_first_paragraph() {
        let page = "You drove [⟦Nick⟧](/person/person_a1) to ⟦the clinic⟧.[^ev-1]\n\n## Later\n\nMore.";
        assert_eq!(abstract_of(page), "You drove [⟦Nick⟧](/person/person_a1) to ⟦the clinic⟧.[^ev-1]");
        assert_eq!(plain(abstract_of(page)), "You drove Nick to the clinic.");
    }

    #[test]
    fn an_empty_page_has_an_empty_abstract() {
        assert_eq!(abstract_of(""), "");
        assert_eq!(plain(""), "");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_person_is_found_on_every_page_that_links_them_and_no_other(pool: PgPool) {
        let day = |date: &str, body: &str| {
            let (date, body) = (date.to_string(), body.to_string());
            let pool = pool.clone();
            async move {
                let d: NaiveDate = date.parse().unwrap();
                let id = format!("day_{date}");
                let page = format!("page_{date}");
                sqlx::query("INSERT INTO wiki_days (id, date) VALUES ($1, $2)").bind(&id).bind(d).execute(&pool).await.unwrap();
                sqlx::query("INSERT INTO app_pages (id, title, content) VALUES ($1, $2, $3)")
                    .bind(&page).bind(&date).bind(&body).execute(&pool).await.unwrap();
                sqlx::query("INSERT INTO wiki_articles (id, subject_type, subject_id, page_id) VALUES ($1, 'day', $2, $3)")
                    .bind(format!("art_{date}")).bind(&id).bind(&page).execute(&pool).await.unwrap();
            }
        };
        sqlx::query("INSERT INTO wiki_people (id, name) VALUES ('person_a1', 'Nick'), ('person_a12', 'David Okafor')")
            .execute(&pool).await.unwrap();
        day("2026-01-02", "You met [Nick](/person/person_a1) for lunch.\n\n## Lunch\n\nThe soup.").await;
        day("2026-01-05", "A quiet day with [David Okafor](/person/person_a12).").await;
        day("2026-01-07", "[Nick](/person/person_a1) called about the move.").await;
        day("2026-01-09", "Today: [Nick](/person/person_a1) again.").await;

        let before: NaiveDate = "2026-01-09".parse().unwrap();
        let (people, linked) = linked(&pool, &["person_a1".to_string()], before, 5).await.unwrap();
        assert_eq!(people.len(), 1);
        assert_eq!(people[0].pages, 2, "the 01-05 page links person_a12, which person_a1 prefixes");
        assert_eq!(people[0].first, Some("2026-01-02".parse().unwrap()));
        let dates: Vec<String> = linked.iter().map(|l| l.date.to_string()).collect();
        assert_eq!(dates, vec!["2026-01-07", "2026-01-02"], "newest first, and never the day itself");
        assert_eq!(linked[1].abstract_text, "You met Nick for lunch.");
    }
}

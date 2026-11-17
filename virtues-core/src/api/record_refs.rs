//! What a record route (`/record/<ontology>/<id>`) points at: its table, its
//! title, and when it happened. The ontology is the `data_*` table's name
//! without the prefix. Tables differ in which columns they have, so each one
//! is asked (`information_schema`) before it is read.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::error::Result;

/// The columns a record's title is read from, first match wins.
const TITLE_COLUMNS: [&str; 4] = ["title", "name", "subject", "summary"];
/// The columns a record's time is read from, first match wins.
const TIME_COLUMNS: [&str; 2] = ["started_at", "occurred_at"];

/// `(table, id)` from `/record/<ontology>/<id>` or from the `<ontology>/<id>`
/// part alone, when the ontology is a plain `[a-z0-9_]` name.
pub fn record_table(route: &str) -> Option<(String, String)> {
    let rest = route.strip_prefix("/record/").unwrap_or(route);
    let (ontology, id) = rest.split_once('/')?;
    let id = id.split(['?', '#']).next().unwrap_or(id);
    let plain = !ontology.is_empty()
        && ontology.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    (plain && !id.is_empty()).then(|| (format!("data_{ontology}"), id.to_string()))
}

/// The columns `table` has, empty when there is no such table.
pub async fn columns_of(pool: &PgPool, table: &str) -> Result<HashSet<String>> {
    let names: Vec<String> = sqlx::query_scalar(
        "SELECT column_name::text FROM information_schema.columns
         WHERE table_schema = 'public' AND table_name = $1",
    )
    .bind(table)
    .fetch_all(pool)
    .await?;
    Ok(names.into_iter().collect())
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecordFacts {
    pub title: Option<String>,
    pub at: Option<DateTime<Utc>>,
}

/// Title and time for each `(table, id)`, keyed the same way. A table with
/// neither kind of column, or no such table, gives nothing.
pub async fn record_facts(
    pool: &PgPool,
    records: &[(String, String)],
) -> Result<HashMap<(String, String), RecordFacts>> {
    let mut by_table: HashMap<&str, Vec<String>> = HashMap::new();
    for (table, id) in records {
        by_table.entry(table.as_str()).or_default().push(id.clone());
    }
    let mut out = HashMap::new();
    for (table, ids) in by_table {
        let columns = columns_of(pool, table).await?;
        let title = TITLE_COLUMNS.iter().find(|c| columns.contains(**c));
        let time = TIME_COLUMNS.iter().find(|c| columns.contains(**c));
        if title.is_none() && time.is_none() {
            continue;
        }
        let title_sql = title.map_or("NULL::text".to_string(), |c| format!("{c}::text"));
        let time_sql = time.map_or("NULL::timestamptz".to_string(), |c| (*c).to_string());
        // `table` came from `record_table` (a checked name) and was found in
        // information_schema above; the columns are from the lists above.
        let rows: Vec<(String, Option<String>, Option<DateTime<Utc>>)> = sqlx::query_as(&format!(
            "SELECT id, {title_sql}, {time_sql} FROM {table} WHERE id = ANY($1)"
        ))
        .bind(&ids)
        .fetch_all(pool)
        .await?;
        for (id, title, at) in rows {
            out.insert((table.to_string(), id), RecordFacts { title, at });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_route_names_its_table() {
        assert_eq!(
            record_table("/record/calendar_event/ev_1"),
            Some(("data_calendar_event".into(), "ev_1".into()))
        );
        assert_eq!(record_table("calendar_event/ev_1?x=1"), Some(("data_calendar_event".into(), "ev_1".into())));
        assert_eq!(record_table("/record/Calendar;drop/ev_1"), None);
        assert_eq!(record_table("/record/calendar_event"), None);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_record_has_a_title_and_a_time(pool: PgPool) {
        sqlx::query(
            "INSERT INTO data_calendar_event (id, title, started_at, ended_at, source_stream_id, source_table, source_provider)
             VALUES ('ev_1', 'Second coat', '2030-01-12T10:00:00Z', '2030-01-12T13:00:00Z', 's1', 'google_calendar', 'google')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let key = ("data_calendar_event".to_string(), "ev_1".to_string());
        let facts = record_facts(&pool, &[key.clone(), ("data_nothing_here".into(), "x".into())]).await.unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[&key].title.as_deref(), Some("Second coat"));
        assert_eq!(facts[&key].at, Some("2030-01-12T10:00:00Z".parse().unwrap()));
    }
}

//! The `<coverage>` block — what the record holds, per source table, as a
//! date range.
//!
//! `<circumstances>` is deliberately silent about absences: no sleep row for
//! last night is not a fact about the night. Coverage is a different thing.
//! Which tables hold anything, and from when to when, is a fact about the
//! RECORD, and it is the fact the model needs to answer the questions where
//! trust is decided — "when did I last see her", "how has my sleep been since
//! June" — because without it an empty result has two readings and the model
//! must pick one blind: the record was flowing and holds nothing for that
//! window (evidence of absence), or the source has no coverage there
//! (absence of evidence). A source connected in June has nothing before June,
//! and a reply that reports "no meetings in May" from it is a lie the model
//! had no way to avoid.
//!
//! One line per table the SQL catalog advertises, from the registry's own
//! timestamp column (the same `timestamp_column` the day lanes and search
//! read), floored to the day so the bytes change at most once a day per
//! table. A table with no rows renders nothing: the absence of a line IS the
//! coverage statement for it, and the block says so in its preamble.
//!
//! Every query is `MIN`/`MAX` over one indexed time column — an index-endpoint
//! read, not a scan — and a table whose read fails is logged and omitted,
//! never guessed.

use chrono::NaiveDate;
use futures::future::join_all;
use sqlx::PgPool;
use std::collections::BTreeMap;

/// A gap this long between the latest observation and today is worth a
/// word: below it the difference is ingest lag, above it the source may have
/// stopped, and the model should not report a quiet week as a quiet life.
const STALE_AFTER_DAYS: i64 = 2;

/// Build the block. `today` is the person's local date, derived from the
/// same quantized instant `<circumstances>` uses, so the two agree on what
/// "today" is.
pub async fn build_coverage(pool: &PgPool, today: NaiveDate) -> Option<String> {
    // One row per table: the registry lists several ontologies over one
    // table (messages sent and received are two lanes on one table), and
    // they share a timestamp column, so the first wins.
    let mut tables: BTreeMap<&'static str, &'static str> = BTreeMap::new();
    for ont in virtues_registry::ontologies::registered_ontologies() {
        tables.entry(ont.table_name).or_insert(ont.timestamp_column);
    }
    // The catalog is the fence: a table the model cannot query has no
    // business being described to it here either.
    let catalog = virtues_registry::sql_catalog::get_table_metadata();
    tables.retain(|t, _| catalog.contains_key(t));

    let reads = tables.iter().map(|(table, column)| async move {
        // Identifiers come from two static registries, never from input.
        let sql = format!("SELECT MIN({column})::date, MAX({column})::date FROM {table}");
        let row: Result<(Option<NaiveDate>, Option<NaiveDate>), sqlx::Error> =
            sqlx::query_as(&sql).fetch_one(pool).await;
        (*table, row)
    });

    let mut lines: Vec<String> = Vec::new();
    for (table, row) in join_all(reads).await {
        match row {
            Ok((Some(first), Some(last))) => {
                let gap = (today - last).num_days();
                let tail = if gap >= STALE_AFTER_DAYS {
                    format!(" — nothing for {gap} days")
                } else {
                    String::new()
                };
                lines.push(format!("  {table}: {first} → {last}{tail}"));
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(table, error = %e, "[chat] coverage line omitted"),
        }
    }

    if lines.is_empty() {
        return None;
    }
    Some(format!(
        "\n\n<coverage>\nWhat the record holds, per table: earliest → latest observation, to the day. A table not listed holds nothing yet. A question about a time outside a table's range cannot be answered from that table — say what is not covered rather than reporting an absence. An empty result INSIDE the range, from a table still flowing, is evidence of absence and can be said plainly.\n{}\n</coverage>",
        lines.join("\n")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every registry timestamp column named for a catalog table must exist
    /// on the migrated schema, or the line is silently omitted forever. And
    /// an empty box renders no block at all — nothing to say is not a
    /// section.
    #[sqlx::test]
    async fn every_coverage_read_is_valid_and_an_empty_record_renders_nothing(pool: PgPool) {
        let today = NaiveDate::from_ymd_opt(2026, 9, 21).unwrap();
        assert!(build_coverage(&pool, today).await.is_none());

        let catalog = virtues_registry::sql_catalog::get_table_metadata();
        for ont in virtues_registry::ontologies::registered_ontologies() {
            if !catalog.contains_key(ont.table_name) {
                continue;
            }
            let sql = format!(
                "SELECT MIN({0})::date, MAX({0})::date FROM {1}",
                ont.timestamp_column, ont.table_name
            );
            sqlx::query_as::<_, (Option<NaiveDate>, Option<NaiveDate>)>(&sql)
                .fetch_one(&pool)
                .await
                .unwrap_or_else(|e| panic!("{}.{} is not readable: {e}", ont.table_name, ont.timestamp_column));
        }
    }

    /// One row makes one line, day-floored, and a gap is said in days.
    #[sqlx::test]
    async fn a_table_with_rows_gets_a_line_with_its_range(pool: PgPool) {
        sqlx::query(
            "INSERT INTO data_health_steps (id, step_count, occurred_at, source_stream_id, source_table, source_provider)
             VALUES ('s1', 100, '2026-09-01T10:00:00Z', 'stream_t', 'steps', 'test'),
                    ('s2', 200, '2026-09-10T10:00:00Z', 'stream_t2', 'steps', 'test')",
        )
        .execute(&pool)
        .await
        .expect("steps row");
        let block = build_coverage(&pool, NaiveDate::from_ymd_opt(2026, 9, 21).unwrap())
            .await
            .expect("a block");
        assert!(block.contains("data_health_steps: 2026-09-01 → 2026-09-10 — nothing for 11 days"), "{block}");
        assert!(!block.contains("data_health_sleep"), "empty tables render nothing: {block}");
    }
}

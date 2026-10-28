//! An applet's `run` SQL: the deterministic stage between its trigger and
//! its prompt.
//!
//! It runs read-only on the reader login, the one view SQL uses
//! (`server::faces::reader_transaction`), so it reads what a view can read
//! and writes nothing. Its rows go to the prompt as a table in the run's
//! context, first [`MAX_ROWS`] of them and the total. No rows means there is
//! nothing to do: with a prompt after it the run is recorded as skipped and
//! no model is called. The design is in agents/plan/applet-shape.md, under
//! `run`.

use crate::error::{Error, Result};

/// The most rows handed to the prompt. It can query for more with
/// `sql_query`.
pub(super) const MAX_ROWS: usize = 200;

/// How long the SQL may take.
const TIMEOUT: &str = "10s";

/// What the SQL returned.
#[derive(Debug)]
pub(super) struct SqlResult {
    pub columns: Vec<String>,
    /// The first [`MAX_ROWS`], as `convert_rows_to_json` writes them.
    pub rows: Vec<serde_json::Value>,
    pub total: i64,
}

pub(super) async fn run(pool: &sqlx::PgPool, sql: &str) -> Result<SqlResult> {
    use sqlx::{Column, Row};
    let fail = |e: String| Error::Other(format!("its SQL failed: {e}"));
    let (mut tx, zone) = crate::server::faces::reader_transaction(pool, TIMEOUT).await.map_err(fail)?;
    let total: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM ({sql}) q"))
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| fail(e.to_string()))?;
    let rows = if total == 0 {
        Vec::new()
    } else {
        sqlx::query(&format!("SELECT * FROM ({sql}) q LIMIT {MAX_ROWS}"))
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| fail(e.to_string()))?
    };
    tx.rollback().await.ok();
    let columns = rows
        .first()
        .map(|r| r.columns().iter().map(|c| c.name().to_string()).collect())
        .unwrap_or_default();
    let rows = crate::tools::sql_query::convert_rows_to_json(&rows, Some(&zone));
    Ok(SqlResult { columns, rows, total })
}

impl SqlResult {
    /// The rows as the prompt reads them: a markdown table in the SELECT's
    /// column order, then how many there were.
    pub fn for_prompt(&self) -> String {
        let cell = |v: Option<&serde_json::Value>| match v {
            None | Some(serde_json::Value::Null) => String::new(),
            Some(serde_json::Value::String(s)) => s.replace('|', "\\|").replace('\n', " "),
            Some(other) => other.to_string(),
        };
        let mut out = format!("Your run's SQL returned these rows:\n\n| {} |\n|", self.columns.join(" | "));
        out.push_str(&" --- |".repeat(self.columns.len()));
        out.push('\n');
        for row in &self.rows {
            let cells: Vec<String> = self.columns.iter().map(|c| cell(row.get(c))).collect();
            out.push_str(&format!("| {} |\n", cells.join(" | ")));
        }
        if self.total as usize > self.rows.len() {
            out.push_str(&format!(
                "\nThese are the first {} of {} rows. Query for more with sql_query if you need them.",
                self.rows.len(),
                self.total
            ));
        } else {
            out.push_str(&format!("\n{} {}.", self.total, if self.total == 1 { "row" } else { "rows" }));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prompt_reads_the_rows_in_order_with_the_total() {
        let r = SqlResult {
            columns: vec!["day".into(), "bpm".into()],
            rows: vec![serde_json::json!({ "bpm": 121, "day": "Mon | early" })],
            total: 250,
        };
        let text = r.for_prompt();
        assert!(text.contains("| day | bpm |"), "{text}");
        assert!(text.contains("| Mon \\| early | 121 |"), "{text}");
        assert!(text.contains("first 1 of 250 rows"), "{text}");
    }

    #[sqlx::test]
    async fn it_returns_rows_and_counts_them(pool: sqlx::PgPool) {
        let r = run(&pool, "select g as n from generate_series(1, 250) g").await.unwrap();
        assert_eq!(r.total, 250);
        assert_eq!(r.rows.len(), MAX_ROWS);
        assert_eq!(r.columns, ["n"]);
        let none = run(&pool, "select 1 where false").await.unwrap();
        assert_eq!(none.total, 0);
        assert!(run(&pool, "select * from no_such_table").await.unwrap_err().to_string().contains("its SQL failed"));
    }
}

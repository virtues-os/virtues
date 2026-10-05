//! `sql_query` / `sql_write` in a sudo turn: any statement, as the app's own
//! database role, with none of the guards those tools carry in chat.
//!
//! In chat `sql_query` is read-only and drops to a reader role, and
//! `sql_write` only reaches `applet_*` schemas. Those guards exist because the
//! tools also run unattended over content nobody reviewed. A sudo turn is the
//! owner, present, who already holds a root shell — so the database tools stop
//! being the narrow door and become the convenient one: DDL, DML, anything on
//! any table, in one call, without reaching for psql.
//!
//! One statement per call (extended protocol). Several at once: the shell and
//! `psql -c`.
//!
//! Until the owner allows a statement it runs in a `READ ONLY` transaction, so
//! Postgres decides what is a write, and a refused write becomes the question
//! (`sudo_gate`).

use futures::TryStreamExt;
use sqlx::PgPool;

use super::executor::{ToolError, ToolResult};

const RETURN_ROWS_MAX: usize = 1000;

/// The statement a call carries, as the grant names it.
pub fn statement(arguments: &serde_json::Value) -> Result<String, ToolError> {
    Ok(arguments
        .get("sql")
        .or_else(|| arguments.get("query"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ToolError::InvalidParameters("sql is required".into()))?
        .trim_end_matches(';')
        .to_string())
}

pub async fn execute(pool: &PgPool, sql: &str, read_only: bool) -> Result<ToolResult, ToolError> {
    // What a read-only transaction would still let act.
    if read_only && super::sudo_gate::sql_acts_anyway(sql) {
        return Ok(super::sudo_gate::ask("sql", sql));
    }
    let ran = if read_only {
        let mut tx = match pool.begin().await {
            Ok(tx) => tx,
            Err(e) => return Ok(failed(&e)),
        };
        if let Err(e) = sqlx::query("SET TRANSACTION READ ONLY").execute(&mut *tx).await {
            return Ok(failed(&e));
        }
        let ran = run(&mut *tx, sql).await;
        // Nothing to keep: the transaction could not write.
        let _ = tx.rollback().await;
        ran
    } else {
        run(pool, sql).await
    };
    let (rows, rows_affected) = match ran {
        Ok(r) => r,
        Err(e) if read_only && super::sudo_gate::is_read_only_refusal(&e.to_string()) => {
            return Ok(super::sudo_gate::ask("sql", sql));
        }
        Err(e) => return Ok(failed(&e)),
    };

    let row_count = rows.len();
    let capped = row_count.min(RETURN_ROWS_MAX);
    let mut data = serde_json::json!({
        "rows": super::sql_query::convert_rows_to_json(&rows[..capped]),
        "row_count": row_count,
        "rows_affected": rows_affected,
    });
    if row_count > capped {
        data["truncated"] = serde_json::json!(format!(
            "showing {capped} of {row_count} rows; add a LIMIT or aggregate"
        ));
    }
    Ok(ToolResult::success(data))
}

fn failed(e: &sqlx::Error) -> ToolResult {
    ToolResult::success(serde_json::json!({ "status": "error", "error": e.to_string() }))
}

async fn run<'e, E>(ex: E, sql: &'e str) -> Result<(Vec<sqlx::postgres::PgRow>, u64), sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    let mut rows = Vec::new();
    let mut rows_affected = 0u64;
    // Deprecated upstream because it invites several statements per call;
    // this is one statement, and it is the only call that yields both the
    // rows and the affected count without running the statement twice.
    #[allow(deprecated)]
    let mut stream = sqlx::query(sql).fetch_many(ex);
    while let Some(item) = stream.try_next().await? {
        match item {
            sqlx::Either::Left(done) => rows_affected += done.rows_affected(),
            sqlx::Either::Right(row) => rows.push(row),
        }
    }
    Ok((rows, rows_affected))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    async fn run_sql(pool: &PgPool, args: serde_json::Value, read_only: bool) -> ToolResult {
        execute(pool, &statement(&args).unwrap(), read_only).await.unwrap()
    }

    #[sqlx::test(migrations = false)]
    async fn runs_ddl_writes_and_reads_once_allowed(pool: PgPool) {
        let r = run_sql(&pool, json!({ "sql": "CREATE TABLE t (n int)" }), false).await;
        assert!(r.data.get("status").is_none(), "{:?}", r.data);
        let r = run_sql(&pool, json!({ "sql": "INSERT INTO t VALUES (1), (2)" }), false).await;
        assert_eq!(r.data["rows_affected"], 2);
        let r = run_sql(&pool, json!({ "operation": "query", "sql": "SELECT n FROM t ORDER BY n;" }), true).await;
        assert_eq!(r.data["row_count"], 2);
        assert_eq!(r.data["rows"][1]["n"], 2);
        let r = run_sql(&pool, json!({ "sql": "SELECT nope FROM t" }), true).await;
        assert_eq!(r.data["status"], "error");
    }

    #[sqlx::test(migrations = false)]
    async fn a_write_not_yet_allowed_asks_and_changes_nothing(pool: PgPool) {
        run_sql(&pool, json!({ "sql": "CREATE TABLE t (n int)" }), false).await;
        let sql = "INSERT INTO t VALUES (1)";
        let r = run_sql(&pool, json!({ "sql": sql }), true).await;
        assert_eq!(r.data["awaiting_owner"], true, "{:?}", r.data);
        assert_eq!(r.data["entity_id"], crate::tools::sudo_gate::grant_id("sql", sql));
        let r = run_sql(&pool, json!({ "sql": "DROP TABLE t" }), true).await;
        assert_eq!(r.data["awaiting_owner"], true, "{:?}", r.data);
        let r = run_sql(&pool, json!({ "sql": "COPY (SELECT 1) TO PROGRAM 'true'" }), true).await;
        assert_eq!(r.data["awaiting_owner"], true, "{:?}", r.data);
        let r = run_sql(&pool, json!({ "sql": "VACUUM t" }), true).await;
        assert_eq!(r.data["awaiting_owner"], true, "{:?}", r.data);
        let r = run_sql(&pool, json!({ "sql": "SELECT count(*) AS n FROM t" }), true).await;
        assert_eq!(r.data["rows"][0]["n"], 0);
    }
}

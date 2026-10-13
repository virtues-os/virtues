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
//! A statement that deletes (`DROP`, `TRUNCATE`, `DELETE`) waits for the
//! owner to allow it (`sudo_gate`, checked by the executor); everything else
//! runs.

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

/// `timezone` is the owner's, as for `sql_query`.
pub async fn execute(pool: &PgPool, sql: &str, timezone: Option<&str>) -> Result<ToolResult, ToolError> {
    let ran = match in_transaction(pool, sql, timezone).await {
        // VACUUM, CREATE INDEX CONCURRENTLY and ALTER SYSTEM refuse a
        // transaction before doing anything, so running them bare is not
        // running them twice. They keep the server's zone.
        Err(e) if e.to_string().contains("cannot run inside a transaction block") => run(pool, sql).await,
        ran => ran,
    };
    let (rows, rows_affected) = match ran {
        Ok(r) => r,
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

/// Dates in the owner's zone, as in chat's `sql_query`: `SET LOCAL` needs a
/// transaction to bind to, and a session `SET` would follow the connection
/// back into the pool.
async fn in_transaction(
    pool: &PgPool,
    sql: &str,
    timezone: Option<&str>,
) -> Result<(Vec<sqlx::postgres::PgRow>, u64), sqlx::Error> {
    let mut tx = pool.begin().await?;
    crate::timezone::set_local_timezone(&mut tx, timezone).await?;
    let ran = run(&mut *tx, sql).await?;
    tx.commit().await?;
    Ok(ran)
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

    async fn run_sql(pool: &PgPool, args: serde_json::Value) -> ToolResult {
        execute(pool, &statement(&args).unwrap(), None).await.unwrap()
    }

    #[sqlx::test]
    async fn runs_ddl_writes_and_reads(pool: PgPool) {
        let r = run_sql(&pool, json!({ "sql": "CREATE TABLE t (n int)" })).await;
        assert!(r.data.get("status").is_none(), "{:?}", r.data);
        let r = run_sql(&pool, json!({ "sql": "INSERT INTO t VALUES (1), (2)" })).await;
        assert_eq!(r.data["rows_affected"], 2);
        let r = run_sql(&pool, json!({ "operation": "query", "sql": "SELECT n FROM t ORDER BY n;" })).await;
        assert_eq!(r.data["row_count"], 2);
        assert_eq!(r.data["rows"][1]["n"], 2);
        let r = run_sql(&pool, json!({ "sql": "SELECT nope FROM t" })).await;
        assert_eq!(r.data["status"], "error");
    }

    #[sqlx::test]
    async fn a_statement_that_refuses_a_transaction_still_runs(pool: PgPool) {
        run_sql(&pool, json!({ "sql": "CREATE TABLE t (n int)" })).await;
        let r = run_sql(&pool, json!({ "sql": "VACUUM t" })).await;
        assert!(r.data.get("status").is_none(), "{:?}", r.data);
        let r = run_sql(&pool, json!({ "sql": "CREATE INDEX CONCURRENTLY t_n ON t (n)" })).await;
        assert!(r.data.get("status").is_none(), "{:?}", r.data);
    }

    #[sqlx::test]
    async fn a_statement_is_in_the_owners_zone(pool: PgPool) {
        let sql = "SELECT current_setting('TimeZone') AS tz, \
                   to_char('2026-10-07 11:00:00+00'::timestamptz, 'HH24:MI') AS label";
        let r = execute(&pool, sql, Some("America/Chicago")).await.unwrap();
        assert_eq!(r.data["rows"][0]["tz"], "America/Chicago", "{:?}", r.data);
        assert_eq!(r.data["rows"][0]["label"], "06:00");
        let r = execute(&pool, sql, Some("Not/AZone")).await.unwrap();
        assert_eq!(r.data["rows"][0]["tz"], "UTC", "{:?}", r.data);
    }
}

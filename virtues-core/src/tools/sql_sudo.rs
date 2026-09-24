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

use futures::TryStreamExt;
use sqlx::PgPool;

use super::executor::{ToolError, ToolResult};

const RETURN_ROWS_MAX: usize = 1000;

pub async fn execute(pool: &PgPool, arguments: serde_json::Value) -> Result<ToolResult, ToolError> {
    let sql = arguments
        .get("sql")
        .or_else(|| arguments.get("query"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ToolError::InvalidParameters("sql is required".into()))?
        .trim_end_matches(';')
        .to_string();

    let mut rows = Vec::new();
    let mut rows_affected = 0u64;
    // Deprecated upstream because it invites several statements per call;
    // this is one statement, and it is the only call that yields both the
    // rows and the affected count without running the statement twice.
    #[allow(deprecated)]
    let mut stream = sqlx::query(&sql).fetch_many(pool);
    loop {
        match stream.try_next().await {
            Ok(Some(sqlx::Either::Left(done))) => rows_affected += done.rows_affected(),
            Ok(Some(sqlx::Either::Right(row))) => rows.push(row),
            Ok(None) => break,
            Err(e) => {
                return Ok(ToolResult::success(serde_json::json!({
                    "status": "error",
                    "error": e.to_string(),
                })))
            }
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[sqlx::test(migrations = false)]
    async fn runs_ddl_writes_and_reads(pool: PgPool) {
        let r = execute(&pool, json!({ "sql": "CREATE TABLE t (n int)" })).await.unwrap();
        assert!(r.data.get("status").is_none(), "{:?}", r.data);
        let r = execute(&pool, json!({ "sql": "INSERT INTO t VALUES (1), (2)" })).await.unwrap();
        assert_eq!(r.data["rows_affected"], 2);
        let r = execute(&pool, json!({ "operation": "query", "sql": "SELECT n FROM t ORDER BY n;" }))
            .await
            .unwrap();
        assert_eq!(r.data["row_count"], 2);
        assert_eq!(r.data["rows"][1]["n"], 2);
        let r = execute(&pool, json!({ "sql": "SELECT nope FROM t" })).await.unwrap();
        assert_eq!(r.data["status"], "error");
    }
}

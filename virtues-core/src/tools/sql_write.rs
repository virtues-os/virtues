//! `sql_write` — the scoped write path into applet-owned tables.
//!
//! Runs one statement as the `virtues_applet_writer` PG role, whose grants
//! cover DML **inside `applet_*` schemas only** (migration 0054 + the boot
//! grants in `server::faces`). Scope is enforced by Postgres, not by parsing
//! the SQL: a statement touching `data_*` / `app_*` / anything else fails
//! with a permission error at the database.
//!
//! One statement per call, over the extended protocol — a smuggled
//! multi-statement string is a protocol error, not an escape.

use sqlx::PgPool;

use super::executor::{ToolError, ToolResult};

const SQL_MAX: usize = 16 * 1024;
const RETURN_ROWS_MAX: usize = 500;

pub async fn execute(pool: &PgPool, arguments: serde_json::Value) -> Result<ToolResult, ToolError> {
    let sql = arguments
        .get("sql")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ToolError::InvalidParameters("sql is required".into()))?;

    if sql.len() > SQL_MAX {
        return Ok(fail("statement too long (16KB max)"));
    }
    // Single statement only. A trailing semicolon is tolerated; interior ones
    // are rejected up front with a legible error (the extended protocol would
    // reject them anyway, less helpfully).
    if sql.trim_end_matches(';').contains(';') {
        return Ok(fail("one statement per call — split multiple statements into separate calls"));
    }

    let owned_sql = sql.trim_end_matches(';').to_string();
    let wants_rows = owned_sql.to_lowercase().contains("returning");

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => return Ok(fail(&format!("begin failed: {e}"))),
    };
    for setup in [
        "SET LOCAL ROLE virtues_applet_writer",
        "SET LOCAL statement_timeout = '5s'",
    ] {
        if let Err(e) = sqlx::query(setup).execute(&mut *tx).await {
            return Ok(fail(&format!("setup failed: {e}")));
        }
    }

    if wants_rows {
        match sqlx::query(&owned_sql).fetch_all(&mut *tx).await {
            Ok(rows) => {
                if let Err(e) = tx.commit().await {
                    return Ok(fail(&format!("commit failed: {e}")));
                }
                let capped = rows.len().min(RETURN_ROWS_MAX);
                let json_rows = super::sql_query::convert_rows_to_json(&rows[..capped]);
                Ok(ToolResult::success(serde_json::json!({
                    "rows": json_rows,
                    "row_count": rows.len(),
                })))
            }
            Err(e) => {
                tx.rollback().await.ok();
                Ok(fail(&e.to_string()))
            }
        }
    } else {
        match sqlx::query(&owned_sql).execute(&mut *tx).await {
            Ok(done) => {
                if let Err(e) = tx.commit().await {
                    return Ok(fail(&format!("commit failed: {e}")));
                }
                Ok(ToolResult::success(serde_json::json!({
                    "rows_affected": done.rows_affected(),
                })))
            }
            Err(e) => {
                tx.rollback().await.ok();
                Ok(fail(&e.to_string()))
            }
        }
    }
}

fn fail(msg: &str) -> ToolResult {
    ToolResult::success(serde_json::json!({
        "status": "error",
        "error": msg,
        "hint": "sql_write can only touch tables in applet_* schemas (create them via setup_applet's schema_sql)",
    }))
}

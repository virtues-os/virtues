//! `sql_write` — the scoped write path into applet-owned tables.
//!
//! Runs one statement logged in as the `virtues_applet_writer` PG role
//! (`database::restricted`), whose grants cover DML **inside `applet_*`
//! schemas only** (migration 0054 + the boot grants in `server::faces`).
//! Scope is enforced by Postgres, not by parsing
//! the SQL: a statement touching `data_*` / `app_*` / anything else fails
//! with a permission error at the database.
//!
//! One statement per call, over the extended protocol — a smuggled
//! multi-statement string is a protocol error, not an escape.

use sqlx::PgPool;

use super::executor::{ToolError, ToolResult};

const SQL_MAX: usize = 16 * 1024;
const RETURN_ROWS_MAX: usize = 500;

/// `timezone` is the owner's; it only puts returned times on their clock.
/// `run_id` is the applet run the write is part of, which records the rows it
/// wrote so the run can show them.
pub async fn execute(
    pool: &PgPool,
    arguments: serde_json::Value,
    timezone: Option<&str>,
    run_id: Option<&str>,
) -> Result<ToolResult, ToolError> {
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
    // RETURNING as a whole word (not the substring inside e.g. a string
    // literal "returning"). Word-boundary check on lowercased tokens.
    let lower = owned_sql.to_lowercase();
    let wants_rows = lower
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .any(|w| w == "returning");

    let mut tx = match crate::database::restricted::begin(pool, crate::database::restricted::Login::Writer).await {
        Ok(tx) => tx,
        Err(e) => return Ok(fail(&format!("begin failed: {e}"))),
    };
    for setup in [
        "SET LOCAL statement_timeout = '5s'",
        // Force fully-qualified names; unqualified writes can't fall through
        // to public even on PG versions where PUBLIC holds create-in-public.
        "SET LOCAL search_path = ''",
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
                record(pool, run_id, &owned_sql, rows.len() as i64).await;
                let capped = rows.len().min(RETURN_ROWS_MAX);
                // The write has committed; a failed zone lookup only leaves the
                // returned times in UTC, marked `+00:00`.
                let zone = crate::timezone::resolve_timezone(pool, timezone)
                    .await
                    .inspect_err(|e| tracing::warn!(error = %e, "sql_write: zone lookup failed; times stay UTC"))
                    .ok();
                let json_rows = super::sql_query::convert_rows_to_json(&rows[..capped], zone.as_deref());
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
                record(pool, run_id, &owned_sql, done.rows_affected() as i64).await;
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

/// Record on the run the rows a committed write touched. A record that fails
/// is logged: the write itself has happened.
async fn record(pool: &PgPool, run_id: Option<&str>, sql: &str, rows: i64) {
    let (Some(run_id), Some(table)) = (run_id, target_table(sql)) else { return };
    if let Err(e) = crate::scheduler::applets::record_rows_output(pool, run_id, &table, rows).await {
        tracing::warn!(run_id, table, error = %e, "could not record the rows a run wrote");
    }
}

/// The table an INSERT, UPDATE or DELETE writes: the name after `INSERT
/// INTO`, `UPDATE` or `DELETE FROM`.
fn target_table(sql: &str) -> Option<String> {
    let words: Vec<String> = sql.split_whitespace().map(|w| w.to_lowercase()).collect();
    let after = match words.first()?.as_str() {
        "insert" | "delete" => 2,
        "update" => 1,
        _ => return None,
    };
    let name = words.get(after)?;
    let name = name.split(['(', ',']).next()?.trim();
    (!name.is_empty()).then(|| name.to_string())
}

fn fail(msg: &str) -> ToolResult {
    ToolResult::success(serde_json::json!({
        "status": "error",
        "error": msg,
        "hint": "sql_write can only touch tables in applet_* schemas (create them with the applet tool's setup action, tables)",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_a_write_names() {
        assert_eq!(target_table("INSERT INTO applet_water.drinks (ml) VALUES (250)").as_deref(), Some("applet_water.drinks"));
        assert_eq!(target_table("insert into applet_water.drinks(ml) values (1)").as_deref(), Some("applet_water.drinks"));
        assert_eq!(target_table("UPDATE applet_water.drinks SET ml = 1").as_deref(), Some("applet_water.drinks"));
        assert_eq!(target_table("DELETE FROM applet_water.drinks WHERE true").as_deref(), Some("applet_water.drinks"));
        assert_eq!(target_table("WITH x AS (SELECT 1) INSERT INTO t VALUES (1)"), None);
    }
}

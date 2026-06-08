//! Developer API for database introspection and SQL execution.
//!
//! `execute_sql` runs an arbitrary query inside a read-only transaction —
//! `SET TRANSACTION READ ONLY` is the pg equivalent of opening the connection
//! `read_only(true)` (what we did before Postgres). Any attempt to write inside
//! the transaction is rejected by the server with `read-only transaction`.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use sqlx::{Column, Executor, Row, TypeInfo, ValueRef};
use std::collections::HashMap;

/// Request for executing a SQL query
#[derive(Debug, Deserialize)]
pub struct ExecuteSqlRequest {
    pub sql: String,
}

/// Result of a SQL query, including column names (even for empty results)
#[derive(Debug, Serialize)]
pub struct SqlQueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<HashMap<String, serde_json::Value>>,
}

/// Execute a SQL query inside a `READ ONLY` transaction and return JSON rows.
pub async fn execute_sql(
    pool: &sqlx::PgPool,
    request: ExecuteSqlRequest,
) -> Result<SqlQueryResult> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| Error::Database(format!("Failed to start transaction: {}", e)))?;

    sqlx::query("SET TRANSACTION READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to set read-only: {}", e)))?;

    let rows = sqlx::query(&request.sql)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Query execution failed: {}", e)))?;

    let columns: Vec<String> = if !rows.is_empty() {
        rows[0].columns().iter().map(|c| c.name().to_string()).collect()
    } else {
        match (&mut *tx).describe(&request.sql).await {
            Ok(desc) => desc.columns.iter().map(|c| c.name().to_string()).collect(),
            Err(_) => vec![],
        }
    };

    let mut results = Vec::new();

    for row in rows {
        let mut row_map = HashMap::new();

        for col in row.columns() {
            let name = col.name();
            let raw_value = row.try_get_raw(col.ordinal()).unwrap();

            let json_val = if raw_value.is_null() {
                serde_json::Value::Null
            } else {
                match col.type_info().name() {
                    "INT2" | "INT4" | "INT8" | "INTEGER" | "BIGINT" | "SMALLINT" => {
                        let v: Option<i64> = row.try_get(col.ordinal()).ok();
                        match v {
                            Some(n) => serde_json::Value::Number(n.into()),
                            None => serde_json::Value::Null,
                        }
                    }
                    "FLOAT4" | "FLOAT8" | "REAL" | "DOUBLE PRECISION" | "NUMERIC" => {
                        let v: Option<f64> = row.try_get(col.ordinal()).ok();
                        match v {
                            Some(n) => serde_json::json!(n),
                            None => serde_json::Value::Null,
                        }
                    }
                    "BOOL" | "BOOLEAN" => {
                        let v: Option<bool> = row.try_get(col.ordinal()).ok();
                        match v {
                            Some(b) => serde_json::Value::Bool(b),
                            None => serde_json::Value::Null,
                        }
                    }
                    "BYTEA" => {
                        let v: Option<Vec<u8>> = row.try_get(col.ordinal()).ok();
                        match v {
                            Some(bytes) => serde_json::Value::String(format!(
                                "<BYTEA: {} bytes>",
                                bytes.len()
                            )),
                            None => serde_json::Value::Null,
                        }
                    }
                    "JSON" | "JSONB" => {
                        let v: Option<serde_json::Value> = row.try_get(col.ordinal()).ok();
                        v.unwrap_or(serde_json::Value::Null)
                    }
                    "TIMESTAMPTZ" | "TIMESTAMP" => {
                        let v: Option<chrono::DateTime<chrono::Utc>> =
                            row.try_get(col.ordinal()).ok();
                        match v {
                            Some(ts) => serde_json::Value::String(ts.to_rfc3339()),
                            None => serde_json::Value::Null,
                        }
                    }
                    // Default to string for TEXT, VARCHAR, UUID, etc.
                    _ => {
                        let v: Option<String> = row.try_get(col.ordinal()).ok();
                        match v {
                            Some(s) => serde_json::Value::String(s),
                            None => serde_json::Value::Null,
                        }
                    }
                }
            };

            row_map.insert(name.to_string(), json_val);
        }

        results.push(row_map);
    }

    // Commit the read-only tx (effectively no-op).
    let _ = tx.commit().await;

    Ok(SqlQueryResult { columns, rows: results })
}

/// List all user tables in the public schema.
pub async fn list_tables(pool: &sqlx::PgPool) -> Result<Vec<String>> {
    let query = "SELECT tablename AS name FROM pg_tables \
                 WHERE schemaname = 'public' ORDER BY tablename";

    let rows = sqlx::query(query)
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to list tables: {}", e)))?;

    let tables: Vec<String> = rows.iter().map(|row| row.get("name")).collect();

    Ok(tables)
}

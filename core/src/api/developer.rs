//! Developer API for database introspection and SQL execution

use crate::error::{Error, Result};
use serde::Deserialize;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Column, ConnectOptions, Row, TypeInfo, ValueRef};
use std::collections::HashMap;
use std::str::FromStr;

/// Request for executing a SQL query
#[derive(Debug, Deserialize)]
pub struct ExecuteSqlRequest {
    pub sql: String,
}

/// Execute a SQL query in read-only mode and return results as JSON
pub async fn execute_sql(
    pool: &sqlx::SqlitePool,
    request: ExecuteSqlRequest,
) -> Result<Vec<HashMap<String, serde_json::Value>>> {
    // Get the database URL from environment (same as main pool)
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:./data/virtues.db".to_string());
    
    // Parse the URL and create read-only options
    let base_options = SqliteConnectOptions::from_str(&database_url)
        .map_err(|e| Error::Database(format!("Invalid database URL: {}", e)))?;
    
    // Create a NEW connection with read_only(true)
    // This is the critical safety check - SQLite engine will reject writes
    let read_only_options = base_options
        .read_only(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

    // Connect specifically for this query
    let mut conn = read_only_options
        .connect()
        .await
        .map_err(|e| Error::Database(format!("Failed to connect in read-only mode: {}", e)))?;

    // Execute the query using the dynamic query interface
    let rows = sqlx::query(&request.sql)
        .fetch_all(&mut conn)
        .await
        .map_err(|e| Error::Database(format!("Query execution failed: {}", e)))?;

    // Convert rows to JSON
    let mut results = Vec::new();

    for row in rows {
        let mut row_map = HashMap::new();

        for col in row.columns() {
            let name = col.name();
            
            // Handle different types dynamically
            let raw_value = row.try_get_raw(col.ordinal()).unwrap();
            
            let json_val = if raw_value.is_null() {
                serde_json::Value::Null
            } else {
                match col.type_info().name() {
                    "INTEGER" | "BIGINT" | "INT8" => {
                        let v: Option<i64> = row.try_get(col.ordinal()).ok();
                        match v {
                            Some(n) => serde_json::Value::Number(n.into()),
                            None => serde_json::Value::Null
                        }
                    }
                    "REAL" | "DOUBLE" | "FLOAT" => {
                        let v: Option<f64> = row.try_get(col.ordinal()).ok();
                        match v {
                            Some(n) => serde_json::json!(n),
                            None => serde_json::Value::Null
                        }
                    }
                    "BOOLEAN" | "BOOL" => {
                        let v: Option<bool> = row.try_get(col.ordinal()).ok();
                        match v {
                            Some(b) => serde_json::Value::Bool(b),
                            None => serde_json::Value::Null
                        }
                    }
                    // Handle BLOB data - show byte count since it can't be displayed as text
                    "BLOB" => {
                        let v: Option<Vec<u8>> = row.try_get(col.ordinal()).ok();
                        match v {
                            Some(bytes) => serde_json::Value::String(format!("<BLOB: {} bytes>", bytes.len())),
                            None => serde_json::Value::Null
                        }
                    }
                    // Default to string for TEXT, etc.
                    _ => {
                        let v: Option<String> = row.try_get(col.ordinal()).ok();
                        match v {
                            Some(s) => serde_json::Value::String(s),
                            None => serde_json::Value::Null
                        }
                    }
                }
            };
            
            row_map.insert(name.to_string(), json_val);
        }

        results.push(row_map);
    }

    Ok(results)
}

/// List all tables in the database (excluding internal sqlite_ tables)
pub async fn list_tables(pool: &sqlx::SqlitePool) -> Result<Vec<String>> {
    let query = "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name";
    
    let rows = sqlx::query(query)
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to list tables: {}", e)))?;
        
    let tables: Vec<String> = rows
        .iter()
        .map(|row| row.get("name"))
        .collect();
        
    Ok(tables)
}

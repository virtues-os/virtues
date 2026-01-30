//! MCP Tool utilities
//!
//! This module provides utilities for MCP tool implementations.
//! The actual tool definitions and execution are in core/src/tools/.

use sqlx::{Column, Row, TypeInfo};

// ============================================================================
// Shared Utilities
// ============================================================================

/// Convert SQLite rows to JSON array with comprehensive type support
///
/// This function handles NULL values, integers, floats, decimals, booleans,
/// dates, timestamps, JSON, and falls back gracefully for unsupported types.
pub fn convert_rows_to_json(rows: &[sqlx::sqlite::SqliteRow]) -> Vec<serde_json::Value> {
    let mut json_rows = Vec::new();

    for row in rows {
        let mut obj = serde_json::Map::new();

        for (i, col) in row.columns().iter().enumerate() {
            let col_name = col.name();

            // Try to extract value based on type, with comprehensive type support
            let value: serde_json::Value =
                // Try NULL first - check if the column value is NULL
                if let Ok(opt_val) = row.try_get::<Option<String>, _>(i) {
                    if opt_val.is_none() {
                        serde_json::Value::Null
                    } else {
                        // We know it's not NULL, so unwrap the Option
                        serde_json::Value::String(opt_val.unwrap())
                    }
                }
                // String types (if NULL check didn't work, try direct string)
                else if let Ok(v) = row.try_get::<String, _>(i) {
                    serde_json::Value::String(v)
                }
                // Integer types
                else if let Ok(v) = row.try_get::<i64, _>(i) {
                    serde_json::Value::Number(v.into())
                } else if let Ok(v) = row.try_get::<i32, _>(i) {
                    serde_json::Value::Number(v.into())
                } else if let Ok(v) = row.try_get::<i16, _>(i) {
                    serde_json::Value::Number(v.into())
                }
                // Float types
                else if let Ok(v) = row.try_get::<f64, _>(i) {
                    serde_json::json!(v)
                } else if let Ok(v) = row.try_get::<f32, _>(i) {
                    serde_json::json!(v)
                }
                // Boolean (SQLite stores as INTEGER 0/1)
                else if let Ok(v) = row.try_get::<bool, _>(i) {
                    serde_json::Value::Bool(v)
                }
                // Date/Time types (convert to string)
                else if let Ok(v) = row.try_get::<chrono::NaiveDate, _>(i) {
                    serde_json::Value::String(v.format("%Y-%m-%d").to_string())
                } else if let Ok(v) = row.try_get::<chrono::NaiveDateTime, _>(i) {
                    serde_json::Value::String(v.format("%Y-%m-%d %H:%M:%S").to_string())
                } else if let Ok(v) = row.try_get::<chrono::DateTime<chrono::Utc>, _>(i) {
                    serde_json::Value::String(v.to_rfc3339())
                } else if let Ok(v) = row.try_get::<chrono::DateTime<chrono::FixedOffset>, _>(i) {
                    serde_json::Value::String(v.to_rfc3339())
                } else if let Ok(v) = row.try_get::<chrono::DateTime<chrono::Local>, _>(i) {
                    serde_json::Value::String(v.to_rfc3339())
                }
                // JSONB/JSON
                else if let Ok(v) = row.try_get::<serde_json::Value, _>(i) {
                    v
                }
                // UUID
                else if let Ok(v) = row.try_get::<uuid::Uuid, _>(i) {
                    serde_json::Value::String(v.to_string())
                }
                // Fallback: try to get raw value info for better error message
                else {
                    let type_info = col.type_info().name();
                    tracing::warn!(
                        "Column '{}' with SQLite type '{}' could not be extracted - falling back to placeholder",
                        col_name,
                        type_info
                    );
                    serde_json::Value::String(format!("<unsupported type: {}>", type_info))
                };

            obj.insert(col_name.to_string(), value);
        }

        json_rows.push(serde_json::Value::Object(obj));
    }

    json_rows
}

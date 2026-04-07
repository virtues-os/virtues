//! Ontologies API
//!
//! Endpoints for querying available ontology tables based on enabled streams.

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use std::collections::HashSet;

use crate::error::{Error, Result};
use crate::registry;
use crate::tools::sql_query::convert_rows_to_json;
use virtues_registry::ontologies::registered_ontologies;

/// List available ontology tables based on enabled streams
///
/// This queries the database for enabled streams and maps them to ontology tables
/// using the source registry as the single source of truth.
/// Only returns tables that both (1) have enabled streams AND (2) actually exist in the database schema.
pub async fn list_available_ontologies(db: &SqlitePool) -> Result<Vec<String>> {
    // First, get all tables that actually exist in the database
    // SQLite uses sqlite_master instead of information_schema
    let existing_tables = sqlx::query!(
        r#"
        SELECT name as table_name
        FROM sqlite_master
        WHERE type = 'table'
          AND name LIKE 'data_%'
          AND name NOT LIKE 'data_stream_%'
          AND name NOT IN ('data_sources', 'data_streams', 'data_sync_logs')
        "#
    )
    .fetch_all(db)
    .await?;

    let existing_set: HashSet<String> = existing_tables
        .into_iter()
        .filter_map(|row| row.table_name)
        .collect();

    tracing::debug!(
        count = existing_set.len(),
        tables = ?existing_set,
        "Found existing ontology tables in database schema"
    );

    // Query enabled streams from database
    let rows = sqlx::query!(
        r#"
        SELECT DISTINCT s.table_name
        FROM elt_stream_connections s
        JOIN elt_source_connections src ON s.source_connection_id = src.id
        WHERE s.is_enabled = true
          AND src.is_active = true
        "#
    )
    .fetch_all(db)
    .await?;

    // Map stream tables to ontology tables using source registry
    // AND filter by actual schema existence
    let mut ontologies = HashSet::new();
    for row in rows {
        if let Some((_source_name, stream)) = registry::get_stream_by_table_name(&row.table_name) {
            // Add all target ontology tables for this stream
            for target_table in &stream.descriptor.target_ontologies {
                // Only include if table actually exists in the schema
                if existing_set.contains(*target_table) {
                    ontologies.insert(target_table.to_string());
                } else {
                    tracing::warn!(
                        stream = %row.table_name,
                        target = %target_table,
                        "Target ontology table does not exist in database schema (transform may not have run yet)"
                    );
                }
            }
        } else {
            tracing::debug!(
                table_name = %row.table_name,
                "No stream found in registry, skipping"
            );
        }
    }

    // Return sorted list
    let mut result: Vec<String> = ontologies.into_iter().collect();
    result.sort();

    tracing::info!(
        count = result.len(),
        tables = ?result,
        "Returning available ontology tables (enabled + existing)"
    );

    Ok(result)
}

/// Ontology overview information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyOverview {
    pub name: String,
    pub domain: String,
    pub record_count: i64,
    pub sample_record: Option<serde_json::Value>,
}

/// Get overview of all available ontologies with counts and sample records
pub async fn get_ontologies_overview(db: &SqlitePool) -> Result<Vec<OntologyOverview>> {
    let available_tables = list_available_ontologies(db).await?;
    let mut overviews = Vec::new();

    for table_name in available_tables {
        // Extract domain from table name (e.g., "health_heart_rate" -> "Health")
        let domain = extract_domain(&table_name);

        // Get record count
        let count_query = format!("SELECT COUNT(*) as count FROM data_{}", table_name);
        let count_result = sqlx::query_scalar::<_, i64>(&count_query)
            .fetch_one(db)
            .await;

        let record_count = match count_result {
            Ok(count) => count,
            Err(e) => {
                tracing::warn!(
                    table = %table_name,
                    error = %e,
                    "Failed to get record count for ontology table"
                );
                0
            }
        };

        // Get one random sample record if records exist
        let sample_record = if record_count > 0 {
            let sample_query = format!(
                "SELECT row_to_json(t) as record FROM (SELECT * FROM data_{} ORDER BY RANDOM() LIMIT 1) t",
                table_name
            );

            match sqlx::query_scalar::<_, serde_json::Value>(&sample_query)
                .fetch_one(db)
                .await
            {
                Ok(record) => Some(record),
                Err(e) => {
                    tracing::warn!(
                        table = %table_name,
                        error = %e,
                        "Failed to fetch sample record for ontology table"
                    );
                    None
                }
            }
        } else {
            None
        };

        overviews.push(OntologyOverview {
            name: table_name,
            domain,
            record_count,
            sample_record,
        });
    }

    // Sort by domain, then by name
    overviews.sort_by(|a, b| a.domain.cmp(&b.domain).then_with(|| a.name.cmp(&b.name)));

    Ok(overviews)
}

/// Column schema information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub is_nullable: bool,
}

/// Response for ontology data queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyDataResponse {
    pub table_name: String,
    pub display_name: String,
    pub domain: String,
    pub columns: Vec<ColumnInfo>,
    pub key_columns: Vec<String>,
    pub timestamp_column: String,
    pub rows: Vec<serde_json::Value>,
    pub total_count: i64,
    pub limit: u32,
    pub offset: u32,
}

/// Query parameters for ontology data endpoint
#[derive(Debug, Deserialize)]
pub struct OntologyDataQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub sort: Option<String>,
    pub dir: Option<String>,
    pub date: Option<String>,
    pub search: Option<String>,
}

/// Infrastructure columns hidden from data responses
const HIDDEN_COLUMNS: &[&str] = &[
    "source_connection_id",
    "source_stream_id",
    "source_table",
    "source_provider",
    "deleted_at_source",
    "is_archived",
    "metadata",
    "created_at",
    "updated_at",
];

/// Query data from a specific ontology table with optional filtering
pub async fn query_ontology_data(
    db: &SqlitePool,
    table_name: &str,
    params: &OntologyDataQuery,
) -> Result<OntologyDataResponse> {
    // Validate table_name against registered ontologies
    let ontology = registered_ontologies()
        .into_iter()
        .find(|o| o.name == table_name || o.table_name == table_name)
        .ok_or_else(|| Error::NotFound(format!("Unknown ontology: {}", table_name)))?;

    let full_table_name = ontology.table_name;

    // Get column schema via PRAGMA
    let pragma_query = format!("PRAGMA table_info({})", full_table_name);
    let pragma_rows = sqlx::query(&pragma_query).fetch_all(db).await?;

    let all_columns: Vec<ColumnInfo> = pragma_rows
        .iter()
        .map(|row| {
            let name: String = row.get("name");
            let data_type: String = row.get("type");
            let notnull: i32 = row.get("notnull");
            ColumnInfo {
                name,
                data_type,
                is_nullable: notnull == 0,
            }
        })
        .collect();

    // Filter out hidden infrastructure columns for the response
    let visible_columns: Vec<ColumnInfo> = all_columns
        .iter()
        .filter(|c| !HIDDEN_COLUMNS.contains(&c.name.as_str()))
        .cloned()
        .collect();

    // Build visible column names for SELECT (include id for frontend keying)
    let select_cols: Vec<&str> = all_columns
        .iter()
        .filter(|c| !HIDDEN_COLUMNS.contains(&c.name.as_str()))
        .map(|c| c.name.as_str())
        .collect();

    let select_clause = select_cols.join(", ");

    // Get key_columns from table metadata
    let key_columns: Vec<String> = crate::tools::sql_query::get_table_metadata_for(full_table_name)
        .map(|m| m.key_columns.iter().map(|s| s.to_string()).collect())
        .unwrap_or_default();

    // Build WHERE clause
    let mut where_clauses = Vec::new();
    let mut bind_values: Vec<String> = Vec::new();

    // Date filter
    if let Some(ref date) = params.date {
        let ts_col = ontology.timestamp_column;
        if let Some(end_ts) = ontology.end_timestamp_column {
            // Span events: start on or before end of day, end on or after start of day
            where_clauses.push(format!(
                "date({}) <= ?{} AND date({}) >= ?{}",
                ts_col,
                bind_values.len() + 1,
                end_ts,
                bind_values.len() + 2,
            ));
            bind_values.push(date.clone());
            bind_values.push(date.clone());
        } else {
            where_clauses.push(format!(
                "date({}) = ?{}",
                ts_col,
                bind_values.len() + 1
            ));
            bind_values.push(date.clone());
        }
    }

    // Search filter (LIKE across text key_columns)
    if let Some(ref search) = params.search {
        if !search.trim().is_empty() {
            let search_cols: Vec<&str> = key_columns
                .iter()
                .filter(|kc| {
                    visible_columns
                        .iter()
                        .any(|vc| &vc.name == *kc && vc.data_type.to_uppercase().contains("TEXT"))
                })
                .map(|s| s.as_str())
                .collect();

            if !search_cols.is_empty() {
                let like_clauses: Vec<String> = search_cols
                    .iter()
                    .map(|col| {
                        bind_values.push(format!("%{}%", search.trim()));
                        format!("{} LIKE ?{}", col, bind_values.len())
                    })
                    .collect();
                where_clauses.push(format!("({})", like_clauses.join(" OR ")));
            }
        }
    }

    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", where_clauses.join(" AND "))
    };

    // Sort
    let sort_col = params
        .sort
        .as_deref()
        .filter(|s| select_cols.contains(s))
        .unwrap_or(ontology.timestamp_column);
    let sort_dir = params
        .dir
        .as_deref()
        .filter(|d| *d == "asc" || *d == "desc")
        .unwrap_or("desc");

    // Pagination
    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);

    // Count query
    let count_sql = format!("SELECT COUNT(*) FROM {}{}", full_table_name, where_sql);
    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
    for val in &bind_values {
        count_query = count_query.bind(val);
    }
    let total_count = count_query.fetch_one(db).await.unwrap_or(0);

    // Data query
    let data_sql = format!(
        "SELECT {} FROM {}{} ORDER BY {} {} LIMIT {} OFFSET {}",
        select_clause, full_table_name, where_sql, sort_col, sort_dir, limit, offset
    );
    let mut data_query = sqlx::query(&data_sql);
    for val in &bind_values {
        data_query = data_query.bind(val);
    }
    let rows = data_query.fetch_all(db).await?;
    let json_rows = convert_rows_to_json(&rows);

    Ok(OntologyDataResponse {
        table_name: ontology.name.to_string(),
        display_name: ontology.display_name.to_string(),
        domain: ontology.domain.to_string(),
        columns: visible_columns,
        key_columns,
        timestamp_column: ontology.timestamp_column.to_string(),
        rows: json_rows,
        total_count,
        limit,
        offset,
    })
}

/// Extract domain name from table name
fn extract_domain(table_name: &str) -> String {
    let parts: Vec<&str> = table_name.split('_').collect();
    if parts.is_empty() {
        return "Unknown".to_string();
    }

    // Capitalize first letter
    let domain = parts[0];
    let mut chars = domain.chars();
    match chars.next() {
        None => "Unknown".to_string(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_has_enabled_streams() {
        // Verify that the main ENABLED streams exist in the registry
        // Note: Some streams (e.g., gmail) may be disabled for certification reasons
        let known_enabled_streams = vec![
            "stream_google_calendar",
            "stream_ios_microphone",
        ];

        for stream_table in known_enabled_streams {
            let result = registry::get_stream_by_table_name(stream_table);
            assert!(
                result.is_some(),
                "Expected enabled stream {} in registry, but not found",
                stream_table
            );

            // Verify target_ontologies is populated
            if let Some((_, stream)) = result {
                assert!(
                    !stream.descriptor.target_ontologies.is_empty(),
                    "Expected target_ontologies for {}, but was empty",
                    stream_table
                );
            }
        }
    }

    #[test]
    fn test_registry_has_disabled_streams() {
        // Verify that disabled streams exist when including disabled
        // This ensures transform logic is available even for disabled streams
        let known_disabled_streams = vec![
            "stream_google_gmail",
            "stream_notion_pages",
        ];

        for stream_table in known_disabled_streams {
            let result = registry::get_stream_by_table_name_including_disabled(stream_table);
            assert!(
                result.is_some(),
                "Expected stream {} in registry (including disabled), but not found",
                stream_table
            );

            // Verify the stream has transforms registered (unified registry)
            if let Some((_, stream)) = result {
                // Gmail and similar should have transforms even when disabled
                if stream.descriptor.target_ontologies.len() > 0 {
                    assert!(
                        !stream.transforms.is_empty() || stream.descriptor.table_name.starts_with("stream_notion"),
                        "Expected transforms for {}, but was empty",
                        stream_table
                    );
                }
            }
        }
    }
}

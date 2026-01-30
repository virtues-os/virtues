//! SQL Query tool implementation
//!
//! Provides read-only SQL access to user's personal data tables.

use serde::{Deserialize, Serialize};
use sqlx::{Column, Row, SqlitePool, TypeInfo};
use std::collections::HashMap;
use std::sync::Arc;

use super::executor::{ToolError, ToolResult};

/// Table metadata for get_schema operation
#[derive(Debug, Clone, Serialize)]
pub struct TableMetadata {
    pub description: &'static str,
    pub category: &'static str,
    pub key_columns: &'static [&'static str],
    pub join_hint: Option<&'static str>,
}

/// Static table metadata - descriptions and key queryable columns
fn get_table_metadata() -> HashMap<&'static str, TableMetadata> {
    let mut m = HashMap::new();

    // ============================================================================
    // DATA TABLES - Health
    // ============================================================================
    m.insert("data_health_heart_rate", TableMetadata {
        description: "Heart rate BPM measurements from wearables",
        category: "health",
        key_columns: &["bpm", "timestamp"],
        join_hint: None,
    });
    m.insert("data_health_hrv", TableMetadata {
        description: "Heart rate variability measurements in milliseconds",
        category: "health",
        key_columns: &["hrv_ms", "timestamp"],
        join_hint: None,
    });
    m.insert("data_health_steps", TableMetadata {
        description: "Step count records (may have multiple per day)",
        category: "health",
        key_columns: &["step_count", "timestamp"],
        join_hint: None,
    });
    m.insert("data_health_sleep", TableMetadata {
        description: "Sleep sessions with duration and quality metrics",
        category: "health",
        key_columns: &["start_time", "end_time", "duration_minutes", "sleep_quality_score", "sleep_stages"],
        join_hint: None,
    });
    m.insert("data_health_workout", TableMetadata {
        description: "Exercise and workout sessions",
        category: "health",
        key_columns: &["workout_type", "start_time", "end_time", "duration_minutes", "calories_burned", "distance_km", "avg_heart_rate", "max_heart_rate"],
        join_hint: Some("JOIN wiki_places ON place_id = wiki_places.id"),
    });

    // ============================================================================
    // DATA TABLES - Location
    // ============================================================================
    m.insert("data_location_point", TableMetadata {
        description: "Raw GPS coordinates (high volume, use sparingly)",
        category: "location",
        key_columns: &["latitude", "longitude", "altitude", "horizontal_accuracy", "timestamp"],
        join_hint: None,
    });
    m.insert("data_location_visit", TableMetadata {
        description: "Place visits with arrival/departure times",
        category: "location",
        key_columns: &["place_name", "latitude", "longitude", "arrival_time", "departure_time", "duration_minutes"],
        join_hint: Some("JOIN wiki_places ON place_id = wiki_places.id"),
    });

    // ============================================================================
    // DATA TABLES - Social
    // ============================================================================
    m.insert("data_social_email", TableMetadata {
        description: "Email messages from Gmail, etc.",
        category: "social",
        key_columns: &["subject", "body", "body_preview", "from_email", "from_name", "to_emails", "direction", "is_read", "is_starred", "has_attachments", "labels", "thread_id", "timestamp"],
        join_hint: Some("JOIN wiki_people ON from_person_id = wiki_people.id"),
    });
    m.insert("data_social_message", TableMetadata {
        description: "Chat messages (iMessage, SMS, etc.)",
        category: "social",
        key_columns: &["body", "channel", "from_identifier", "from_name", "to_identifiers", "is_read", "is_group_message", "has_attachments", "thread_id", "timestamp"],
        join_hint: Some("JOIN wiki_people ON from_person_id = wiki_people.id"),
    });

    // ============================================================================
    // DATA TABLES - Calendar
    // ============================================================================
    m.insert("data_calendar", TableMetadata {
        description: "Calendar events with attendees and location",
        category: "calendar",
        key_columns: &["title", "description", "calendar_name", "event_type", "status", "response_status", "organizer_identifier", "attendee_identifiers", "location_name", "conference_url", "start_time", "end_time", "is_all_day", "timezone"],
        join_hint: Some("JOIN wiki_places ON place_id = wiki_places.id"),
    });

    // ============================================================================
    // DATA TABLES - Financial (amounts in cents)
    // ============================================================================
    m.insert("data_financial_account", TableMetadata {
        description: "Bank, credit, and investment accounts",
        category: "financial",
        key_columns: &["account_name", "account_type", "institution_name", "mask", "currency", "current_balance", "available_balance", "credit_limit", "is_active"],
        join_hint: None,
    });
    m.insert("data_financial_transaction", TableMetadata {
        description: "Transactions (amounts in cents, negative=debit)",
        category: "financial",
        key_columns: &["account_id", "amount", "currency", "merchant_name", "merchant_category", "description", "category", "is_pending", "transaction_type", "payment_channel", "timestamp"],
        join_hint: Some("JOIN data_financial_account ON account_id = data_financial_account.id"),
    });
    m.insert("data_financial_asset", TableMetadata {
        description: "Investment holdings (stocks, crypto, etc.)",
        category: "financial",
        key_columns: &["account_id", "asset_type", "symbol", "name", "quantity", "cost_basis", "current_value", "currency", "timestamp"],
        join_hint: Some("JOIN data_financial_account ON account_id = data_financial_account.id"),
    });
    m.insert("data_financial_liability", TableMetadata {
        description: "Loans, mortgages, and debt",
        category: "financial",
        key_columns: &["account_id", "liability_type", "principal", "interest_rate", "minimum_payment", "next_payment_due_date", "currency", "timestamp"],
        join_hint: Some("JOIN data_financial_account ON account_id = data_financial_account.id"),
    });

    // ============================================================================
    // DATA TABLES - Activity
    // ============================================================================
    m.insert("data_activity_app_usage", TableMetadata {
        description: "Desktop/mobile app usage sessions",
        category: "activity",
        key_columns: &["app_name", "app_bundle_id", "app_category", "start_time", "end_time", "window_title", "url"],
        join_hint: None,
    });
    m.insert("data_activity_web_browsing", TableMetadata {
        description: "Web browsing history",
        category: "activity",
        key_columns: &["url", "domain", "page_title", "visit_duration_seconds", "timestamp"],
        join_hint: None,
    });

    // ============================================================================
    // DATA TABLES - Knowledge
    // ============================================================================
    m.insert("data_knowledge_document", TableMetadata {
        description: "Saved documents and notes",
        category: "knowledge",
        key_columns: &["title", "content", "content_summary", "document_type", "tags", "is_authored", "created_time", "last_modified_time"],
        join_hint: None,
    });
    m.insert("data_knowledge_ai_conversation", TableMetadata {
        description: "Past AI chat conversation history",
        category: "knowledge",
        key_columns: &["conversation_id", "message_id", "role", "content", "model", "provider", "timestamp"],
        join_hint: None,
    });

    // ============================================================================
    // DATA TABLES - Other
    // ============================================================================
    m.insert("data_speech_transcription", TableMetadata {
        description: "Voice/audio transcriptions",
        category: "speech",
        key_columns: &["text", "language", "duration_seconds", "start_time", "end_time", "speaker_count"],
        join_hint: None,
    });
    m.insert("data_device_battery", TableMetadata {
        description: "Device battery level snapshots",
        category: "device",
        key_columns: &["battery_level", "battery_state", "is_low_power_mode", "timestamp"],
        join_hint: None,
    });
    m.insert("data_environment_pressure", TableMetadata {
        description: "Barometric pressure readings",
        category: "environment",
        key_columns: &["pressure_hpa", "relative_altitude_change", "timestamp"],
        join_hint: None,
    });

    // ============================================================================
    // WIKI TABLES - Entities (resolved nouns)
    // ============================================================================
    m.insert("wiki_people", TableMetadata {
        description: "Resolved people in user's life",
        category: "wiki_entity",
        key_columns: &["canonical_name", "emails", "phones", "relationship_category", "nickname", "notes", "first_interaction", "last_interaction", "interaction_count", "birthday"],
        join_hint: None,
    });
    m.insert("wiki_places", TableMetadata {
        description: "Resolved places in user's life",
        category: "wiki_entity",
        key_columns: &["name", "category", "address", "latitude", "longitude", "radius_m", "visit_count", "first_visit", "last_visit"],
        join_hint: None,
    });
    m.insert("wiki_orgs", TableMetadata {
        description: "Organizations in user's life",
        category: "wiki_entity",
        key_columns: &["canonical_name", "organization_type", "relationship_type", "role_title", "start_date", "end_date", "interaction_count", "first_interaction", "last_interaction"],
        join_hint: Some("JOIN wiki_places ON primary_place_id = wiki_places.id"),
    });
    m.insert("wiki_connections", TableMetadata {
        description: "Relationships between people/places/orgs",
        category: "wiki_entity",
        key_columns: &["source_type", "source_id", "target_type", "target_id", "relationship", "strength", "first_seen", "last_seen"],
        join_hint: None,
    });

    // ============================================================================
    // WIKI TABLES - Temporal
    // ============================================================================
    m.insert("wiki_days", TableMetadata {
        description: "Day summaries with autobiography and context",
        category: "wiki_temporal",
        key_columns: &["date", "start_timezone", "end_timezone", "autobiography", "last_edited_by", "context_vector"],
        join_hint: Some("JOIN narrative_acts ON act_id = narrative_acts.id"),
    });
    m.insert("wiki_years", TableMetadata {
        description: "Year summaries with highlights and themes",
        category: "wiki_temporal",
        key_columns: &["year", "summary", "highlights", "themes", "content"],
        join_hint: None,
    });
    m.insert("wiki_events", TableMetadata {
        description: "Timeline events within a day",
        category: "wiki_temporal",
        key_columns: &["day_id", "start_time", "end_time", "auto_label", "auto_location", "user_label", "user_location", "user_notes", "is_unknown", "is_transit"],
        join_hint: Some("JOIN wiki_days ON day_id = wiki_days.id"),
    });

    // ============================================================================
    // WIKI TABLES - References
    // ============================================================================
    m.insert("wiki_citations", TableMetadata {
        description: "Links wiki content to source ontology records",
        category: "wiki_reference",
        key_columns: &["source_type", "source_id", "target_table", "target_id", "citation_index", "label", "preview", "added_by"],
        join_hint: None,
    });

    // ============================================================================
    // NARRATIVE TABLES - Life story structure
    // ============================================================================
    m.insert("narrative_telos", TableMetadata {
        description: "User's life purpose/direction",
        category: "narrative",
        key_columns: &["title", "description", "is_active", "content"],
        join_hint: None,
    });
    m.insert("narrative_acts", TableMetadata {
        description: "Major life periods (multi-year)",
        category: "narrative",
        key_columns: &["title", "subtitle", "description", "start_date", "end_date", "sort_order", "themes", "location", "content"],
        join_hint: Some("JOIN narrative_telos ON telos_id = narrative_telos.id"),
    });
    m.insert("narrative_chapters", TableMetadata {
        description: "Chapters within acts (months/seasons)",
        category: "narrative",
        key_columns: &["act_id", "title", "subtitle", "description", "start_date", "end_date", "sort_order", "themes", "content"],
        join_hint: Some("JOIN narrative_acts ON act_id = narrative_acts.id"),
    });

    m
}

/// SQL query tool arguments (from LLM)
#[derive(Debug, Deserialize)]
pub struct SqlQueryArgs {
    /// Operation to perform: query, list_tables, get_schema
    pub operation: String,
    /// SQL query (for "query" operation)
    #[serde(default)]
    pub sql: Option<String>,
    /// Table names (for "get_schema" operation)
    #[serde(default)]
    pub tables: Option<Vec<String>>,
    /// Max rows to return (default 50, max 200)
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Column information
#[derive(Debug, Serialize)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub is_nullable: bool,
}

/// SQL Query tool
#[derive(Clone)]
pub struct SqlQueryTool {
    pool: Arc<SqlitePool>,
}

impl SqlQueryTool {
    pub fn new(pool: Arc<SqlitePool>) -> Self {
        Self { pool }
    }

    /// Execute SQL query tool
    pub async fn execute(&self, arguments: serde_json::Value) -> Result<ToolResult, ToolError> {
        let args: SqlQueryArgs = serde_json::from_value(arguments)
            .map_err(|e| ToolError::InvalidParameters(format!("Invalid arguments: {}", e)))?;

        match args.operation.as_str() {
            "list_tables" => self.list_tables().await,
            "get_schema" => {
                let tables = args.tables.ok_or_else(|| {
                    ToolError::InvalidParameters("'tables' array is required for get_schema operation".into())
                })?;
                if tables.is_empty() {
                    return Err(ToolError::InvalidParameters("'tables' array cannot be empty".into()));
                }
                self.get_schema(&tables).await
            }
            "query" => {
                let sql = args.sql.ok_or_else(|| {
                    ToolError::InvalidParameters("'sql' is required for query operation".into())
                })?;
                let limit = args.limit.unwrap_or(50).min(200);
                self.execute_query(&sql, limit).await
            }
            _ => Err(ToolError::InvalidParameters(format!(
                "Unknown operation: '{}'. Use: query, list_tables, get_schema",
                args.operation
            ))),
        }
    }

    /// List all queryable tables (data_*, wiki_*, narrative_*)
    async fn list_tables(&self) -> Result<ToolResult, ToolError> {
        // Get all queryable tables: data_*, wiki_*, narrative_*
        let rows = sqlx::query(
            r#"
            SELECT name FROM sqlite_master 
            WHERE type='table' AND (
                name LIKE 'data_%' 
                OR name LIKE 'wiki_%' 
                OR name LIKE 'narrative_%'
            )
            ORDER BY 
                CASE 
                    WHEN name LIKE 'data_%' THEN 1
                    WHEN name LIKE 'wiki_%' THEN 2
                    WHEN name LIKE 'narrative_%' THEN 3
                END,
                name
            "#,
        )
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to list tables: {}", e)))?;

        let metadata = get_table_metadata();
        let mut tables = Vec::new();
        
        for row in rows {
            let table_name: String = row.get("name");
            
            // Get row count
            let count_query = format!("SELECT COUNT(*) as cnt FROM \"{}\"", table_name);
            let count_row = sqlx::query(&count_query)
                .fetch_optional(self.pool.as_ref())
                .await
                .ok()
                .flatten();
            
            let row_count: i64 = count_row
                .map(|r| r.get::<i64, _>("cnt"))
                .unwrap_or(0);

            // Get description from metadata
            let description = metadata
                .get(table_name.as_str())
                .map(|m| m.description)
                .unwrap_or("");

            tables.push(serde_json::json!({
                "name": table_name,
                "row_count": row_count,
                "description": description,
            }));
        }

        Ok(ToolResult::success(serde_json::json!({
            "operation": "list_tables",
            "tables": tables,
            "count": tables.len(),
        })))
    }

    /// Get schema for one or more tables
    async fn get_schema(&self, tables: &[String]) -> Result<ToolResult, ToolError> {
        let metadata = get_table_metadata();
        let mut result_tables = serde_json::Map::new();

        for table in tables {
            // Validate table name (must be a known queryable table)
            let is_valid = table.starts_with("data_") 
                || table.starts_with("wiki_") 
                || table.starts_with("narrative_");
            
            if !is_valid {
                return Err(ToolError::InvalidParameters(format!(
                    "Can only get schema for data_*, wiki_*, or narrative_* tables. Got: '{}'",
                    table
                )));
            }

            // Prevent SQL injection by validating table name
            if !table.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return Err(ToolError::InvalidParameters(format!(
                    "Invalid table name: '{}'",
                    table
                )));
            }

            // Get table schema using PRAGMA
            let pragma_query = format!("PRAGMA table_info(\"{}\")", table);
            let rows = sqlx::query(&pragma_query)
                .fetch_all(self.pool.as_ref())
                .await
                .map_err(|e| ToolError::ExecutionFailed(format!("Failed to get table info: {}", e)))?;

            if rows.is_empty() {
                return Err(ToolError::ExecutionFailed(format!(
                    "Table '{}' not found",
                    table
                )));
            }

            let columns: Vec<ColumnInfo> = rows
                .iter()
                .map(|row| {
                    let notnull: i32 = row.get("notnull");
                    ColumnInfo {
                        name: row.get("name"),
                        data_type: row.get("type"),
                        is_nullable: notnull == 0,
                    }
                })
                .collect();

            // Get row count
            let count_query = format!("SELECT COUNT(*) as cnt FROM \"{}\"", table);
            let count_row = sqlx::query(&count_query)
                .fetch_one(self.pool.as_ref())
                .await
                .ok();
            let row_count: i64 = count_row.map(|r| r.get("cnt")).unwrap_or(0);

            // Get metadata if available
            let table_meta = metadata.get(table.as_str());
            let description = table_meta.map(|m| m.description).unwrap_or("");
            let key_columns: Vec<&str> = table_meta
                .map(|m| m.key_columns.to_vec())
                .unwrap_or_default();
            let join_hint = table_meta.and_then(|m| m.join_hint);

            let mut table_info = serde_json::json!({
                "description": description,
                "columns": columns,
                "row_count": row_count,
                "key_columns": key_columns,
            });

            if let Some(hint) = join_hint {
                table_info["join_hint"] = serde_json::json!(hint);
            }

            result_tables.insert(table.clone(), table_info);
        }

        Ok(ToolResult::success(serde_json::json!({
            "operation": "get_schema",
            "tables": result_tables,
        })))
    }

    /// Execute a read-only SQL query
    async fn execute_query(&self, sql: &str, limit: u32) -> Result<ToolResult, ToolError> {
        // Validate query is read-only
        let sql_lower = sql.trim().to_lowercase();

        if !sql_lower.starts_with("select") && !sql_lower.starts_with("with") {
            return Err(ToolError::InvalidParameters(
                "Only SELECT queries are allowed".into(),
            ));
        }

        // Check for dangerous keywords
        let forbidden = ["insert", "update", "delete", "drop", "create", "alter", "truncate"];
        for keyword in &forbidden {
            if sql_lower.contains(keyword) {
                return Err(ToolError::InvalidParameters(format!(
                    "Query contains forbidden keyword: {}",
                    keyword
                )));
            }
        }

        // Validate query length
        if sql.len() > 5000 {
            return Err(ToolError::InvalidParameters(
                "Query too long (max 5000 characters)".into(),
            ));
        }

        // Apply limit if not already present
        let query = if sql_lower.contains("limit") {
            sql.to_string()
        } else {
            format!("{} LIMIT {}", sql, limit)
        };

        // Execute query
        let rows = sqlx::query(&query)
            .fetch_all(self.pool.as_ref())
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Query failed: {}", e)))?;

        // Convert rows to JSON
        let json_rows = convert_rows_to_json(&rows);

        Ok(ToolResult::success(serde_json::json!({
            "operation": "query",
            "row_count": json_rows.len(),
            "rows": json_rows,
        })))
    }
}

/// Convert SQLite rows to JSON array
fn convert_rows_to_json(rows: &[sqlx::sqlite::SqliteRow]) -> Vec<serde_json::Value> {
    let mut json_rows = Vec::new();

    for row in rows {
        let mut obj = serde_json::Map::new();

        for (i, col) in row.columns().iter().enumerate() {
            let col_name = col.name();

            let value: serde_json::Value = 
                // Try NULL first
                if let Ok(opt_val) = row.try_get::<Option<String>, _>(i) {
                    match opt_val {
                        Some(v) => serde_json::Value::String(v),
                        None => serde_json::Value::Null,
                    }
                }
                // Integer types
                else if let Ok(v) = row.try_get::<i64, _>(i) {
                    serde_json::Value::Number(v.into())
                } else if let Ok(v) = row.try_get::<i32, _>(i) {
                    serde_json::Value::Number(v.into())
                }
                // Float types
                else if let Ok(v) = row.try_get::<f64, _>(i) {
                    serde_json::json!(v)
                }
                // Boolean
                else if let Ok(v) = row.try_get::<bool, _>(i) {
                    serde_json::Value::Bool(v)
                }
                // JSON
                else if let Ok(v) = row.try_get::<serde_json::Value, _>(i) {
                    v
                }
                // Fallback
                else {
                    let type_info = col.type_info().name();
                    serde_json::Value::String(format!("<{}>", type_info))
                };

            obj.insert(col_name.to_string(), value);
        }

        json_rows.push(serde_json::Value::Object(obj));
    }

    json_rows
}

impl std::fmt::Debug for SqlQueryTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqlQueryTool").finish()
    }
}

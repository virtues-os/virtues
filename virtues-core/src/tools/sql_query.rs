//! SQL Query tool implementation
//!
//! Provides read-only SQL access to user's personal data tables.

use serde::{Deserialize, Serialize};
use sqlx::{Column, Row, PgPool, TypeInfo};
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
        join_hint: Some("JOIN wiki_entity_refs er ON er.source_table = 'data_health_workout' AND er.source_id = data_health_workout.id JOIN wiki_places ON er.entity_id = wiki_places.id AND er.entity_type = 'place'"),
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
        join_hint: Some("JOIN wiki_entity_refs er ON er.source_table = 'data_location_visit' AND er.source_id = data_location_visit.id JOIN wiki_places ON er.entity_id = wiki_places.id AND er.entity_type = 'place'"),
    });

    // ============================================================================
    // DATA TABLES - Communication
    // ============================================================================
    m.insert("data_communication_email", TableMetadata {
        description: "Email messages from Gmail, etc.",
        category: "communication",
        key_columns: &["subject", "body", "body_preview", "from_email", "from_name", "to_emails", "direction", "is_read", "is_starred", "has_attachments", "labels", "thread_id", "timestamp"],
        join_hint: Some("JOIN wiki_entity_refs er ON er.source_table = 'data_communication_email' AND er.source_id = data_communication_email.id JOIN wiki_people ON er.entity_id = wiki_people.id AND er.entity_type = 'person'"),
    });
    m.insert("data_communication_message", TableMetadata {
        description: "Chat messages (iMessage, SMS, etc.)",
        category: "communication",
        key_columns: &["body", "channel", "from_identifier", "from_name", "to_identifiers", "is_read", "is_group_message", "has_attachments", "thread_id", "timestamp"],
        // A message links to the person on the other end via wiki_entity_refs:
        // role='sender' for messages you received, role='recipient' for messages you
        // sent. Filter both to get a full thread with someone; the message's own
        // direction is in metadata->>'is_from_me'.
        join_hint: Some("JOIN wiki_entity_refs er ON er.source_table = 'data_communication_message' AND er.source_id = data_communication_message.id AND er.entity_type = 'person' AND er.role IN ('sender','recipient') JOIN wiki_people ON er.entity_id = wiki_people.id"),
    });

    // ============================================================================
    // DATA TABLES - Calendar
    // ============================================================================
    m.insert("data_calendar_event", TableMetadata {
        description: "Calendar events with attendees and location",
        category: "calendar",
        key_columns: &["title", "description", "calendar_name", "event_type", "status", "response_status", "organizer_identifier", "attendee_identifiers", "location_name", "conference_url", "start_time", "end_time", "is_all_day", "timezone"],
        join_hint: Some("JOIN wiki_entity_refs er ON er.source_table = 'data_calendar_event' AND er.source_id = data_calendar_event.id"),
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
    m.insert("data_activity_app_session", TableMetadata {
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
    // DATA TABLES - Content
    // ============================================================================
    m.insert("data_content_document", TableMetadata {
        description: "Saved documents and notes",
        category: "content",
        key_columns: &["title", "content", "content_summary", "document_type", "tags", "is_authored", "created_time", "last_modified_time"],
        join_hint: None,
    });
    m.insert("data_content_conversation", TableMetadata {
        description: "Past AI chat conversation history",
        category: "content",
        key_columns: &["conversation_id", "message_id", "role", "content", "model", "provider", "timestamp"],
        join_hint: None,
    });
    m.insert("data_content_bookmark", TableMetadata {
        description: "Saved/starred content (GitHub stars, browser bookmarks, etc.)",
        category: "content",
        key_columns: &["url", "title", "description", "source_platform", "bookmark_type", "content_type", "author", "tags", "timestamp"],
        join_hint: None,
    });

    // ============================================================================
    // DATA TABLES - Other
    // ============================================================================
    m.insert("data_communication_transcription", TableMetadata {
        description: "Voice/audio transcriptions",
        category: "communication",
        key_columns: &["text", "language", "duration_seconds", "start_time", "end_time", "speaker_count"],
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
        join_hint: None,
    });

    // ============================================================================
    // WIKI TABLES - Temporal
    // ============================================================================
    m.insert("wiki_days", TableMetadata {
        description: "Day summaries with autobiography and context",
        category: "wiki_temporal",
        key_columns: &["date", "start_timezone", "autobiography", "last_edited_by"],
        join_hint: Some("JOIN wiki_acts ON act_id = wiki_acts.id"),
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
    m.insert("wiki_entity_refs", TableMetadata {
        description: "Junction table linking entities (people, places, orgs) to ontology records. Use for 'everything about entity X' queries.",
        category: "wiki_reference",
        key_columns: &["entity_type", "entity_id", "source_table", "source_id", "role", "confidence", "resolved_by", "timestamp"],
        join_hint: None,
    });

    // ============================================================================
    // WIKI TABLES - Narrative (life story structure)
    // ============================================================================
    m.insert("wiki_telos", TableMetadata {
        description: "User's life purpose/direction",
        category: "wiki_narrative",
        key_columns: &["title", "description", "is_active", "content"],
        join_hint: None,
    });
    m.insert("wiki_acts", TableMetadata {
        description: "Major life periods (multi-year)",
        category: "wiki_narrative",
        key_columns: &["title", "subtitle", "description", "start_date", "end_date", "sort_order", "themes", "location", "content"],
        join_hint: Some("JOIN wiki_telos ON telos_id = wiki_telos.id"),
    });
    m.insert("wiki_chapters", TableMetadata {
        description: "Chapters within acts (months/seasons)",
        category: "wiki_narrative",
        key_columns: &["act_id", "title", "subtitle", "description", "start_date", "end_date", "sort_order", "themes", "content"],
        join_hint: Some("JOIN wiki_acts ON act_id = wiki_acts.id"),
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
    pool: Arc<PgPool>,
}

impl SqlQueryTool {
    pub fn new(pool: Arc<PgPool>) -> Self {
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

    /// List all queryable tables (data_*, wiki_*)
    async fn list_tables(&self) -> Result<ToolResult, ToolError> {
        // Get all queryable tables: data_*, wiki_*
        let rows = sqlx::query(
            r#"
            SELECT tablename AS name FROM pg_tables
            WHERE schemaname = 'public' AND (
                tablename LIKE 'data_%'
                OR tablename LIKE 'wiki_%'
            )
            ORDER BY
                CASE
                    WHEN tablename LIKE 'data_%' THEN 1
                    WHEN tablename LIKE 'wiki_%' THEN 2
                END,
                tablename
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
                ;
            
            if !is_valid {
                return Err(ToolError::InvalidParameters(format!(
                    "Can only get schema for data_* or wiki_* tables. Got: '{}'",
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

            // Postgres catalog equivalent of PRAGMA table_info — pull from
            // information_schema. Bound parameter for the table name.
            let rows = sqlx::query(
                "SELECT column_name AS name, data_type AS \"type\", \
                 is_nullable AS nullable \
                 FROM information_schema.columns \
                 WHERE table_schema = 'public' AND table_name = $1 \
                 ORDER BY ordinal_position",
            )
            .bind(table)
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
                    let nullable: String = row.get("nullable");
                    ColumnInfo {
                        name: row.get("name"),
                        data_type: row.get("type"),
                        is_nullable: nullable == "YES",
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

    /// Execute a read-only SQL query.
    ///
    /// Read-only is enforced by Postgres itself (`SET TRANSACTION READ ONLY`),
    /// not by inspecting the query text. The keyword checks below are kept as a
    /// fast, legible rejection for obvious misuse — but they are a courtesy, not
    /// the boundary. A text blocklist cannot be the boundary: `SELECT ... INTO
    /// new_table FROM ...` writes, starts with `select`, and contains none of
    /// the forbidden tokens. This matters more here than in the human-facing
    /// Developer console (`api::developer::execute_sql`, which uses the same
    /// transaction guard) because `sql_query` is in APPLET_RUN_ALLOWED_TOOLS
    /// and SUBAGENT_TOOLS — it runs unattended, over content nobody reviewed,
    /// so the query text can be steered by ingested data.
    async fn execute_query(&self, sql: &str, limit: u32) -> Result<ToolResult, ToolError> {
        let sql_lower = sql.trim().to_lowercase();

        if !sql_lower.starts_with("select") && !sql_lower.starts_with("with") {
            return Err(ToolError::InvalidParameters(
                "Only SELECT queries are allowed".into(),
            ));
        }

        // Match whole tokens only — a substring check falsely rejects common
        // columns like `created_at` ("create") and `updated_at` ("update").
        // Splitting on non-alphanumeric chars (which includes `_`) tokenizes
        // `created_at` into ["created", "at"], so only a standalone
        // `create`/`update`/etc. statement keyword trips the guard.
        let forbidden = ["insert", "update", "delete", "drop", "create", "alter", "truncate"];
        if let Some(keyword) = sql_lower
            .split(|c: char| !c.is_alphanumeric())
            .find(|tok| forbidden.contains(tok))
        {
            return Err(ToolError::InvalidParameters(format!(
                "Query contains forbidden keyword: {}",
                keyword
            )));
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

        // The actual boundary: Postgres refuses any write in this transaction,
        // whatever the text says. Mirrors `api::developer::execute_sql` and
        // `server::faces::run_face_query`.
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Query failed: {}", e)))?;

        sqlx::query("SET TRANSACTION READ ONLY")
            .execute(&mut *tx)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Query failed: {}", e)))?;

        let rows = sqlx::query(&query)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Query failed: {}", e)))?;

        // Read-only transaction: nothing to commit, and a rollback can't lose
        // work. Dropping would do this anyway; explicit for legibility.
        let _ = tx.rollback().await;

        // Convert rows to JSON
        let mut json_rows = convert_rows_to_json(&rows);

        // Make the answer citable. The agent is told to cite a claim by linking
        // the `ref` a tool returned, and to cite nothing when a result has no
        // `ref` — so without this, every fact the model learned from SQL was
        // structurally uncitable, which is most of what it knows about the
        // owner's own data.
        attach_record_refs(&query, &mut json_rows);

        Ok(ToolResult::success(serde_json::json!({
            "operation": "query",
            "row_count": json_rows.len(),
            "rows": json_rows,
        })))
    }
}

/// Attach a citable `/record/{ontology}/{id}` to each row, matching the `ref`
/// that `semantic_search` already returns — the frontend renders that route as
/// a click-through to the record itself.
///
/// Deliberately conservative: only for a JOIN-free query naming exactly ONE
/// registered ontology table, on rows that actually carry an `id`. An
/// aggregate has no record to point at, and in a join the `id` column may
/// belong to either side — `SELECT d.id FROM wiki_events e JOIN wiki_days d`
/// would otherwise be labelled a wiki_event. Resolving that needs real alias
/// analysis, and a confident link to the wrong record is worse than no link:
/// the agent prompt tells the model to skip results without a `ref`, so
/// silence degrades to "uncited", never to "miscited".
fn attach_record_refs(query: &str, rows: &mut [serde_json::Value]) {
    use virtues_registry::ontologies::registered_ontologies;

    let lowered = query.to_lowercase();
    if mentions_table(&lowered, "join") {
        return;
    }
    // Ambiguity is about TABLES, not ontologies.
    //
    // Two ontologies can share one table — `app_page` and `wiki_article` both
    // sit on `app_pages`, split by `kind` (migration 0081) so the record's own
    // prose stays out of the day view. They point at the same rows, so a query
    // naming that table is not ambiguous about WHICH ROW to cite; only about
    // which label to hang on it.
    //
    // Counting ontologies instead of tables turned the split into a silent
    // regression: every SQL-tool result touching `app_pages` would lose its
    // `ref`, and because the agent prompt tells the model to skip uncited
    // results, they would quietly vanish from answers rather than error.
    let mut matched_table: Option<&'static str> = None;
    let mut matched: Option<&'static str> = None;
    for ont in registered_ontologies() {
        if !mentions_table(&lowered, ont.table_name) {
            continue;
        }
        match matched_table {
            // A genuinely different table — that is real ambiguity.
            Some(t) if t != ont.table_name => return,
            // Same table, second ontology: keep the first. `get_record`
            // resolves a bare table name the same way, so the ref still opens.
            Some(_) => continue,
            None => {
                matched_table = Some(ont.table_name);
                matched = Some(ont.name);
            }
        }
    }
    let Some(ontology) = matched else { return };

    for row in rows.iter_mut() {
        let Some(obj) = row.as_object_mut() else { continue };
        // Never shadow a real column the query happened to select.
        if obj.contains_key("ref") {
            continue;
        }
        let Some(id) = obj.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let route = format!("/record/{ontology}/{id}");
        obj.insert("ref".to_string(), serde_json::Value::String(route));
    }
}

/// Whole-word search for a table name, so `wiki_days` doesn't match inside a
/// longer identifier and a name inside a string literal doesn't count either.
fn mentions_table(lowered_query: &str, table: &str) -> bool {
    lowered_query
        .match_indices(table)
        .any(|(idx, _)| {
            let before_ok = idx == 0
                || !lowered_query[..idx]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_');
            let after = idx + table.len();
            let after_ok = after >= lowered_query.len()
                || !lowered_query[after..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_');
            before_ok && after_ok
        })
}

/// Convert Postgres rows to JSON array
pub fn convert_rows_to_json(rows: &[sqlx::postgres::PgRow]) -> Vec<serde_json::Value> {
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
                } else if let Ok(v) = row.try_get::<i16, _>(i) {
                    serde_json::Value::Number(v.into())
                }
                // Float types
                else if let Ok(v) = row.try_get::<f64, _>(i) {
                    serde_json::json!(v)
                } else if let Ok(v) = row.try_get::<f32, _>(i) {
                    serde_json::json!(v)
                }
                // Boolean
                else if let Ok(v) = row.try_get::<bool, _>(i) {
                    serde_json::Value::Bool(v)
                }
                // Date/time types → strings (RFC3339 for tz-aware)
                else if let Ok(v) = row.try_get::<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>, _>(i) {
                    serde_json::Value::String(v.to_rfc3339())
                } else if let Ok(v) = row.try_get::<sqlx::types::chrono::DateTime<sqlx::types::chrono::FixedOffset>, _>(i) {
                    serde_json::Value::String(v.to_rfc3339())
                } else if let Ok(v) = row.try_get::<sqlx::types::chrono::NaiveDateTime, _>(i) {
                    serde_json::Value::String(v.format("%Y-%m-%d %H:%M:%S%.f").to_string())
                } else if let Ok(v) = row.try_get::<sqlx::types::chrono::NaiveDate, _>(i) {
                    serde_json::Value::String(v.format("%Y-%m-%d").to_string())
                }
                // Interval → readable string (PgInterval has no Display)
                else if let Ok(v) = row.try_get::<sqlx::postgres::types::PgInterval, _>(i) {
                    let secs = v.microseconds / 1_000_000;
                    let micros = (v.microseconds % 1_000_000).abs();
                    serde_json::Value::String(format!(
                        "{} months {} days {}.{:06} seconds",
                        v.months, v.days, secs, micros
                    ))
                }
                // Numeric/decimal → string (preserves precision)
                else if let Ok(v) = row.try_get::<sqlx::types::Decimal, _>(i) {
                    serde_json::Value::String(v.to_string())
                }
                // JSON
                else if let Ok(v) = row.try_get::<serde_json::Value, _>(i) {
                    v
                }
                // UUID
                else if let Ok(v) = row.try_get::<sqlx::types::Uuid, _>(i) {
                    serde_json::Value::String(v.to_string())
                }
                // Fallback — log so unhandled types surface instead of silently masking
                else {
                    let type_info = col.type_info().name();
                    tracing::warn!(
                        "SQL tool: column '{}' with Postgres type '{}' has no decoder — emitting placeholder",
                        col_name, type_info
                    );
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

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(ids: &[&str]) -> Vec<serde_json::Value> {
        ids.iter()
            .map(|id| serde_json::json!({ "id": id, "body": "hi" }))
            .collect()
    }

    fn ref_of(row: &serde_json::Value) -> Option<&str> {
        row.get("ref").and_then(|v| v.as_str())
    }

    #[test]
    fn single_table_query_gets_citable_refs() {
        let mut r = rows(&["msg_1", "msg_2"]);
        attach_record_refs(
            "SELECT id, body FROM data_communication_message LIMIT 2",
            &mut r,
        );
        assert_eq!(ref_of(&r[0]), Some("/record/communication_message/msg_1"));
        assert_eq!(ref_of(&r[1]), Some("/record/communication_message/msg_2"));
    }

    /// In a join the `id` column may belong to either side, so citing it would
    /// be a confident link to possibly the wrong record.
    #[test]
    fn joined_query_gets_no_refs() {
        let mut r = rows(&["x_1"]);
        attach_record_refs(
            "SELECT d.id FROM wiki_events e JOIN wiki_days d ON d.id = e.day_id",
            &mut r,
        );
        assert_eq!(ref_of(&r[0]), None);
    }

    /// Two ontologies on two DIFFERENT tables, named without a join (a union, a
    /// subquery), is just as ambiguous as a join.
    ///
    /// Renamed from `two_ontologies_get_no_refs`: since migration 0081 two
    /// ontologies can describe one table, and that case must still cite. The
    /// old name now reads as asserting the opposite of what the code does.
    #[test]
    fn two_tables_get_no_refs() {
        let mut r = rows(&["x_1"]);
        attach_record_refs(
            "SELECT id FROM data_calendar_event UNION ALL SELECT id FROM data_communication_email",
            &mut r,
        );
        assert_eq!(ref_of(&r[0]), None);
    }

    /// An aggregate row is not a record.
    #[test]
    fn rows_without_an_id_get_no_refs() {
        let mut r = vec![serde_json::json!({ "count": 12 })];
        attach_record_refs("SELECT count(*) FROM data_calendar_event", &mut r);
        assert_eq!(ref_of(&r[0]), None);
    }

    #[test]
    fn a_selected_ref_column_is_never_shadowed() {
        let mut r = vec![serde_json::json!({ "id": "m1", "ref": "mine" })];
        attach_record_refs("SELECT id, ref FROM data_communication_message", &mut r);
        assert_eq!(ref_of(&r[0]), Some("mine"));
    }

    /// Two ontologies over one table must not read as ambiguity.
    ///
    /// `app_page` and `wiki_article` both sit on `app_pages` (migration 0081).
    /// Before this was distinguished from real ambiguity, the split silently
    /// stripped the `ref` from every SQL-tool row touching that table — and
    /// since the agent prompt says to skip uncited results, they disappeared
    /// from answers instead of erroring.
    #[test]
    fn two_ontologies_on_one_table_still_cite() {
        let mut r = rows(&["page_abc"]);
        attach_record_refs("SELECT id, title FROM app_pages LIMIT 5", &mut r);
        let got = r[0].get("ref").and_then(|v| v.as_str());
        assert!(
            got.is_some_and(|s| s.contains("page_abc")),
            "a query naming one table should still cite, even when two \
             ontologies describe it; got {got:?}"
        );
    }

    // Was missing its #[test] attribute since it was written — it compiled,
    // the compiler warned "never used", and it never once ran.
    #[test]
    fn table_names_match_on_word_boundaries() {
        assert!(mentions_table("select * from wiki_events", "wiki_events"));
        assert!(!mentions_table("select * from wiki_events_backup", "wiki_events"));
        assert!(!mentions_table("select * from my_wiki_events", "wiki_events"));
    }

    /// The boundary is Postgres, not the keyword blocklist.
    ///
    /// `SELECT ... INTO t ...` is the case that proves it: it creates and
    /// populates a table, starts with `select`, and contains none of the
    /// forbidden tokens — so the text checks pass it through. Only the
    /// `SET TRANSACTION READ ONLY` guard stops it. If someone ever "simplifies"
    /// that transaction away and leans on the blocklist alone, this fails.
    ///
    /// Requires a live Postgres: `#[sqlx::test]` provisions a scratch DB and
    /// applies migrations automatically. Set DATABASE_URL when running.
    #[sqlx::test]
    async fn select_into_cannot_write_despite_passing_the_keyword_check(pool: sqlx::PgPool) {
        let sql = "SELECT 1 AS n INTO smuggled_table";

        // Precondition: the text checks do NOT catch this.
        let lowered = sql.to_lowercase();
        assert!(lowered.starts_with("select"), "guard assumes SELECT prefix");
        let forbidden = ["insert", "update", "delete", "drop", "create", "alter", "truncate"];
        assert!(
            !lowered
                .split(|c: char| !c.is_alphanumeric())
                .any(|tok| forbidden.contains(&tok)),
            "SELECT INTO must slip the blocklist — that's the point of this test",
        );

        let tool = SqlQueryTool::new(Arc::new(pool.clone()));
        let result = tool.execute_query(sql, 100).await;

        assert!(
            result.is_err(),
            "SELECT INTO must be rejected by the read-only transaction",
        );

        // And it must not have written: the table must not exist.
        let exists: Option<bool> = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM information_schema.tables \
             WHERE table_name = 'smuggled_table')",
        )
        .fetch_one(&pool)
        .await
        .expect("existence check");
        assert_eq!(
            exists,
            Some(false),
            "read-only transaction must leave no table behind",
        );
    }

    /// An ordinary SELECT still works — the guard rejects writes, not reads.
    #[sqlx::test]
    async fn plain_select_still_succeeds(pool: sqlx::PgPool) {
        let tool = SqlQueryTool::new(Arc::new(pool));
        let result = tool.execute_query("SELECT 1 AS n", 10).await;
        assert!(result.is_ok(), "a plain SELECT must still run: {result:?}");
    }
}

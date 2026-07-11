//! Raw record viewer — fetch a single life-graph record by (ontology, id).
//!
//! Every searchable ontology is a raw `data_*` table, and a semantic-search hit
//! points at one row in it. This gives that row a viewable surface: the data
//! viewer opens `/record/<ontology>/<id>`, and citations link straight to it, so
//! "open the source" lands on the actual email / event / transaction — not just
//! the entity it's about. It also backs the Today/Day drill-down (click a row in
//! the day's ontology table to see the record whole).
//!
//! Safety: the table name is never taken from the request. We resolve the
//! ontology through the registry (`get_ontology`), which is a static allowlist,
//! and use its `table_name`. An unknown ontology is a 404, not a query.

use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;

use crate::error::{Error, Result};

/// A single raw record plus the descriptor metadata a viewer needs to render it.
#[derive(Debug, Serialize)]
pub struct OntologyRecord {
    /// Ontology name (e.g. `communication_email`).
    pub ontology: String,
    /// The record's id within its table.
    pub record_id: String,
    /// Human-readable ontology label (e.g. `Email`).
    pub display_name: String,
    /// Resolved table name (e.g. `data_communication_email`).
    pub table_name: String,
    /// The primary timestamp column, so the viewer can lead with it.
    pub timestamp_column: String,
    /// The full row as a JSON object (all columns).
    pub row: Value,
}

/// Fetch one record. `ontology` is validated against the registry; `record_id`
/// is bound as a parameter. Missing ontology or row → `NotFound`.
///
/// `ontology` may be either the ontology **name** (`calendar_event`, what
/// `semantic_search` cites) or the **table name** (`data_calendar_event`, what
/// the day/ontology data tables carry) — we accept both so every caller can link
/// with whatever it has on hand.
pub async fn get_record(pool: &PgPool, ontology: &str, record_id: &str) -> Result<OntologyRecord> {
    let desc = virtues_registry::ontologies::get_ontology(ontology)
        .or_else(|| {
            virtues_registry::ontologies::registered_ontologies()
                .into_iter()
                .find(|o| o.table_name == ontology)
        })
        .ok_or_else(|| Error::NotFound(format!("Unknown ontology: {}", ontology)))?;

    // `table_name` is a compile-time constant from the registry, never user
    // input, so interpolating it here is injection-safe. The id is bound.
    let sql = format!(
        "SELECT to_jsonb(t) AS row FROM {} t WHERE t.id = $1",
        desc.table_name
    );
    let row: Value = sqlx::query_scalar(&sql)
        .bind(record_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to load record: {}", e)))?
        .ok_or_else(|| Error::NotFound(format!("Record not found: {}/{}", ontology, record_id)))?;

    Ok(OntologyRecord {
        ontology: desc.name.to_string(),
        record_id: record_id.to_string(),
        display_name: desc.display_name.to_string(),
        table_name: desc.table_name.to_string(),
        timestamp_column: desc.timestamp_column.to_string(),
        row,
    })
}

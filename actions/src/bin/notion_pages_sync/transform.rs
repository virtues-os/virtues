//! Notion pages → `data_knowledge_document` transform.
//!
//! Adapted from the deleted `core/src/sources/notion/pages/transform.rs`.
//! Notion's `/v1/search` returns pages with `properties` (title is one), plus
//! `created_time`, `last_edited_time`, `url`, `archived`, etc.
//!
//! Body content fetching (block children) is deferred to a future iteration —
//! this transform stores the page metadata only. Content can be lazily
//! fetched per-page later.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::SqlitePool;
use uuid::Uuid;
use virtues_helpers::dedup::{build_batch_insert_query, BATCH_SIZE};

#[allow(clippy::type_complexity)]
type DocRow = (
    String,                    // id
    Option<String>,            // title
    Option<String>,            // content (None for now; future: fetch blocks)
    String,                    // document_type
    Option<String>,            // external_id (notion page id)
    Option<String>,            // external_url
    Option<DateTime<Utc>>,     // created_time
    Option<DateTime<Utc>>,     // last_modified_time
    String,                    // source_stream_id
    Value,                     // metadata
    i32,                       // is_archived (0 or 1)
);

pub async fn write_pages(db: &SqlitePool, pages: &[Value]) -> Result<usize> {
    let mut pending: Vec<DocRow> = Vec::new();
    let mut written = 0;

    for page in pages {
        let object_kind = page.get("object").and_then(|v| v.as_str()).unwrap_or("");
        if object_kind != "page" && object_kind != "database" {
            continue;
        }

        let notion_id = page.get("id").and_then(|v| v.as_str()).unwrap_or("");
        if notion_id.is_empty() {
            continue;
        }

        let title = extract_title(page);
        let url = page.get("url").and_then(|v| v.as_str()).map(String::from);

        let created_time = page
            .get("created_time")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<DateTime<Utc>>().ok());
        let last_modified_time = page
            .get("last_edited_time")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<DateTime<Utc>>().ok());

        let archived = page
            .get("archived")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let metadata = serde_json::json!({
            "notion_id": notion_id,
            "object": object_kind,
            "parent": page.get("parent"),
            "properties_keys": page.get("properties")
                .and_then(|p| p.as_object())
                .map(|o| o.keys().cloned().collect::<Vec<_>>()),
        });

        let id = Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("notion:doc:{notion_id}").as_bytes(),
        )
        .to_string();

        pending.push((
            id,
            title,
            None,
            object_kind.to_string(),
            Some(notion_id.to_string()),
            url,
            created_time,
            last_modified_time,
            notion_id.to_string(),
            metadata,
            if archived { 1 } else { 0 },
        ));

        if pending.len() >= BATCH_SIZE {
            written += flush(db, &pending).await?;
            pending.clear();
        }
    }
    if !pending.is_empty() {
        written += flush(db, &pending).await?;
    }
    Ok(written)
}

fn extract_title(page: &Value) -> Option<String> {
    let props = page.get("properties")?.as_object()?;
    for (_, prop) in props {
        if prop.get("type").and_then(|v| v.as_str()) == Some("title") {
            if let Some(rich) = prop.get("title").and_then(|v| v.as_array()) {
                let text: String = rich
                    .iter()
                    .filter_map(|t| t.get("plain_text").and_then(|v| v.as_str()))
                    .collect::<Vec<_>>()
                    .join("");
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
    }
    None
}

async fn flush(db: &SqlitePool, records: &[DocRow]) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }
    let sql = build_batch_insert_query(
        "data_knowledge_document",
        &[
            "id",
            "title",
            "content",
            "document_type",
            "external_id",
            "external_url",
            "created_time",
            "last_modified_time",
            "source_stream_id",
            "source_table",
            "source_provider",
            "metadata",
            "is_archived",
        ],
        "source_stream_id",
        records.len(),
    );

    let mut q = sqlx::query(&sql);
    for r in records {
        q = q
            .bind(&r.0)
            .bind(&r.1)
            .bind(&r.2)
            .bind(&r.3)
            .bind(&r.4)
            .bind(&r.5)
            .bind(r.6)
            .bind(r.7)
            .bind(&r.8)
            .bind("notion_pages")
            .bind("notion")
            .bind(&r.9)
            .bind(r.10);
    }
    let result = q.execute(db).await?;
    Ok(result.rows_affected() as usize)
}

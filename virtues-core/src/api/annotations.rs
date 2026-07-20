//! Document annotations API (researcher-plan D2).
//!
//! Highlights + margin notes on drive files, global to the file. Anchoring is
//! quote-based (quote_text + prefix/suffix context) so the passage re-finds in
//! the pdf.js text layer regardless of the Rust extractor's offsets; `rects`
//! are normalized page-space quads for drawing the overlay.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::error::{Error, Result};
use crate::ids;
use crate::types::Timestamp;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Annotation {
    pub id: String,
    pub file_id: String,
    pub page_num: Option<i32>,
    pub quote_text: String,
    pub quote_prefix: String,
    pub quote_suffix: String,
    pub rects: serde_json::Value,
    pub color: String,
    pub note_md: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Deserialize)]
pub struct CreateAnnotationRequest {
    pub file_id: String,
    pub page_num: Option<i32>,
    pub quote_text: String,
    #[serde(default)]
    pub quote_prefix: String,
    #[serde(default)]
    pub quote_suffix: String,
    #[serde(default = "default_rects")]
    pub rects: serde_json::Value,
    #[serde(default = "default_color")]
    pub color: String,
    #[serde(default)]
    pub note_md: String,
}

fn default_rects() -> serde_json::Value {
    serde_json::json!([])
}
fn default_color() -> String {
    "yellow".to_string()
}

/// A highlight's note and/or color can be edited after creation; the anchor
/// (quote, rects, page) is immutable — re-anchoring means a new highlight.
#[derive(Debug, Deserialize)]
pub struct UpdateAnnotationRequest {
    pub color: Option<String>,
    pub note_md: Option<String>,
}

/// List a file's annotations, page then creation order (reading order).
pub async fn list_annotations(pool: &PgPool, file_id: &str) -> Result<Vec<Annotation>> {
    sqlx::query_as::<_, Annotation>(
        "SELECT id, file_id, page_num, quote_text, quote_prefix, quote_suffix, \
                rects, color, note_md, created_at, updated_at \
         FROM app_annotations WHERE file_id = $1 \
         ORDER BY COALESCE(page_num, 0), created_at",
    )
    .bind(file_id)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list annotations: {e}")))
}

pub async fn get_annotation(pool: &PgPool, id: &str) -> Result<Annotation> {
    sqlx::query_as::<_, Annotation>(
        "SELECT id, file_id, page_num, quote_text, quote_prefix, quote_suffix, \
                rects, color, note_md, created_at, updated_at \
         FROM app_annotations WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get annotation: {e}")))?
    .ok_or_else(|| Error::NotFound(format!("Annotation not found: {id}")))
}

pub async fn create_annotation(
    pool: &PgPool,
    req: CreateAnnotationRequest,
) -> Result<Annotation> {
    if req.quote_text.trim().is_empty() {
        return Err(Error::InvalidInput("quote_text cannot be empty".into()));
    }
    // Deterministic per (file, page, quote, prefix) so re-highlighting the same
    // passage upserts rather than duplicating.
    let id = ids::generate_id(
        ids::ANNOTATION_PREFIX,
        &[
            &req.file_id,
            &req.page_num.unwrap_or(0).to_string(),
            &req.quote_text,
            &req.quote_prefix,
        ],
    );
    sqlx::query(
        "INSERT INTO app_annotations \
            (id, file_id, page_num, quote_text, quote_prefix, quote_suffix, rects, color, note_md) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) \
         ON CONFLICT (id) DO UPDATE \
           SET rects = EXCLUDED.rects, color = EXCLUDED.color, \
               note_md = EXCLUDED.note_md, updated_at = now()",
    )
    .bind(&id)
    .bind(&req.file_id)
    .bind(req.page_num)
    .bind(&req.quote_text)
    .bind(&req.quote_prefix)
    .bind(&req.quote_suffix)
    .bind(&req.rects)
    .bind(&req.color)
    .bind(&req.note_md)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create annotation: {e}")))?;
    get_annotation(pool, &id).await
}

pub async fn update_annotation(
    pool: &PgPool,
    id: &str,
    req: UpdateAnnotationRequest,
) -> Result<Annotation> {
    sqlx::query(
        "UPDATE app_annotations \
         SET color = COALESCE($2, color), \
             note_md = COALESCE($3, note_md), \
             updated_at = now() \
         WHERE id = $1",
    )
    .bind(id)
    .bind(req.color.as_deref())
    .bind(req.note_md.as_deref())
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update annotation: {e}")))?;
    get_annotation(pool, id).await
}

pub async fn delete_annotation(pool: &PgPool, id: &str) -> Result<()> {
    // Annotations are retrievable (document_annotation ontology); drop the
    // embedding along with the row so search doesn't cite a deleted highlight.
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| Error::Database(format!("delete tx: {e}")))?;
    sqlx::query("DELETE FROM app_annotations WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to delete annotation: {e}")))?;
    sqlx::query(
        "DELETE FROM search_embeddings WHERE ontology = 'document_annotation' AND record_id = $1",
    )
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|e| Error::Database(format!("Failed to delete annotation embedding: {e}")))?;
    tx.commit()
        .await
        .map_err(|e| Error::Database(format!("delete commit: {e}")))?;
    Ok(())
}

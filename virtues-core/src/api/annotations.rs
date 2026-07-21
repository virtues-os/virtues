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

/// A highlight enriched with its file's name, for the notebook-wide Highlights
/// view (D2.5). Annotations live on files; a notebook gathers them by joining
/// its `library` items (url = `/drive/{file_id}`) back to `app_annotations`.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct NotebookAnnotation {
    pub id: String,
    pub file_id: String,
    pub filename: String,
    pub page_num: Option<i32>,
    pub quote_text: String,
    pub color: String,
    pub note_md: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Every highlight across a notebook's library documents, grouped by file
/// (reading order within each file), newest file activity first.
pub async fn list_notebook_annotations(
    pool: &PgPool,
    notebook_id: &str,
) -> Result<Vec<NotebookAnnotation>> {
    sqlx::query_as::<_, NotebookAnnotation>(
        "SELECT a.id, a.file_id, f.filename, a.page_num, a.quote_text, \
                a.color, a.note_md, a.created_at, a.updated_at \
         FROM app_annotations a \
         JOIN app_notebook_items ni \
           ON ni.url = '/drive/' || a.file_id AND ni.role = 'library' \
         JOIN app_drive_files f ON f.id = a.file_id \
         WHERE ni.notebook_id = $1 \
         ORDER BY a.file_id, COALESCE(a.page_num, 0), a.created_at",
    )
    .bind(notebook_id)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list notebook annotations: {e}")))
}

/// Render a file's highlights as markdown (researcher-plan D4.3).
///
/// Each highlight becomes a blockquote plus a citation ref that lands back on
/// the mark, so an exported set of notes stays traceable to its source.
pub async fn export_file_annotations_md(pool: &PgPool, file_id: &str) -> Result<String> {
    let filename: Option<String> =
        sqlx::query_scalar("SELECT filename FROM app_drive_files WHERE id = $1")
            .bind(file_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| Error::Database(format!("export: filename lookup: {e}")))?;
    let name = filename.unwrap_or_else(|| file_id.to_string());
    let annos = list_annotations(pool, file_id).await?;

    let mut out = format!("# Highlights — {name}\n");
    if annos.is_empty() {
        out.push_str("\n_No highlights yet._\n");
        return Ok(out);
    }
    for a in &annos {
        out.push('\n');
        out.push_str(&render_annotation_md(&name, file_id, a));
        out.push('\n');
    }
    Ok(out)
}

/// Render every highlight across a notebook's library documents, grouped by
/// file in reading order.
pub async fn export_notebook_annotations_md(pool: &PgPool, notebook_id: &str) -> Result<String> {
    let notebook: Option<String> =
        sqlx::query_scalar("SELECT name FROM app_notebooks WHERE id = $1")
            .bind(notebook_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| Error::Database(format!("export: notebook lookup: {e}")))?;
    let title = notebook.unwrap_or_else(|| notebook_id.to_string());
    let annos = list_notebook_annotations(pool, notebook_id).await?;

    let mut out = format!("# Highlights — {title}\n");
    if annos.is_empty() {
        out.push_str("\n_No highlights yet._\n");
        return Ok(out);
    }
    let mut current = String::new();
    for a in &annos {
        if a.file_id != current {
            out.push_str(&format!("\n## {}\n", a.filename));
            current = a.file_id.clone();
        }
        let anno = Annotation {
            id: a.id.clone(),
            file_id: a.file_id.clone(),
            page_num: a.page_num,
            quote_text: a.quote_text.clone(),
            quote_prefix: String::new(),
            quote_suffix: String::new(),
            rects: serde_json::json!([]),
            color: a.color.clone(),
            note_md: a.note_md.clone(),
            created_at: a.created_at.clone(),
            updated_at: a.updated_at.clone(),
        };
        out.push('\n');
        out.push_str(&render_annotation_md(&a.filename, &a.file_id, &anno));
        out.push('\n');
    }
    Ok(out)
}

/// One highlight → blockquote + citation ref (shared by both exporters and
/// mirrored by the client-side "send to page" formatting).
fn render_annotation_md(name: &str, file_id: &str, a: &Annotation) -> String {
    let label = match a.page_num {
        Some(p) => format!("{name}, p. {p}"),
        None => name.to_string(),
    };
    let mut route = format!("/drive/{file_id}?");
    if let Some(p) = a.page_num {
        route.push_str(&format!("page={p}&"));
    }
    route.push_str(&format!("hl={}", a.id));

    let quote = a.quote_text.trim().replace('\n', "\n> ");
    let mut md = format!("> {quote}\n>\n> — [{label}]({route})");
    if !a.note_md.trim().is_empty() {
        md.push_str(&format!("\n\n{}", a.note_md.trim()));
    }
    md
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

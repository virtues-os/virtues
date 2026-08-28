//! Document text extraction (researcher-plan D1).
//!
//! Universal, native-text-only extraction over drive files: every text-bearing
//! upload is extracted, chunked, and (via the `uploaded_document` ontology)
//! embedded — the whole drive is corpus. No OCR here; scanned PDFs are marked
//! `no_text` and become the D5 OCR queue.
//!
//! Extractors sit behind [`TextExtractor`] so backends are swappable (pdfium
//! today, an OCR backend later) — the pipeline and schema never change.
//!
//! Telemetry doctrine: counts and timings only. Never log document content.

mod chunker;
mod docx;
mod html;
mod pdf;

pub use chunker::{chunk_pages, Chunk};

use sqlx::PgPool;

use crate::error::{Error, Result};
use crate::ids;

/// One page of extracted text. `page_num` is 1-based. Unpaged formats
/// (txt/md/html/docx) produce a single "page" with `page_num = None` semantics
/// handled by the chunker (chunks carry `Option<i32>`).
#[derive(Debug)]
pub struct ExtractedPage {
    /// 1-based page number for paged formats; None for unpaged formats.
    pub page_num: Option<i32>,
    pub text: String,
}

/// Outcome of running an extractor over one file.
#[derive(Debug)]
pub enum Extraction {
    /// Text was found.
    Pages(Vec<ExtractedPage>),
    /// The file parsed fine but carries no text layer (scanned PDF).
    NoText,
}

/// A pluggable text extractor for one family of formats.
pub trait TextExtractor {
    /// Extract text from raw file bytes.
    fn extract(&self, bytes: &[u8]) -> Result<Extraction>;
}

/// File kinds we extract, resolved from mime + filename.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DocKind {
    Pdf,
    Docx,
    Plain,
    Html,
}

/// Allowlist: which files are text-bearing (extractable) at all.
/// Mirrors the backfill predicate in migration 0055.
pub fn doc_kind(mime_type: Option<&str>, filename: &str) -> Option<DocKind> {
    let name = filename.to_lowercase();
    match mime_type {
        Some("application/pdf") => return Some(DocKind::Pdf),
        Some("application/vnd.openxmlformats-officedocument.wordprocessingml.document") => {
            return Some(DocKind::Docx)
        }
        Some("text/plain") | Some("text/markdown") => return Some(DocKind::Plain),
        Some("text/html") => return Some(DocKind::Html),
        _ => {}
    }
    if name.ends_with(".pdf") {
        Some(DocKind::Pdf)
    } else if name.ends_with(".docx") {
        Some(DocKind::Docx)
    } else if name.ends_with(".txt") || name.ends_with(".md") || name.ends_with(".markdown") {
        Some(DocKind::Plain)
    } else if name.ends_with(".html") || name.ends_with(".htm") {
        Some(DocKind::Html)
    } else {
        None
    }
}

/// Extracted-text size ceiling — a text stream past this is truncated (the
/// chunk pipeline caps, the file still indexes up to the ceiling).
const MAX_EXTRACTED_CHARS: usize = 50 * 1024 * 1024;

/// Files larger than this on disk are skipped outright.
const MAX_FILE_BYTES: i64 = 200 * 1024 * 1024;

/// How long an `extracting` claim may live before a crashed run's file is
/// recovered back to `pending`.
const STALE_CLAIM_MINUTES: i32 = 30;

/// ID prefix for chunk rows. Deterministic per (file_id, chunk_index) so
/// re-extraction upserts in place.
pub const CHUNK_PREFIX: &str = "chunk";

fn chunk_id(file_id: &str, chunk_index: i32) -> String {
    ids::generate_id(CHUNK_PREFIX, &[file_id, &chunk_index.to_string()])
}

/// Run one extraction drain: claim pending files, extract, chunk, upsert.
/// Returns the number of files processed. Safe to run concurrently (row-level
/// claims via `extracting` status); a crashed run's claims age back to pending.
pub async fn run_extraction_job(pool: &PgPool, config: &crate::api::DriveConfig) -> Result<u64> {
    // Orphaned-embedding sweep: chunk rows die by FK cascade (file purge) and
    // by re-extraction shrinkage, but search_embeddings has no FK to source
    // tables and the indexer's own GC only runs for records that still exist.
    // Collect strays here and keep the BM25 corpus stats honest.
    sweep_orphaned_embeddings(pool).await?;

    // Recover stale claims from crashed runs.
    sqlx::query(
        r#"
        UPDATE app_drive_files
        SET extraction_status = 'pending'
        WHERE extraction_status = 'extracting'
          AND updated_at < now() - ($1 || ' minutes')::interval
        "#,
    )
    .bind(STALE_CLAIM_MINUTES.to_string())
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("stale-claim recovery: {e}")))?;

    let mut processed = 0u64;
    loop {
        // Claim one pending file at a time — extraction is CPU-bound and the
        // drain loop keeps going until the backlog is empty.
        let claimed = sqlx::query_as::<_, (String, String, Option<String>, i64)>(
            r#"
            UPDATE app_drive_files
            SET extraction_status = 'extracting', updated_at = now()
            WHERE id = (
                SELECT id FROM app_drive_files
                WHERE extraction_status = 'pending'
                  AND is_folder = FALSE AND deleted_at IS NULL
                ORDER BY updated_at ASC
                FOR UPDATE SKIP LOCKED
                LIMIT 1
            )
            RETURNING id, filename, mime_type, size_bytes
            "#,
        )
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::Database(format!("claim: {e}")))?;

        let Some((file_id, filename, mime_type, size_bytes)) = claimed else {
            break;
        };

        let status = match extract_one(pool, config, &file_id, &filename, mime_type.as_deref(), size_bytes)
            .await
        {
            Ok(status) => status,
            Err(e) => {
                tracing::warn!(file_id = %file_id, "extraction failed: {e}");
                "failed"
            }
        };

        sqlx::query(
            r#"
            UPDATE app_drive_files
            SET extraction_status = $2,
                extracted_at = CASE WHEN $2 IN ('done','no_text') THEN now() ELSE extracted_at END
            WHERE id = $1
            "#,
        )
        .bind(&file_id)
        .bind(status)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("status update: {e}")))?;

        processed += 1;
    }

    Ok(processed)
}

/// Extract + chunk + upsert one file. Returns the terminal status.
async fn extract_one(
    pool: &PgPool,
    config: &crate::api::DriveConfig,
    file_id: &str,
    filename: &str,
    mime_type: Option<&str>,
    size_bytes: i64,
) -> Result<&'static str> {
    let Some(kind) = doc_kind(mime_type, filename) else {
        return Ok("skipped");
    };
    if size_bytes > MAX_FILE_BYTES {
        return Ok("skipped");
    }

    // Fetch the file's storage path, then its bytes.
    let path: String =
        sqlx::query_scalar("SELECT path FROM app_drive_files WHERE id = $1")
            .bind(file_id)
            .fetch_one(pool)
            .await
            .map_err(|e| Error::Database(format!("path lookup: {e}")))?;
    let bytes = config
        .storage
        .download(&path)
        .await
        .map_err(|e| Error::Storage(format!("read for extraction: {e}")))?;

    // CPU-bound parse off the async runtime.
    let extraction = {
        let kind_owned = kind;
        tokio::task::spawn_blocking(move || -> Result<Extraction> {
            match kind_owned {
                DocKind::Pdf => pdf::PdfExtractor::shared()?.extract(&bytes),
                DocKind::Docx => docx::DocxExtractor.extract(&bytes),
                DocKind::Plain => {
                    let text = String::from_utf8_lossy(&bytes).into_owned();
                    Ok(if text.trim().is_empty() {
                        Extraction::NoText
                    } else {
                        Extraction::Pages(vec![ExtractedPage { page_num: None, text }])
                    })
                }
                DocKind::Html => html::HtmlExtractor.extract(&bytes),
            }
        })
        .await
        .map_err(|e| Error::Other(format!("extraction task join: {e}")))??
    };

    let pages = match extraction {
        Extraction::NoText => return Ok("no_text"),
        Extraction::Pages(pages) => pages,
    };

    let total_chars: usize = pages.iter().map(|p| p.text.len()).sum();
    let pages = if total_chars > MAX_EXTRACTED_CHARS {
        truncate_pages(pages, MAX_EXTRACTED_CHARS)
    } else {
        pages
    };

    let chunks = chunk_pages(&pages);
    if chunks.is_empty() {
        return Ok("no_text");
    }

    // Upsert chunks (deterministic ids); drop stale tail chunks from a
    // previous, longer extraction.
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| Error::Database(format!("chunk tx: {e}")))?;
    for (i, chunk) in chunks.iter().enumerate() {
        let idx = i as i32;
        sqlx::query(
            r#"
            INSERT INTO extracted_document_chunks
                (id, file_id, chunk_index, page_num, char_start, char_end, quote_head, text,
                 occurred_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8,
                    -- Event time = when the document entered the record, not
                    -- when this parser ran. The FK guarantees the file row.
                    (SELECT f.created_at FROM app_drive_files f WHERE f.id = $2))
            ON CONFLICT (file_id, chunk_index) DO UPDATE
            SET page_num = EXCLUDED.page_num,
                char_start = EXCLUDED.char_start,
                char_end = EXCLUDED.char_end,
                quote_head = EXCLUDED.quote_head,
                text = EXCLUDED.text,
                occurred_at = EXCLUDED.occurred_at,
                id = EXCLUDED.id
            "#,
        )
        .bind(chunk_id(file_id, idx))
        .bind(file_id)
        .bind(idx)
        .bind(chunk.page_num)
        .bind(chunk.char_start as i64)
        .bind(chunk.char_end as i64)
        .bind(&chunk.quote_head)
        .bind(&chunk.text)
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("chunk upsert: {e}")))?;
    }
    // Stale tail from a previous longer extraction: drop the chunk rows. Their
    // embeddings are collected by `sweep_orphaned_embeddings` on the next job
    // run (which also fixes the BM25 corpus stats — a raw embedding delete
    // here would corrupt them).
    sqlx::query("DELETE FROM extracted_document_chunks WHERE file_id = $1 AND chunk_index >= $2")
        .bind(file_id)
        .bind(chunks.len() as i32)
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("stale chunk delete: {e}")))?;
    tx.commit()
        .await
        .map_err(|e| Error::Database(format!("chunk tx commit: {e}")))?;

    tracing::info!(
        file_id = %file_id,
        chunks = chunks.len(),
        pages = pages.len(),
        "document extracted"
    );
    Ok("done")
}

/// Delete `uploaded_document` embeddings whose chunk row no longer exists,
/// decrementing the BM25 corpus stats by what's dropped (mirrors the
/// indexer's own stale-tail accounting).
async fn sweep_orphaned_embeddings(pool: &PgPool) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| Error::Database(format!("orphan sweep tx: {e}")))?;
    let dropped: Vec<Option<i64>> = sqlx::query_scalar(
        r#"
        DELETE FROM search_embeddings se
        WHERE se.ontology = 'uploaded_document'
          AND NOT EXISTS (
              SELECT 1 FROM extracted_document_chunks c WHERE c.id = se.record_id
          )
        RETURNING se.bm25_len
        "#,
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| Error::Database(format!("orphan sweep: {e}")))?;
    if !dropped.is_empty() {
        let dropped_len: i64 = dropped.iter().flatten().sum();
        sqlx::query(
            "UPDATE search_index_meta \
             SET n_docs = GREATEST(n_docs - $1, 0), sum_len = GREATEST(sum_len - $2, 0) \
             WHERE singleton",
        )
        .bind(dropped.len() as i64)
        .bind(dropped_len)
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("orphan sweep stats: {e}")))?;
        tracing::info!(count = dropped.len(), "swept orphaned document embeddings");
    }
    tx.commit()
        .await
        .map_err(|e| Error::Database(format!("orphan sweep commit: {e}")))?;
    Ok(())
}

fn truncate_pages(pages: Vec<ExtractedPage>, cap: usize) -> Vec<ExtractedPage> {
    let mut out = Vec::new();
    let mut used = 0usize;
    for mut page in pages {
        if used >= cap {
            break;
        }
        let remaining = cap - used;
        if page.text.len() > remaining {
            let mut cut = remaining;
            while cut > 0 && !page.text.is_char_boundary(cut) {
                cut -= 1;
            }
            page.text.truncate(cut);
        }
        used += page.text.len();
        out.push(page);
    }
    out
}

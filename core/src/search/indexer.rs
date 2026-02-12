//! Background embedding indexer
//!
//! Processes records from searchable ontologies, generates embeddings via
//! the local model, and stores them in search_embeddings + vec_search tables.

use anyhow::Result;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use super::embedder::get_embedder;

/// Maximum records to process per ontology per run
const BATCH_SIZE: i64 = 500;

/// Run one cycle of the embedding indexer.
///
/// For each searchable ontology:
/// 1. Find records not yet embedded (via LEFT JOIN)
/// 2. Embed them via fastembed
/// 3. Insert into search_embeddings + vec_search in a transaction
/// 4. Update progress checkpoint
pub async fn run_embedding_job(pool: &SqlitePool) -> Result<()> {
    let embedder = get_embedder().await?;
    let searchable = virtues_registry::ontologies::registered_ontologies()
        .into_iter()
        .filter(|o| o.embedding.is_some())
        .collect::<Vec<_>>();

    tracing::info!("Embedding indexer: processing {} ontologies", searchable.len());

    let mut total_embedded = 0u64;
    for ontology in &searchable {
        let config = ontology.embedding.as_ref().unwrap();
        let table = ontology.table_name;
        let ont_name = ontology.name;

        // Find unprocessed records via LEFT JOIN (no cursor — always finds gaps)
        // Prefix bare column refs with t. to avoid ambiguity with search_embeddings columns
        let prefix_col = |sql: &str| -> String {
            if sql.contains('.') || sql.contains('(') || sql == "NULL" {
                sql.to_string()
            } else {
                format!("t.{}", sql)
            }
        };
        let timestamp_sql = prefix_col(config.timestamp_sql);
        let title_sql = config.title_sql.map(prefix_col).unwrap_or_else(|| "NULL".to_string());
        let preview_sql = prefix_col(config.preview_sql);
        let author_sql = config.author_sql.map(prefix_col).unwrap_or_else(|| "NULL".to_string());
        let sql = format!(
            "SELECT t.id, \
             {embed_text} as embed_text, \
             {title} as title, \
             {preview} as preview, \
             {author} as author, \
             {timestamp} as ts \
             FROM {table} t \
             LEFT JOIN search_embeddings se ON se.ontology = ? AND se.record_id = t.id \
             WHERE se.id IS NULL \
             ORDER BY t.id ASC \
             LIMIT ?",
            embed_text = config.embed_text_sql,
            title = title_sql,
            preview = preview_sql,
            author = author_sql,
            timestamp = timestamp_sql,
            table = table,
        );

        let rows = sqlx::query_as::<_, (String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>)>(&sql)
            .bind(ont_name)
            .bind(BATCH_SIZE)
            .fetch_all(pool)
            .await?;

        if rows.is_empty() {
            continue;
        }

        tracing::info!("Embedding {} records from {}", rows.len(), ont_name);

        let mut batch_count = 0u64;

        for (record_id, embed_text, title, preview, author, timestamp) in &rows {
            let text = match embed_text {
                Some(t) if !t.trim().is_empty() => t.as_str(),
                _ => {
                    // Insert a placeholder row so LEFT JOIN skips this record next run
                    sqlx::query(
                        "INSERT OR IGNORE INTO search_embeddings \
                         (id, ontology, record_id, text_hash, model, chunk_index) \
                         VALUES (?, ?, ?, 'empty', 'skip', 0)",
                    )
                    .bind(format!("{}:{}", ont_name, record_id))
                    .bind(ont_name)
                    .bind(record_id)
                    .execute(pool)
                    .await?;
                    continue;
                }
            };

            // Generate embedding (runs on blocking thread pool)
            let embedding = match embedder.embed_async(text).await {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!("Failed to embed {}/{}: {}", ont_name, record_id, e);
                    continue;
                }
            };

            // Compute stable text hash for change detection (SHA-256, first 16 hex chars)
            let text_hash = {
                let mut hasher = Sha256::new();
                hasher.update(text.as_bytes());
                format!("{:.16x}", hasher.finalize())
            };
            let embedding_id = format!("{}:{}", ont_name, record_id);

            // Serialize embedding as f32 little-endian bytes for sqlite-vec
            let embedding_bytes: Vec<u8> = embedding
                .iter()
                .flat_map(|f| f.to_le_bytes())
                .collect();

            // Insert metadata + vector in a transaction to avoid orphaned rows
            let mut tx = pool.begin().await?;

            sqlx::query(
                "INSERT OR REPLACE INTO search_embeddings \
                 (id, ontology, record_id, text_hash, model, chunk_index, title, preview, author, timestamp) \
                 VALUES (?, ?, ?, ?, 'nomic-embed-text-v1.5', 0, ?, ?, ?, ?)",
            )
            .bind(&embedding_id)
            .bind(ont_name)
            .bind(record_id)
            .bind(&text_hash)
            .bind(title)
            .bind(preview)
            .bind(author)
            .bind(timestamp)
            .execute(&mut *tx)
            .await?;

            sqlx::query(
                "INSERT OR REPLACE INTO vec_search (embedding_id, embedding) VALUES (?, ?)",
            )
            .bind(&embedding_id)
            .bind(&embedding_bytes)
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;

            batch_count += 1;
        }

        if batch_count > 0 {
            // Update progress stats
            sqlx::query(
                "INSERT INTO search_embedding_progress (ontology, last_processed_id, total_embedded, last_run_at) \
                 VALUES (?, '', ?, datetime('now')) \
                 ON CONFLICT(ontology) DO UPDATE SET \
                 total_embedded = total_embedded + excluded.total_embedded, \
                 last_run_at = datetime('now')",
            )
            .bind(ont_name)
            .bind(batch_count as i64)
            .execute(pool)
            .await?;

            total_embedded += batch_count;
            tracing::info!("Embedded {} records from {}", batch_count, ont_name);
        }
    }

    if total_embedded > 0 {
        tracing::info!("Embedding indexer: {} total records embedded", total_embedded);
    } else {
        tracing::debug!("Embedding indexer: no new records to embed");
    }

    Ok(())
}

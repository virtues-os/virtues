//! Background embedding indexer.
//!
//! Processes records from searchable ontologies, generates embeddings via
//! the local model, and stores them in `search_embeddings` + `search_vectors`
//! (pgvector `vector(1024)` with HNSW cosine index).

use anyhow::Result;
use pgvector::Vector;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use super::embedder::get_embedder;

/// Maximum records to process per ontology per run
const BATCH_SIZE: i64 = 500;

/// Run one cycle of the embedding indexer.
pub async fn run_embedding_job(pool: &PgPool) -> Result<u64> {
    let embedder = get_embedder().await?;
    let searchable = virtues_registry::ontologies::registered_ontologies()
        .into_iter()
        .filter(|o| o.embedding.is_some())
        .collect::<Vec<_>>();

    tracing::debug!("Embedding indexer: checking {} ontologies", searchable.len());

    let mut total_embedded = 0u64;
    for ontology in &searchable {
        let config = ontology.embedding.as_ref().unwrap();
        let table = ontology.table_name;
        let ont_name = ontology.name;

        // Find unprocessed records via LEFT JOIN (no cursor — always finds gaps)
        let prefix_col = |sql: &str| -> String {
            if sql.contains('.') || sql.contains('(') || sql == "NULL" {
                sql.to_string()
            } else {
                format!("t.{}", sql)
            }
        };
        let timestamp_sql = prefix_col(config.timestamp_sql);
        let title_sql = config
            .title_sql
            .map(prefix_col)
            .unwrap_or_else(|| "NULL".to_string());
        let preview_sql = prefix_col(config.preview_sql);
        let author_sql = config
            .author_sql
            .map(prefix_col)
            .unwrap_or_else(|| "NULL".to_string());
        let sql = format!(
            "SELECT t.id, \
             {embed_text} as embed_text, \
             {title} as title, \
             {preview} as preview, \
             {author} as author, \
             {timestamp}::text as ts \
             FROM {table} t \
             LEFT JOIN search_embeddings se ON se.ontology = $1 AND se.record_id = t.id \
             WHERE se.id IS NULL \
             ORDER BY t.id ASC \
             LIMIT $2",
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
                    // Insert a placeholder row so LEFT JOIN skips this record next run.
                    sqlx::query(
                        "INSERT INTO search_embeddings \
                         (id, ontology, record_id, text_hash, model, chunk_index) \
                         VALUES ($1, $2, $3, 'empty', 'skip', 0) \
                         ON CONFLICT (ontology, record_id, chunk_index) DO NOTHING",
                    )
                    .bind(format!("{}:{}", ont_name, record_id))
                    .bind(ont_name)
                    .bind(record_id)
                    .execute(pool)
                    .await?;
                    continue;
                }
            };

            // Parse the record timestamp once (config emits via ::text above).
            let ts_parsed: Option<chrono::DateTime<chrono::Utc>> =
                timestamp.as_ref().and_then(|s| {
                    chrono::DateTime::parse_from_rfc3339(s)
                        .ok()
                        .map(|d| d.with_timezone(&chrono::Utc))
                });

            // Split long records into ~512-token (≈380-word) chunks with ~15%
            // overlap; short records stay a single chunk. Each chunk is its own
            // embedded + lexically-indexed row (chunk_index 0,1,2…). The
            // selection LEFT JOIN above keys on record_id, so a record with any
            // chunk is considered done.
            let chunks = chunk_text(text);
            let mut tx = pool.begin().await?;
            for (ci, chunk) in chunks.iter().enumerate() {
                let embedding = match embedder.embed_async(chunk).await {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!("Failed to embed {}/{} chunk {}: {}", ont_name, record_id, ci, e);
                        continue;
                    }
                };
                let text_hash = {
                    let mut hasher = Sha256::new();
                    hasher.update(chunk.as_bytes());
                    format!("{:.16x}", hasher.finalize())
                };
                let embedding_id = format!("{}:{}:{}", ont_name, record_id, ci);

                sqlx::query(
                    "INSERT INTO search_embeddings \
                     (id, ontology, record_id, text_hash, model, chunk_index, title, preview, author, timestamp, content, source_table) \
                     VALUES ($1, $2, $3, $4, 'embeddinggemma', $10, $5, $6, $7, $8, $9, $11) \
                     ON CONFLICT (ontology, record_id, chunk_index) DO UPDATE SET \
                       text_hash = EXCLUDED.text_hash, \
                       model = EXCLUDED.model, \
                       title = EXCLUDED.title, \
                       preview = EXCLUDED.preview, \
                       author = EXCLUDED.author, \
                       timestamp = EXCLUDED.timestamp, \
                       content = EXCLUDED.content, \
                       source_table = EXCLUDED.source_table",
                )
                .bind(&embedding_id)
                .bind(ont_name)
                .bind(record_id)
                .bind(&text_hash)
                .bind(title)
                .bind(preview)
                .bind(author)
                .bind(ts_parsed)
                .bind(chunk.as_str()) // content — the same text we embed, for lexical/FTS
                .bind(ci as i32)
                .bind(table) // source_table — for the wiki_entity_refs join (entity filtering)
                .execute(&mut *tx)
                .await?;

                sqlx::query(
                    "INSERT INTO search_vectors (embedding_id, embedding) VALUES ($1, $2) \
                     ON CONFLICT (embedding_id) DO UPDATE SET embedding = EXCLUDED.embedding",
                )
                .bind(&embedding_id)
                .bind(Vector::from(embedding))
                .execute(&mut *tx)
                .await?;
            }
            tx.commit().await?;

            batch_count += chunks.len() as u64;
        }

        if batch_count > 0 {
            sqlx::query(
                "INSERT INTO search_embedding_progress \
                   (ontology, last_processed_id, total_embedded, last_run_at) \
                 VALUES ($1, '', $2, now()) \
                 ON CONFLICT(ontology) DO UPDATE SET \
                   total_embedded = search_embedding_progress.total_embedded + EXCLUDED.total_embedded, \
                   last_run_at = now()",
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

    Ok(total_embedded)
}

/// Split text into ~512-token chunks with ~15% overlap. We have no tokenizer
/// here, so we proxy on whitespace words (~380 words ≈ 512 tokens for English).
/// Short text (the common case for personal-data records) returns a single
/// chunk. Overlap preserves context across boundaries so a fact split across a
/// chunk edge is still recallable from both sides.
fn chunk_text(text: &str) -> Vec<String> {
    const WORDS_PER_CHUNK: usize = 380;
    const OVERLAP: usize = 57; // ~15%
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= WORDS_PER_CHUNK {
        return vec![text.to_string()];
    }
    let step = WORDS_PER_CHUNK - OVERLAP;
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < words.len() {
        let end = (start + WORDS_PER_CHUNK).min(words.len());
        chunks.push(words[start..end].join(" "));
        if end == words.len() {
            break;
        }
        start += step;
    }
    chunks
}

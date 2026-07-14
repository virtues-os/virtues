//! Background embedding indexer.
//!
//! Processes records from searchable ontologies, generates embeddings via
//! the local model, and stores them in `search_embeddings` + `search_vectors`
//! (pgvector `vector(1024)` with HNSW cosine index).

use anyhow::Result;
use pgvector::Vector;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::collections::HashMap;

use super::embedder::get_embedder;

/// Maximum records to process per ontology per batch (memory bound — rows are
/// held in memory while their chunks embed).
const BATCH_SIZE: i64 = 500;

/// Wall-clock ceiling for one invocation in drain mode. A fresh corpus drains
/// in one long run (~200 windows/sec sustained), but a wedged state — embedder
/// returning instantly-failing results, a table that never shrinks — must not
/// run forever. Checked between batches, so one batch may overshoot slightly.
const MAX_DRAIN_DURATION: std::time::Duration = std::time::Duration::from_secs(2 * 60 * 60);

/// Advisory lock key for the single-flight guard (arbitrary but stable —
/// ASCII "embidx01" as i64). The runner's own concurrency gate treats runs as
/// stale after 10 minutes, so a multi-hour drain would otherwise race a later
/// cron tick and double-index.
const INDEXER_LOCK_KEY: i64 = 0x656d_6269_6478_3031;

/// Run one cycle of the embedding indexer.
///
/// Drain semantics: for each searchable ontology we loop batches back-to-back
/// until a short batch signals the backlog is empty (or [`MAX_DRAIN_DURATION`]
/// trips). One invocation therefore drains an entire onboarding backlog in
/// hours instead of trickling `BATCH_SIZE` records per 15-minute cron tick.
/// No sleep between batches — the embed sidecar is the natural rate limiter.
pub async fn run_embedding_job(pool: &PgPool) -> Result<u64> {
    // Single-flight guard: a 15-min cron tick landing mid-drain must no-op
    // cleanly, not start a second indexer against the same tables. Session
    // advisory lock on a connection detached from the pool — dropping the
    // detached connection closes it, which releases the lock on every exit
    // path (including `?` early returns).
    let mut lock_conn = pool.acquire().await?.detach();
    let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(INDEXER_LOCK_KEY)
        .fetch_one(&mut lock_conn)
        .await?;
    if !acquired {
        tracing::info!("Embedding indexer: another run holds the advisory lock; skipping");
        return Ok(0);
    }

    let embedder = get_embedder().await?;
    let searchable = virtues_registry::ontologies::registered_ontologies()
        .into_iter()
        .filter(|o| o.embedding.is_some())
        .collect::<Vec<_>>();

    tracing::debug!("Embedding indexer: checking {} ontologies", searchable.len());

    let started = std::time::Instant::now();
    let mut total_embedded = 0u64;

    'ontologies: for ontology in &searchable {
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
        // The backlog is "never indexed OR indexed from different text".
        //
        // It used to be only the former — `WHERE se.id IS NULL` — which meant a
        // record was embedded once and never reconsidered. Edit a page and search
        // answered with the version you first wrote; add a message to a chat and
        // the chat's document froze at whatever it said the first time. That second
        // one is fatal now that a chat IS a document: its text is its messages, and
        // messages arrive.
        //
        // `doc_hash` is computed by Postgres from the same expression that produces
        // the text, so the freshness check and the writer cannot disagree about
        // what the document said. `md5` is change-detection, not security.
        //
        // The join pins `chunk_index = 0` so a multi-chunk record is one row here,
        // not N.
        let sql = format!(
            "SELECT t.id, \
             {embed_text} as embed_text, \
             {title} as title, \
             {preview} as preview, \
             {author} as author, \
             {timestamp}::text as ts, \
             md5(COALESCE({embed_text}, '')) as doc_hash \
             FROM {table} t \
             LEFT JOIN search_embeddings se \
                    ON se.ontology = $1 AND se.record_id = t.id AND se.chunk_index = 0 \
             WHERE se.id IS NULL \
                OR se.doc_hash IS DISTINCT FROM md5(COALESCE({embed_text}, '')) \
             ORDER BY t.id ASC \
             LIMIT $2",
            embed_text = config.embed_text_sql,
            title = title_sql,
            preview = preview_sql,
            author = author_sql,
            timestamp = timestamp_sql,
            table = table,
        );

        // Drain loop: keep pulling batches while they come back full. A short
        // batch means the LEFT JOIN found fewer than BATCH_SIZE gaps — backlog
        // drained for this ontology, move on.
        let mut batches_run = 0u64;
        let mut records_this_run = 0u64;
        loop {
            if started.elapsed() >= MAX_DRAIN_DURATION {
                tracing::warn!(
                    "Embedding indexer: {}s drain ceiling reached in {} ({} records this run); \
                     stopping — remaining backlog resumes on the next cron tick",
                    MAX_DRAIN_DURATION.as_secs(),
                    ont_name,
                    records_this_run,
                );
                break 'ontologies;
            }

            let (fetched, chunks_embedded) =
                embed_one_batch(pool, &embedder, &sql, ont_name, table).await?;
            total_embedded += chunks_embedded;
            records_this_run += fetched as u64;
            batches_run += 1;

            if batches_run % 10 == 0 {
                let elapsed = started.elapsed().as_secs_f64().max(0.001);
                tracing::info!(
                    "Embedding indexer: {} — {} records this run ({:.0} records/sec)",
                    ont_name,
                    records_this_run,
                    records_this_run as f64 / elapsed,
                );
            }

            if (fetched as i64) < BATCH_SIZE {
                break;
            }
            // Full batch — likely more remain; continue immediately, yielding
            // so we don't monopolize the executor between batches.
            tokio::task::yield_now().await;
        }
    }

    if total_embedded > 0 {
        tracing::info!("Embedding indexer: {} total records embedded", total_embedded);
    } else {
        tracing::debug!("Embedding indexer: no new records to embed");
    }

    // Explicit unlock is belt-and-braces; closing the detached connection
    // (dropped below) releases the session lock regardless.
    let _ = sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(INDEXER_LOCK_KEY)
        .execute(&mut lock_conn)
        .await;

    Ok(total_embedded)
}

/// Fetch and embed one batch for one ontology. Returns `(records fetched,
/// chunks embedded)` — the caller uses the fetch count to decide whether the
/// backlog likely has more (a full batch) and the chunk count for progress
/// accounting.
async fn embed_one_batch(
    pool: &PgPool,
    embedder: &std::sync::Arc<super::embedder::LocalEmbedder>,
    sql: &str,
    ont_name: &str,
    table: &str,
) -> Result<(usize, u64)> {
    #[allow(clippy::type_complexity)]
    let rows = sqlx::query_as::<_, (String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>)>(sql)
        .bind(ont_name)
        .bind(BATCH_SIZE)
        .fetch_all(pool)
        .await?;

    if rows.is_empty() {
        return Ok((0, 0));
    }

    tracing::info!("Embedding {} records from {}", rows.len(), ont_name);

    let mut batch_count = 0u64;

    for (record_id, embed_text, title, preview, author, timestamp, doc_hash) in &rows {
        let text = match embed_text {
            Some(t) if !t.trim().is_empty() => t.as_str(),
            _ => {
                // A placeholder, so the backlog stops reconsidering this record.
                //
                // `doc_hash` MUST be set here, not left NULL: the freshness check is
                // `doc_hash IS DISTINCT FROM md5(text)`, and NULL is distinct from
                // everything — including md5(''). A NULL here would make every empty
                // record eternally stale, and the indexer would spin on it forever.
                sqlx::query(
                    "INSERT INTO search_embeddings \
                     (id, ontology, record_id, text_hash, model, chunk_index, doc_hash) \
                     VALUES ($1, $2, $3, 'empty', 'skip', 0, $4) \
                     ON CONFLICT (ontology, record_id, chunk_index) DO UPDATE SET \
                       doc_hash = EXCLUDED.doc_hash",
                )
                .bind(format!("{}:{}", ont_name, record_id))
                .bind(ont_name)
                .bind(record_id)
                .bind(doc_hash)
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

        // Split long records into ~128-token windows (see chunk_text); short
        // records stay a single chunk. Each chunk is its own embedded +
        // lexically-indexed row (chunk_index 0,1,2…). The selection LEFT JOIN
        // above keys on record_id, so a record with any chunk is considered
        // done.
        let chunks = chunk_text(text);
        let mut tx = pool.begin().await?;

        // A re-indexed document may be SHORTER than it was — an edited page, a
        // re-cut event. Nothing deleted stale chunks before, so the tail of the old
        // version survived as orphans: still embedded, still searchable, still
        // citable, describing text that no longer exists. Drop them, and correct
        // the corpus stats by what we drop (0030 admits deletes were never
        // accounted for; this is where that gets paid).
        let stale: Vec<(String, Option<i64>)> = sqlx::query_as(
            "SELECT id, bm25_len FROM search_embeddings \
             WHERE ontology = $1 AND record_id = $2 AND chunk_index >= $3",
        )
        .bind(ont_name)
        .bind(record_id)
        .bind(chunks.len() as i32)
        .fetch_all(&mut *tx)
        .await?;

        if !stale.is_empty() {
            let dropped_len: i64 = stale.iter().filter_map(|(_, l)| *l).sum();
            let ids: Vec<String> = stale.iter().map(|(id, _)| id.clone()).collect();
            // CASCADE takes search_vectors and search_bm25_postings with it.
            sqlx::query("DELETE FROM search_embeddings WHERE id = ANY($1)")
                .bind(&ids)
                .execute(&mut *tx)
                .await?;
            sqlx::query(
                "UPDATE search_bm25_stats \
                 SET n_docs = GREATEST(n_docs - $1, 0), sum_len = GREATEST(sum_len - $2, 0) \
                 WHERE singleton",
            )
            .bind(ids.len() as i64)
            .bind(dropped_len)
            .execute(&mut *tx)
            .await?;
        }
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

            // BM25 lexical terms for this chunk. Same tokenizer query.rs uses.
            let (bm_terms, bm_tfs, bm_len) = bm25_postings(chunk);

            // Was this chunk indexed before? Detected BEFORE the upsert so the
            // corpus stats (N, Σlen) update by the right delta on a re-index.
            // `None` row → new doc; `Some(_)` → existing (bm25_len may be NULL on
            // pre-migration rows, treated as 0).
            let prior_len: Option<Option<i64>> = sqlx::query_scalar(
                "SELECT bm25_len FROM search_embeddings WHERE id = $1",
            )
            .bind(&embedding_id)
            .fetch_optional(&mut *tx)
            .await?;

            sqlx::query(
                "INSERT INTO search_embeddings \
                 (id, ontology, record_id, text_hash, model, chunk_index, title, preview, author, timestamp, content, source_table, bm25_len, doc_hash) \
                 VALUES ($1, $2, $3, $4, $13, $10, $5, $6, $7, $8, $9, $11, $12, $14) \
                 ON CONFLICT (ontology, record_id, chunk_index) DO UPDATE SET \
                   text_hash = EXCLUDED.text_hash, \
                   model = EXCLUDED.model, \
                   title = EXCLUDED.title, \
                   preview = EXCLUDED.preview, \
                   author = EXCLUDED.author, \
                   timestamp = EXCLUDED.timestamp, \
                   content = EXCLUDED.content, \
                   source_table = EXCLUDED.source_table, \
                   bm25_len = EXCLUDED.bm25_len, \
                   doc_hash = EXCLUDED.doc_hash",
            )
            .bind(&embedding_id)
            .bind(ont_name)
            .bind(record_id)
            .bind(&text_hash)
            .bind(title)
            .bind(preview)
            .bind(author)
            .bind(ts_parsed)
            .bind(chunk.as_str()) // content — the same text we embed, for lexical/BM25
            .bind(ci as i32)
            .bind(table) // source_table — for the wiki_entity_refs join (entity filtering)
            .bind(bm_len)
            // The model that ACTUALLY produced this vector, not a literal. Two
            // models of the same width put their vectors in different geometries,
            // and cosine between them is meaningless — so the index has to be able
            // to say which one it was built with.
            .bind(embedder.model_id())
            // Computed by Postgres from the same expression that produced the text,
            // so the freshness check can never disagree with what was indexed.
            .bind(doc_hash)
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

            // Replace this chunk's BM25 postings (idempotent under re-index).
            sqlx::query("DELETE FROM search_bm25_postings WHERE chunk_id = $1")
                .bind(&embedding_id)
                .execute(&mut *tx)
                .await?;
            if !bm_terms.is_empty() {
                sqlx::query(
                    "INSERT INTO search_bm25_postings (chunk_id, term, tf) \
                     SELECT $1, t, f FROM UNNEST($2::text[], $3::int[]) AS u(t, f)",
                )
                .bind(&embedding_id)
                .bind(&bm_terms)
                .bind(&bm_tfs)
                .execute(&mut *tx)
                .await?;
            }

            // Corpus stats: a new chunk adds a doc + its length; a re-index only
            // adjusts Σlen by the length delta (doc count unchanged).
            match prior_len {
                None => {
                    sqlx::query(
                        "UPDATE search_bm25_stats \
                         SET n_docs = n_docs + 1, sum_len = sum_len + $1 WHERE singleton",
                    )
                    .bind(bm_len)
                    .execute(&mut *tx)
                    .await?;
                }
                Some(old) => {
                    sqlx::query(
                        "UPDATE search_bm25_stats SET sum_len = sum_len + $1 WHERE singleton",
                    )
                    .bind(bm_len - old.unwrap_or(0))
                    .execute(&mut *tx)
                    .await?;
                }
            }
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

        tracing::info!("Embedded {} records from {}", batch_count, ont_name);
    }

    Ok((rows.len(), batch_count))
}

/// Build this chunk's BM25 postings: the distinct terms, their term-frequencies
/// (parallel to `terms`), and the total token count (the document length used
/// for BM25 length normalization). Uses the shared [`bm25::tokens`](super::bm25)
/// tokenizer so ingest-time terms match query-time terms exactly.
fn bm25_postings(chunk: &str) -> (Vec<String>, Vec<i32>, i64) {
    let toks = super::bm25::tokens(chunk);
    let len = toks.len() as i64;
    let mut tf: HashMap<String, i32> = HashMap::new();
    for t in toks {
        *tf.entry(t).or_insert(0) += 1;
    }
    let terms: Vec<String> = tf.keys().cloned().collect();
    let tfs: Vec<i32> = terms.iter().map(|t| tf[t]).collect();
    (terms, tfs, len)
}

/// Target window size in whitespace words. Retrieval-quality research
/// (2025–26) converges on 64–128-token chunks for short factual content: the
/// embedder's context is not the constraint (EmbeddingGemma takes 2048
/// tokens), retrieval precision is — small windows keep each vector about one
/// thing. We have no tokenizer here (and must not gain one), so we proxy via
/// the standard approximation 128 tokens ≈ 96 English words.
const WINDOW_WORDS: usize = 96;

/// ~15% of [`WINDOW_WORDS`]. Overlap preserves context across boundaries so a
/// fact split across a chunk edge is still recallable from both sides.
const OVERLAP_WORDS: usize = 14;

/// Hard character cap per chunk. Word-splitting assumes whitespace exists;
/// pathological inputs (base64 blobs, long URLs) can yield a single "word" of
/// arbitrary length, which would blow past the embedder context. Any chunk
/// over the cap is split on the cap (at char boundaries).
const MAX_CHUNK_CHARS: usize = 2048;

/// Split text into ~128-token windows ([`WINDOW_WORDS`] words) with ~15%
/// overlap. Short text (the common case for personal-data records) returns a
/// single chunk, whitespace preserved; multi-chunk output rejoins words with
/// single spaces. Every chunk is additionally bounded by [`MAX_CHUNK_CHARS`].
fn chunk_text(text: &str) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut chunks = Vec::new();
    if words.len() <= WINDOW_WORDS {
        push_capped(text.to_string(), &mut chunks);
        return chunks;
    }
    let step = WINDOW_WORDS - OVERLAP_WORDS;
    let mut start = 0;
    while start < words.len() {
        let end = (start + WINDOW_WORDS).min(words.len());
        push_capped(words[start..end].join(" "), &mut chunks);
        if end == words.len() {
            break;
        }
        start += step;
    }
    chunks
}

/// Push `chunk` onto `out`, splitting it into [`MAX_CHUNK_CHARS`]-byte pieces
/// (backed off to char boundaries) if it exceeds the cap.
fn push_capped(chunk: String, out: &mut Vec<String>) {
    if chunk.len() <= MAX_CHUNK_CHARS {
        out.push(chunk);
        return;
    }
    let mut rest = chunk.as_str();
    while !rest.is_empty() {
        let mut end = MAX_CHUNK_CHARS.min(rest.len());
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        out.push(rest[..end].to_string());
        rest = &rest[end..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_is_single_chunk_verbatim() {
        let text = "just a  short   note\nwith odd whitespace";
        assert_eq!(chunk_text(text), vec![text.to_string()]);
    }

    #[test]
    fn typical_chunks_land_in_60_to_110_word_range() {
        let text = (0..1000).map(|i| format!("word{i}")).collect::<Vec<_>>().join(" ");
        let chunks = chunk_text(&text);
        assert!(chunks.len() > 1);
        for (i, chunk) in chunks.iter().enumerate() {
            let n = chunk.split_whitespace().count();
            if i + 1 < chunks.len() {
                assert!((60..=110).contains(&n), "chunk {i} has {n} words");
            } else {
                // Tail chunk may be short but never oversized.
                assert!(n <= 110, "tail chunk has {n} words");
            }
        }
        // Overlap: each chunk starts OVERLAP_WORDS words before the previous end.
        let first_words: Vec<&str> = chunks[0].split_whitespace().collect();
        let second_words: Vec<&str> = chunks[1].split_whitespace().collect();
        assert_eq!(
            &first_words[first_words.len() - OVERLAP_WORDS..],
            &second_words[..OVERLAP_WORDS],
        );
    }

    #[test]
    fn pathological_no_space_input_is_capped() {
        // A 10k-char base64-ish blob: one "word", must split on the char cap.
        let blob = "A".repeat(10_000);
        let chunks = chunk_text(&blob);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|c| c.len() <= MAX_CHUNK_CHARS));
        // No content lost.
        assert_eq!(chunks.concat(), blob);

        // Multibyte chars: cap must not split inside a char boundary.
        let emoji_blob = "🦀".repeat(3_000);
        let chunks = chunk_text(&emoji_blob);
        assert!(chunks.iter().all(|c| c.len() <= MAX_CHUNK_CHARS));
        assert_eq!(chunks.concat(), emoji_blob);
    }
}

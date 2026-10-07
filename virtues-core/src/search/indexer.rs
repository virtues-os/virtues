//! Background embedding indexer.
//!
//! Processes records from searchable ontologies, embeds them through the box's
//! embedding endpoint (`embedder.rs`), and stores them in `search_embeddings` +
//! `search_vectors` (pgvector `halfvec`, as wide as the model, with an HNSW
//! cosine index).

use anyhow::Result;
use pgvector::Vector;
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

/// The geometry the index was built in, as (model, width), or `None` if nothing
/// has recorded one: a fresh box, or the first run after a reindex.
async fn recorded_geometry(pool: &PgPool) -> Result<Option<(String, i32)>> {
    let recorded: Option<(Option<String>, Option<i32>)> =
        sqlx::query_as("SELECT model, dim FROM search_index_meta WHERE singleton")
            .fetch_optional(pool)
            .await?;
    Ok(match recorded {
        Some((Some(model), Some(dim))) => Some((model, dim)),
        _ => None,
    })
}

/// Refuse an embedder that is not the one the index was built with. A different
/// width or a different model puts its vectors in a space the stored ones do not
/// share: cosine between them means nothing, so written vectors would rot the
/// index and query vectors would rank it at random, with no error anywhere.
fn ensure_same_geometry(built: &(String, i32), model: &str, dim: i32) -> Result<()> {
    let (prev_model, prev_dim) = built;
    if *prev_dim == dim && prev_model == model {
        return Ok(());
    }
    Err(anyhow::anyhow!(
        "the search index was built with {prev_model} at {prev_dim}-d, but the \
         embedding endpoint now serves {model} at {dim}-d.\n\n\
         Vectors from two models live in different geometries — the distance \
         between them is meaningless, so mixing them would quietly rot every \
         search result rather than fail loudly.\n\n\
         Run `virtues reindex` to rebuild the index with the new model \
         (your source data is untouched — embeddings are a cache)."
    ))
}

/// Before searching: is the embedder the one the index was built with? An index
/// with no recorded geometry has nothing to disagree with.
pub(crate) async fn check_index_geometry(pool: &PgPool, model: &str, dim: i32) -> Result<()> {
    match recorded_geometry(pool).await? {
        Some(built) => ensure_same_geometry(&built, model, dim),
        None => Ok(()),
    }
}

/// Establish, or verify, the geometry the index lives in.
///
/// **Empty index** → it has no geometry yet. Record what the endpoint is actually
/// serving and size the vector columns to it. This is the only moment a model may
/// be adopted, and it is safe precisely because there is nothing to contradict.
/// Bringup cannot do the sizing: it reads the width from the record this writes,
/// so on a fresh box, or after a reindex, it finds none and leaves the columns at
/// their old width (the migrations' 256 on a fresh box).
///
/// **Populated index** → the geometry is already decided, and the endpoint must
/// still agree with it (`ensure_same_geometry`). Refuse, and say what to do
/// about it.
///
/// This is what makes "bring your own model" true rather than merely claimed.
pub(crate) async fn reconcile_index_geometry(pool: &PgPool, model: &str, dim: i32) -> Result<()> {
    match recorded_geometry(pool).await? {
        // Geometry established. It must not move under us.
        Some(built) => ensure_same_geometry(&built, model, dim)?,
        // No geometry yet: a fresh box, or the first run after a reindex. Adopt the
        // endpoint we actually have, and record the truth about it.
        None => {
            let empty: i64 = sqlx::query_scalar("SELECT count(*) FROM search_vectors")
                .fetch_one(pool)
                .await?;
            if empty > 0 {
                // Vectors with no recorded geometry: they predate this bookkeeping
                // (built by the old hardcoded path). Adopt rather than destroy —
                // the width is whatever the column says, and it has not changed.
                tracing::warn!(
                    %model, dim,
                    "index has vectors but no recorded geometry (pre-existing); adopting"
                );
            }
            sqlx::query(
                "INSERT INTO search_index_meta (singleton, model, dim) \
                 VALUES (TRUE, $1, $2) \
                 ON CONFLICT (singleton) DO UPDATE SET \
                   model = EXCLUDED.model, dim = EXCLUDED.dim",
            )
            .bind(model)
            .bind(dim)
            .execute(pool)
            .await?;
            tracing::info!(%model, dim, "search index geometry recorded");
            crate::database::Database::from_pool(pool.clone())
                .ensure_embedding_dims()
                .await?;
        }
    }
    Ok(())
}

/// The indexer's single-flight lock. Whoever holds it is the only writer of the
/// index: a 15-min cron tick landing mid-drain must no-op cleanly, not start a
/// second indexer against the same tables, and `virtues reindex` must not have
/// the box's indexer write between its wipe and its re-embed.
///
/// A session advisory lock on a connection detached from the pool. Dropping this
/// drops the connection, which closes it and releases the lock on every exit
/// path (including `?` early returns).
pub(crate) struct IndexerLock(#[allow(dead_code)] sqlx::PgConnection);

impl IndexerLock {
    /// The lock, or `None` if another run holds it.
    pub(crate) async fn try_acquire(pool: &PgPool) -> Result<Option<Self>> {
        let mut conn = pool.acquire().await?.detach();
        let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
            .bind(INDEXER_LOCK_KEY)
            .fetch_one(&mut conn)
            .await?;
        Ok(acquired.then_some(Self(conn)))
    }
}

/// Run one cycle of the embedding indexer, unless another run is already going.
pub async fn run_embedding_job(pool: &PgPool) -> Result<u64> {
    let Some(lock) = IndexerLock::try_acquire(pool).await? else {
        tracing::info!("Embedding indexer: another run holds the advisory lock; skipping");
        return Ok(0);
    };
    drain(pool, &lock).await
}

/// Index everything not yet indexed, for a caller holding the indexer lock.
///
/// Drain semantics: for each searchable ontology we loop batches back-to-back
/// until a short batch signals the backlog is empty (or [`MAX_DRAIN_DURATION`]
/// trips). One invocation therefore drains an entire onboarding backlog in
/// hours instead of trickling `BATCH_SIZE` records per 15-minute cron tick.
/// No sleep between batches — the embed sidecar is the natural rate limiter.
pub(crate) async fn drain(pool: &PgPool, _lock: &IndexerLock) -> Result<u64> {
    let embedder = get_embedder().await?;

    // Before writing a single vector: is this the model the index was built with?
    //
    // Two models can share a width and mean entirely different things by it. Mixing
    // them is silent corruption — the vectors land in different geometries, cosine
    // between them is noise, and nothing anywhere errors. The old code could not
    // even ask: `search_embeddings.model` was the literal 'embeddinggemma', written
    // by this function and read by nobody.
    reconcile_index_geometry(pool, &embedder.model_id(), embedder.dimension() as i32).await?;

    let searchable = virtues_registry::ontologies::registered_ontologies()
        .into_iter()
        .filter(|o| o.embedding.is_some())
        .collect::<Vec<_>>();

    tracing::debug!("Embedding indexer: checking {} ontologies", searchable.len());

    let started = std::time::Instant::now();
    let mut total_embedded = 0u64;

    // Before any draining, so a run that stops at the drain ceiling still
    // dates every ontology.
    if !DATES_BACKFILLED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        for ontology in &searchable {
            let config = ontology.embedding.as_ref().unwrap();
            let ts = prefix_col(config.timestamp_sql);
            match backfill_dates(pool, ontology.name, ontology.table_name, &ts).await {
                Ok(0) => {}
                Ok(n) => tracing::info!(ontology = ontology.name, dated = n, "backfilled chunk dates"),
                Err(e) => tracing::error!(
                    ontology = ontology.name,
                    error = %e,
                    "could not backfill chunk dates"
                ),
            }
        }
    }

    'ontologies: for ontology in &searchable {
        let config = ontology.embedding.as_ref().unwrap();
        let table = ontology.table_name;
        let ont_name = ontology.name;

        // Find unprocessed records via LEFT JOIN (no cursor — always finds gaps)
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
        //
        // The timestamp comes back typed. Its text form ('2026-10-02 18:02:52+00')
        // is not RFC 3339, and parsing it as such stored no date on any chunk, so
        // every date-filtered search returned nothing.
        //
        // The staleness test is PARENTHESISED before `embed_where` is appended,
        // and that is load-bearing rather than tidy. The test is a disjunction —
        // never indexed OR indexed from different text — so splicing a scope
        // onto the end unparenthesised would parse as
        //   se.id IS NULL OR (doc_hash differs AND kind = 'page')
        // by operator precedence. Every never-indexed row of the *other*
        // ontology would satisfy the left branch and get embedded under the
        // wrong name, which is precisely the double-indexing the scope exists
        // to prevent — and it would report success while doing it.
        let sql = format!(
            "SELECT t.id, \
             {embed_text} as embed_text, \
             {title} as title, \
             {preview} as preview, \
             {author} as author, \
             ({timestamp})::timestamptz as ts, \
             md5(COALESCE({embed_text}, '')) as doc_hash \
             FROM {table} t \
             LEFT JOIN search_embeddings se \
                    ON se.ontology = $1 AND se.record_id = t.id AND se.chunk_index = 0 \
             WHERE ({embed_text_is_stale}) {scope} \
             ORDER BY t.id ASC \
             LIMIT $2",
            embed_text = config.embed_text_sql,
            embed_text_is_stale = format!(
                "se.id IS NULL OR se.doc_hash IS DISTINCT FROM md5(COALESCE({}, ''))",
                config.embed_text_sql
            ),
            scope = config.embed_where.unwrap_or(""),
            title = title_sql,
            preview = preview_sql,
            author = author_sql,
            timestamp = timestamp_sql,
            table = table,
        );

        // Rows that have LEFT the scope, evicted before the drain.
        //
        // `embed_where` is a scope, and a scope can be left: a bookmark is
        // tombstoned when the browser it came from drops it, a page's `kind`
        // changes, a record's text is emptied. The backlog query only ever
        // finds rows to ADD, so the index could grow into a scope and never
        // leave it. On 2026-09-21 four bookmarks the owner had deleted in
        // their browser were still being returned to chat and to ⌘K, while
        // the room that owns them hid all four.
        //
        // Deliberately narrow: this evicts rows that still EXIST and no longer
        // qualify. A record deleted outright is somebody else's job — trash
        // clears its own index rows eagerly, and orphaned document chunks have
        // their own sweep — because "the row is gone" and "the row moved out
        // of scope" fail differently and should not share one blunt DELETE.
        if let Some(scope) = config.embed_where {
            match evict_out_of_scope(pool, ont_name, table, scope).await {
                Ok(0) => {}
                Ok(n) => tracing::info!(
                    ontology = ont_name,
                    evicted = n,
                    "records left the ontology's scope; their embeddings are gone"
                ),
                // An eviction failure must not stop the run: indexing what is
                // missing matters more than removing what is stale.
                Err(e) => tracing::error!(
                    ontology = ont_name,
                    error = %e,
                    "could not evict out-of-scope embeddings"
                ),
            }
        }

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

    Ok(total_embedded)
}

/// A registry column expression, qualified to the source table's `t` alias
/// unless it already names a table or is an expression.
fn prefix_col(sql: &str) -> String {
    if sql.contains('.') || sql.contains('(') || sql == "NULL" {
        sql.to_string()
    } else {
        format!("t.{}", sql)
    }
}

/// Set once per process: the date backfill scans every chunk with no date, and
/// a record that genuinely has none (a person's article) would be rescanned on
/// every cron tick.
static DATES_BACKFILLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Give already-indexed chunks the date their record carries.
///
/// The backlog only re-embeds a record whose TEXT changed, so chunks indexed
/// while dates were being dropped would otherwise stay undated forever, and a
/// date filter would keep excluding them.
async fn backfill_dates(
    pool: &PgPool,
    ontology: &str,
    table: &str,
    timestamp_sql: &str,
) -> Result<u64> {
    let done = sqlx::query(&format!(
        "UPDATE search_embeddings se \
         SET occurred_at = ({timestamp_sql})::timestamptz \
         FROM {table} t \
         WHERE se.ontology = $1 AND se.record_id = t.id \
           AND se.occurred_at IS NULL AND ({timestamp_sql}) IS NOT NULL"
    ))
    .bind(ontology)
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}

/// Delete this ontology's embeddings for records that no longer satisfy its
/// `embed_where`, keeping the BM25 corpus stats honest (the same accounting as
/// `trash::drop_embeddings` and `extraction::sweep_orphaned_embeddings`).
///
/// `scope` carries its own leading `AND`, so it is negated as `NOT (TRUE
/// {scope})`. Three-valued logic is on our side here: a scope that evaluates to
/// NULL negates to NULL, the row is not matched, and an expression nobody
/// thought about leaves the index alone rather than emptying it.
async fn evict_out_of_scope(
    pool: &PgPool,
    ontology: &str,
    table: &str,
    scope: &str,
) -> Result<usize> {
    let mut tx = pool.begin().await?;
    let dropped: Vec<Option<i64>> = sqlx::query_scalar(&format!(
        "DELETE FROM search_embeddings se \
         USING {table} t \
         WHERE se.ontology = $1 AND t.id = se.record_id AND NOT (TRUE {scope}) \
         RETURNING se.bm25_len"
    ))
    .bind(ontology)
    .fetch_all(&mut *tx)
    .await?;

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
        .await?;
    }
    tx.commit().await?;
    Ok(dropped.len())
}

/// Chunks handed to the embedder per HTTP call.
///
/// The indexer used to embed **one chunk per request**, sequentially. Every chunk
/// paid a full round-trip, so throughput was bounded by latency rather than by
/// the model: a real mailbox is tens of thousands of chunks, and a reindex became
/// hours of waiting for a job that is minutes of compute.
///
/// 32 is a deliberate middle: large enough that per-request overhead disappears,
/// small enough that a single oversized input cannot fail a huge group, and that
/// the sidecar's memory stays bounded on a box with no GPU.
const EMBED_BATCH: usize = 32;

/// Embed every chunk, in groups, preserving order.
///
/// `None` marks a chunk the embedder refused — the caller skips it, which is the
/// behaviour the one-at-a-time loop had. The subtlety batching introduces: a
/// single bad input would otherwise fail its 31 innocent neighbours, so a failed
/// group is retried one at a time. The slow path costs latency exactly where
/// something is already wrong, and nowhere else.
async fn embed_all(
    embedder: &std::sync::Arc<super::embedder::LocalEmbedder>,
    texts: Vec<String>,
) -> Vec<Option<Vec<f32>>> {
    let mut out: Vec<Option<Vec<f32>>> = Vec::with_capacity(texts.len());

    for group in texts.chunks(EMBED_BATCH) {
        match embedder.embed_batch_async(group.to_vec()).await {
            Ok(vs) if vs.len() == group.len() => out.extend(vs.into_iter().map(Some)),
            // A short/long response means the endpoint is not honouring input
            // order or count. Trust nothing about the mapping; redo it singly.
            Ok(vs) => {
                tracing::warn!(
                    expected = group.len(),
                    got = vs.len(),
                    "embedder returned the wrong number of vectors — retrying the group singly"
                );
                for t in group {
                    out.push(embedder.embed_async(t).await.ok());
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, n = group.len(), "batch embed failed — retrying singly");
                for t in group {
                    match embedder.embed_async(t).await {
                        Ok(v) => out.push(Some(v)),
                        Err(e) => {
                            tracing::warn!(error = %e, "chunk could not be embedded; skipping");
                            out.push(None);
                        }
                    }
                }
            }
        }
    }

    debug_assert_eq!(out.len(), texts.len(), "embed_all must preserve length");
    out
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
    let rows = sqlx::query_as::<_, (String, Option<String>, Option<String>, Option<String>, Option<String>, Option<chrono::DateTime<chrono::Utc>>, Option<String>)>(sql)
        .bind(ont_name)
        .bind(BATCH_SIZE)
        .fetch_all(pool)
        .await?;

    if rows.is_empty() {
        return Ok((0, 0));
    }

    tracing::info!("Embedding {} records from {}", rows.len(), ont_name);

    let mut batch_count = 0u64;

    // ---- Phase 1: chunk everything, and settle the empties ------------------
    //
    // Embedding used to happen inside this loop, ONE HTTP CALL PER CHUNK, one
    // chunk at a time. On a dev corpus of a few hundred that is invisible; on a
    // real mailbox it is tens of thousands of sequential round-trips, and a
    // reindex takes hours of latency rather than minutes of compute. So: work out
    // all the work first, do it in batches, then write.
    struct Doc<'a> {
        record_id: &'a str,
        title: &'a Option<String>,
        preview: &'a Option<String>,
        author: &'a Option<String>,
        doc_hash: &'a Option<String>,
        ts: Option<chrono::DateTime<chrono::Utc>>,
        chunks: Vec<String>,
    }
    let mut docs: Vec<Doc> = Vec::with_capacity(rows.len());

    for (record_id, embed_text, title, preview, author, timestamp, doc_hash) in &rows {
        let text = match embed_text {
            Some(t) if !t.trim().is_empty() => t.as_str(),
            _ => {
                // EMPTIED IS NOT THE SAME AS NEVER-INDEXED, and this branch used to
                // treat them alike: it wrote the placeholder and `continue`d, so the
                // record never reached the stale-chunk deletion below. Chunk 0 kept
                // its old `content` and its `search_vectors` row; chunks 1..N were
                // not touched at all. Deleting the body of a message left every word
                // of it searchable, retrievable and CITABLE — a privacy failure, and
                // the one kind this product cannot afford.
                //
                // So drop the whole record's chunks first, correcting the corpus
                // stats by what we drop, exactly as the shortened-document path does.
                let mut tx = pool.begin().await?;

                let stale: Vec<(String, Option<i64>)> = sqlx::query_as(
                    "SELECT id, bm25_len FROM search_embeddings \
                     WHERE ontology = $1 AND record_id = $2",
                )
                .bind(ont_name)
                .bind(record_id)
                .fetch_all(&mut *tx)
                .await?;

                if !stale.is_empty() {
                    let dropped_docs =
                        stale.iter().filter(|(_, l)| l.is_some()).count() as i64;
                    let dropped_len: i64 = stale.iter().filter_map(|(_, l)| *l).sum();
                    let ids: Vec<String> = stale.iter().map(|(id, _)| id.clone()).collect();
                    // CASCADE takes search_vectors and search_bm25_postings with it.
                    sqlx::query("DELETE FROM search_embeddings WHERE id = ANY($1)")
                        .bind(&ids)
                        .execute(&mut *tx)
                        .await?;
                    // Only rows that COUNTED toward the corpus may be subtracted
                    // from it: a previous placeholder has `bm25_len IS NULL` and was
                    // never added to `n_docs`, so counting it here would drive the
                    // IDF's N below the truth.
                    sqlx::query(
                        "UPDATE search_index_meta \
                         SET n_docs = GREATEST(n_docs - $1, 0), sum_len = GREATEST(sum_len - $2, 0) \
                         WHERE singleton",
                    )
                    .bind(dropped_docs)
                    .bind(dropped_len)
                    .execute(&mut *tx)
                    .await?;
                }

                // Then the placeholder, so the backlog stops reconsidering this
                // record.
                //
                // `doc_hash` MUST be set here, not left NULL: the freshness check is
                // `doc_hash IS DISTINCT FROM md5(text)`, and NULL is distinct from
                // everything — including md5(''). A NULL here would make every empty
                // record eternally stale, and the indexer would spin on it forever.
                sqlx::query(
                    "INSERT INTO search_embeddings \
                     (id, ontology, record_id, model, chunk_index, doc_hash) \
                     VALUES ($1, $2, $3, 'skip', 0, $4) \
                     ON CONFLICT (ontology, record_id, chunk_index) DO UPDATE SET \
                       doc_hash = EXCLUDED.doc_hash",
                )
                .bind(embedding_id(ont_name, record_id, 0))
                .bind(ont_name)
                .bind(record_id)
                .bind(doc_hash)
                .execute(&mut *tx)
                .await?;

                tx.commit().await?;
                continue;
            }
        };

        // Split long records into ~128-token windows (see chunk_text); short
        // records stay a single chunk. Each chunk is its own embedded +
        // lexically-indexed row (chunk_index 0,1,2…).
        docs.push(Doc {
            record_id,
            title,
            preview,
            author,
            doc_hash,
            ts: *timestamp,
            chunks: chunk_text(text),
        });
    }

    if docs.is_empty() {
        return Ok((rows.len(), 0));
    }

    // ---- Phase 2: embed every chunk in the batch, in groups -----------------
    let flat: Vec<String> = docs.iter().flat_map(|d| d.chunks.iter().cloned()).collect();
    let mut vectors = embed_all(embedder, flat).await;
    let mut next = 0usize;

    // ---- Phase 3: write ----------------------------------------------------
    for doc in &docs {
        let Doc { record_id, title, preview, author, doc_hash, ts: ts_parsed, chunks } = doc;
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
                "UPDATE search_index_meta \
                 SET n_docs = GREATEST(n_docs - $1, 0), sum_len = GREATEST(sum_len - $2, 0) \
                 WHERE singleton",
            )
            .bind(ids.len() as i64)
            .bind(dropped_len)
            .execute(&mut *tx)
            .await?;
        }
        for (ci, chunk) in chunks.iter().enumerate() {
            // Phase 2 embedded these in order; `next` walks the flat list in
            // lockstep with the nested one. A `None` is a chunk the embedder
            // refused — warned about there, skipped here, exactly as before.
            let slot = next;
            next += 1;
            let Some(embedding) = vectors[slot].take() else {
                tracing::warn!("skipping unembeddable chunk {}/{} #{}", ont_name, record_id, ci);
                continue;
            };
            let embedding_id = embedding_id(ont_name, record_id, ci);

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
                 (id, ontology, record_id, model, chunk_index, title, preview, author, occurred_at, content, source_table, bm25_len, doc_hash) \
                 VALUES ($1, $2, $3, $12, $9, $4, $5, $6, $7, $8, $10, $11, $13) \
                 ON CONFLICT (ontology, record_id, chunk_index) DO UPDATE SET \
                   model = EXCLUDED.model, \
                   title = EXCLUDED.title, \
                   preview = EXCLUDED.preview, \
                   author = EXCLUDED.author, \
                   occurred_at = EXCLUDED.occurred_at, \
                   content = EXCLUDED.content, \
                   source_table = EXCLUDED.source_table, \
                   bm25_len = EXCLUDED.bm25_len, \
                   doc_hash = EXCLUDED.doc_hash",
            )
            .bind(&embedding_id)
            .bind(ont_name)
            .bind(record_id)
            .bind(title)
            .bind(preview)
            .bind(author)
            .bind(ts_parsed)
            .bind(chunk.as_str()) // content — the same text we embed, for lexical/BM25
            .bind(ci as i32)
            .bind(table) // source_table — for the wiki_refs join (entity filtering)
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
                        "UPDATE search_index_meta \
                         SET n_docs = n_docs + 1, sum_len = sum_len + $1 WHERE singleton",
                    )
                    .bind(bm_len)
                    .execute(&mut *tx)
                    .await?;
                }
                Some(old) => {
                    sqlx::query(
                        "UPDATE search_index_meta SET sum_len = sum_len + $1 WHERE singleton",
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
        // No progress row is written. `search_embedding_progress` existed for
        // resumable indexing and never got it — `last_processed_id` was bound
        // to `''` on every write, so there was nothing to resume from, and the
        // counter beside it had no reader. The log line below is what anyone
        // actually used to follow a reindex.
        tracing::info!("Embedded {} records from {}", batch_count, ont_name);
    }

    Ok((rows.len(), batch_count))
}

/// Build this chunk's BM25 postings: the distinct terms, their term-frequencies
/// (parallel to `terms`), and the total token count (the document length used
/// for BM25 length normalization). Uses the shared [`bm25::tokens`](super::bm25)
/// tokenizer so ingest-time terms match query-time terms exactly.
/// THE id rule: `{ontology}:{record_id}:{chunk_index}`. One Rust authority for
/// both writers; migration 0086's trigger enforces the same rule in Postgres,
/// so a writer that drifts is corrected rather than colliding (see 0085 for
/// what convention-only enforcement cost).
fn embedding_id(ontology: &str, record_id: &str, chunk_index: usize) -> String {
    format!("{ontology}:{record_id}:{chunk_index}")
}

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
    dedupe_identical(chunks)
}

/// Drop chunks whose text has already appeared in this record.
///
/// A chunk identical to one already emitted contributes nothing: it embeds to
/// the same vector, matches the same queries, and adds another identical point
/// to the index. Recall is unchanged by construction — a query that would match
/// the duplicate already matches the original, and the record is returned once
/// either way.
///
/// This is not hypothetical tidiness. Speech-to-text loops: on a real box a
/// single 300-second clip came back as `"[singing] Da da da da…"` repeated to
/// the token cap, 21,714 characters. Windowing turned it into ~89 chunks
/// carrying THREE distinct strings — 89 embedding calls, 86 identical vectors
/// in the index, and enough repeated points to distort HNSW neighbourhoods and
/// skew the BM25 corpus statistics every other query is scored against. Across
/// the corpus, 590 such rows generated 77% of all transcription embeddings.
///
/// Deliberately scoped to WITHIN one record. Two different recordings that both
/// say "Okay." are two honest documents and both should be findable; the
/// duplication that matters is a single document repeating itself.
fn dedupe_identical(chunks: Vec<String>) -> Vec<String> {
    use std::collections::HashSet;
    let mut seen: HashSet<&str> = HashSet::new();
    let mut out: Vec<String> = Vec::with_capacity(chunks.len());
    for c in &chunks {
        if seen.insert(c.as_str()) {
            out.push(c.clone());
        }
    }
    out
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
mod chunk_dedupe_tests {
    use super::*;

    /// The case this exists for: a speech-to-text loop. One real row on a box
    /// came back as 21,714 characters of the same phrase, which windowed into
    /// ~89 chunks holding three distinct strings.
    #[test]
    fn a_repetition_loop_collapses_to_its_distinct_content() {
        let looped = "da da da ".repeat(4_000);
        let chunks = chunk_text(&looped);
        let mut distinct: Vec<&String> = chunks.iter().collect();
        distinct.sort();
        distinct.dedup();
        assert_eq!(
            chunks.len(),
            distinct.len(),
            "every emitted chunk must be distinct"
        );
        assert!(
            chunks.len() < 5,
            "a loop should collapse to a handful, got {}",
            chunks.len()
        );
    }

    /// Ordinary prose must be untouched — overlapping windows are how recall
    /// survives a phrase straddling a boundary, and they are not duplicates.
    #[test]
    fn normal_text_keeps_every_window() {
        let prose: String = (0..600)
            .map(|i| format!("word{i} "))
            .collect::<String>();
        let chunks = chunk_text(&prose);
        assert!(chunks.len() > 1, "long prose should window");
        let mut distinct: Vec<&String> = chunks.iter().collect();
        distinct.sort();
        distinct.dedup();
        assert_eq!(chunks.len(), distinct.len(), "no window was dropped");
    }

    /// Short records — the common case — pass straight through.
    #[test]
    fn short_text_is_one_chunk() {
        assert_eq!(chunk_text("Coffee with Maya."), vec!["Coffee with Maya."]);
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

/// Eviction, against the real registry scope and the real schema.
///
/// The scope is read from the registry rather than written out here: a test
/// that carries its own copy of the predicate passes while the shipped one is
/// wrong, which is the failure mode this whole area keeps having.
#[cfg(test)]
mod eviction_tests {
    use super::*;

    #[sqlx::test]
    async fn a_tombstoned_bookmark_leaves_the_index(pool: PgPool) {
        let scope = virtues_registry::ontologies::registered_ontologies()
            .into_iter()
            .find(|o| o.name == "content_bookmark")
            .and_then(|o| o.embedding.as_ref().map(|e| e.embed_where))
            .expect("content_bookmark is a searchable ontology")
            .expect("its embedding config declares a scope");

        // Two saves with identical text. Only the tombstone is out of scope.
        for (id, tombstoned) in [("evict-live", false), ("evict-gone", true)] {
            sqlx::query(
                "INSERT INTO data_content_bookmark
                   (id, url, title, occurred_at, source_stream_id, source_table,
                    source_provider, deleted_at_source)
                 VALUES ($1, 'https://example.com/a', 'A saved thing', now(), $1,
                         'test', 'test', CASE WHEN $2 THEN now() ELSE NULL END)",
            )
            .bind(id)
            .bind(tombstoned)
            .execute(&pool)
            .await
            .unwrap();

            sqlx::query(
                "INSERT INTO search_embeddings
                   (id, ontology, record_id, model, chunk_index, title, content,
                    bm25_len, doc_hash, created_at)
                 VALUES ($1, 'content_bookmark', $2, 'test-model', 0, 'A saved thing',
                         'A saved thing', 3, 'hash', now())",
            )
            .bind(format!("content_bookmark:{id}:0"))
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        }

        sqlx::query(
            "INSERT INTO search_index_meta (singleton, model, dim, n_docs, sum_len)
             VALUES (true, 'test-model', 384, 2, 6)
             ON CONFLICT (singleton) DO UPDATE SET n_docs = 2, sum_len = 6",
        )
        .execute(&pool)
        .await
        .unwrap();

        let evicted = evict_out_of_scope(
            &pool,
            "content_bookmark",
            "data_content_bookmark",
            scope,
        )
        .await
        .expect("eviction runs");
        assert_eq!(evicted, 1, "exactly the tombstoned row should be evicted");

        let left: Vec<String> =
            sqlx::query_scalar("SELECT record_id FROM search_embeddings ORDER BY record_id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(left, vec!["evict-live".to_string()], "wrong row evicted");

        // The corpus statistics every other query is scored against must
        // follow the delete, or BM25 keeps counting a document that is gone.
        let (n_docs, sum_len): (i64, i64) =
            sqlx::query_as("SELECT n_docs, sum_len FROM search_index_meta WHERE singleton")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!((n_docs, sum_len), (1, 3), "corpus stats did not follow");

        // Idempotent: a second pass has nothing left to take.
        assert_eq!(
            evict_out_of_scope(&pool, "content_bookmark", "data_content_bookmark", scope)
                .await
                .unwrap(),
            0
        );
    }
}

/// The backfill runs each ontology's `timestamp_sql` against its own table, so
/// every registered expression has to execute, and a day's article has to land
/// on that day.
#[cfg(test)]
mod date_tests {
    use super::*;

    async fn index_row(pool: &PgPool, ontology: &str, record_id: &str) {
        sqlx::query(
            "INSERT INTO search_embeddings
               (id, ontology, record_id, model, chunk_index, content, bm25_len, doc_hash, created_at)
             VALUES ($1, $2, $3, 'test-model', 0, 'text', 1, 'hash', now())",
        )
        .bind(format!("{ontology}:{record_id}:0"))
        .bind(ontology)
        .bind(record_id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn occurred_at(pool: &PgPool, record_id: &str) -> Option<chrono::DateTime<chrono::Utc>> {
        sqlx::query_scalar("SELECT occurred_at FROM search_embeddings WHERE record_id = $1")
            .bind(record_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[sqlx::test]
    async fn undated_chunks_take_their_records_date(pool: PgPool) {
        for sql in [
            "INSERT INTO wiki_days (id, date) VALUES ('day_2026-03-03', '2026-03-03')",
            "INSERT INTO app_pages (id, title, kind, content)
               VALUES ('page-day', 'Tuesday, March 3', 'article', 'A long lunch.'),
                      ('page-person', 'David Okafor', 'article', 'David writes most mornings.')",
            "INSERT INTO wiki_articles (id, subject_type, subject_id, page_id)
               VALUES ('art-day', 'day', 'day_2026-03-03', 'page-day'),
                      ('art-person', 'person', 'person_david', 'page-person')",
        ] {
            sqlx::query(sql).execute(&pool).await.unwrap();
        }
        sqlx::query(
            "INSERT INTO data_content_bookmark
               (id, url, title, occurred_at, source_stream_id, source_table, source_provider)
             VALUES ('bm-1', 'https://example.com/a', 'A saved thing',
                     '2026-02-01T09:30:00Z', 'bm-1', 'test', 'test')",
        )
        .execute(&pool)
        .await
        .unwrap();
        index_row(&pool, "wiki_article", "page-day").await;
        index_row(&pool, "wiki_article", "page-person").await;
        index_row(&pool, "content_bookmark", "bm-1").await;

        for o in virtues_registry::ontologies::registered_ontologies() {
            let Some(config) = o.embedding.as_ref() else { continue };
            backfill_dates(&pool, o.name, o.table_name, &prefix_col(config.timestamp_sql))
                .await
                .unwrap_or_else(|e| panic!("{}: timestamp_sql does not run: {e}", o.name));
        }

        assert_eq!(
            occurred_at(&pool, "bm-1").await.map(|d| d.to_rfc3339()),
            Some("2026-02-01T09:30:00+00:00".into()),
        );
        assert_eq!(
            occurred_at(&pool, "page-day").await.map(|d| d.to_rfc3339()),
            Some("2026-03-03T12:00:00+00:00".into()),
            "a day's article is dated at noon on that day",
        );
        assert_eq!(occurred_at(&pool, "page-person").await, None, "a person is not dated");
    }
}

/// The index's geometry, against the real schema the migrations leave behind.
#[cfg(test)]
pub(crate) mod geometry_tests {
    use super::*;

    /// Index one chunk with a `dim`-wide vector, through the indexer's own
    /// statement and binding.
    pub(crate) async fn insert_vector(pool: &PgPool, dim: usize) -> sqlx::Result<()> {
        let id = format!("content_bookmark:geo-{dim}:0");
        sqlx::query(
            "INSERT INTO search_embeddings
               (id, ontology, record_id, model, chunk_index, content, bm25_len, doc_hash, created_at)
             VALUES ($1, 'content_bookmark', $2, 'm', 0, 'text', 1, 'hash', now())",
        )
        .bind(&id)
        .bind(format!("geo-{dim}"))
        .execute(pool)
        .await?;
        sqlx::query(
            "INSERT INTO search_vectors (embedding_id, embedding) VALUES ($1, $2) \
             ON CONFLICT (embedding_id) DO UPDATE SET embedding = EXCLUDED.embedding",
        )
        .bind(&id)
        .bind(Vector::from(vec![0.1f32; dim]))
        .execute(pool)
        .await?;
        Ok(())
    }

    /// A fresh box whose embedder is not 256-wide (every Dragon serves
    /// gte-small at 384). The migrations create the columns at 256, and
    /// bringup cannot size them because nothing has recorded a width yet. The
    /// first embed is the first moment anything knows the width, so it has to
    /// size them there, before writing a vector.
    #[sqlx::test]
    async fn first_embed_sizes_the_columns(pool: PgPool) {
        reconcile_index_geometry(&pool, "m", 384).await.unwrap();
        insert_vector(&pool, 384)
            .await
            .expect("the first vector lands in the index it just recorded");
        assert_eq!(column_types(&pool).await, ["halfvec(384)"; 3]);
    }

    /// Search refuses an embedder the index was not built with: another model at
    /// the same width ranks the index at random without a single error. An index
    /// with no recorded geometry has nothing to disagree with.
    #[sqlx::test]
    async fn search_refuses_another_models_geometry(pool: PgPool) {
        check_index_geometry(&pool, "m", 384).await.expect("nothing recorded yet");

        reconcile_index_geometry(&pool, "m", 384).await.unwrap();
        check_index_geometry(&pool, "m", 384).await.expect("the model it was built with");
        for (model, dim) in [("n", 384), ("m", 768)] {
            let err = check_index_geometry(&pool, model, dim)
                .await
                .expect_err("another model, or another width");
            assert!(err.to_string().contains("virtues reindex"), "{err}");
        }
    }

    /// The three columns that share the index's geometry.
    pub(crate) async fn column_types(pool: &PgPool) -> Vec<String> {
        sqlx::query_scalar(
            "SELECT format_type(a.atttypid, a.atttypmod) FROM pg_attribute a \
             WHERE (a.attrelid, a.attname) IN (('search_vectors'::regclass, 'embedding'), \
                   ('search_topic_cache'::regclass, 'embedding'), \
                   ('app_projects'::regclass, 'centroid')) \
             ORDER BY a.attrelid::regclass::text",
        )
        .fetch_all(pool)
        .await
        .unwrap()
    }
}

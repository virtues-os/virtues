//! Moving the index to another embedding model while search keeps working.
//!
//! A model change used to be `virtues reindex`: wipe, then re-embed in place.
//! Search answered nothing until it finished, which on a CPU box is hours. But
//! chunks and their BM25 postings don't depend on the model; only the vectors
//! do. So the new model's vectors are built in `search_vectors_next`, from the
//! chunk text already stored, while search keeps using the old model and the
//! old table. When every chunk has one, [`step`] swaps the two tables in one
//! transaction and promotes the new endpoint in the box env file, and the next
//! search uses the new model.
//!
//! A change is under way while `VIRTUES_EMBED_NEXT_URL` is set (with the rest of
//! an endpoint's settings beside it: see [`EndpointConfig`]). `virtues upgrade`
//! sets it when a release recommends a different model for a box on the
//! recommended setup; nothing here cares who set it.
//!
//! Runs inside the indexer job, under its lock, after the backlog drains. So no
//! other writer touches the index while this builds or swaps, and every chunk
//! the indexer wrote is visible here. The indexer drops a chunk's next vector
//! when it rewrites the chunk, and this picks it up again.

use std::sync::Arc;
use std::time::Instant;

use anyhow::{anyhow, Result};
use pgvector::Vector;
use sqlx::PgPool;

use super::embedder::{EndpointConfig, LocalEmbedder, ENDPOINT_KEYS};
use super::indexer::IndexerLock;

/// Chunks read and embedded per round trip to the database.
const BATCH: i64 = 256;

/// The share of chunks the new model may refuse before the swap waits. A chunk
/// the new model can't embed loses its vector, not its words: BM25 still finds
/// it. A large share means the server is in trouble, not the chunks.
const MAX_REFUSED_SHARE: f64 = 0.01;

#[derive(Debug, PartialEq)]
pub(crate) enum Progress {
    /// No model change is under way.
    Idle,
    /// Vectors built so far; later runs continue.
    Building { done: i64, total: i64 },
    /// The next server isn't answering, or is failing whole batches.
    Waiting(String),
    /// The new model's index is live.
    Swapped { model: String, dim: i32 },
}

/// A model change in progress, for Settings → Search.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NextStatus {
    pub model: String,
    pub done: i64,
    pub total: i64,
}

/// What Settings shows while a change is under way; `None` otherwise.
pub async fn status(pool: &PgPool) -> Result<Option<NextStatus>> {
    let next: Option<Option<String>> =
        sqlx::query_scalar("SELECT next_model FROM search_index_meta WHERE singleton")
            .fetch_optional(pool)
            .await?;
    let Some(model) = next.flatten() else {
        return Ok(None);
    };
    let (done, total): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM search_vectors_next), (SELECT count(*) FROM search_vectors)",
    )
    .fetch_one(pool)
    .await?;
    Ok(Some(NextStatus { model, done, total }))
}

/// Build what remains of the next index before `deadline`, and swap it in once
/// it's complete. A no-op when no change is under way.
pub(crate) async fn step(pool: &PgPool, _lock: &IndexerLock, deadline: Instant) -> Result<Progress> {
    // A swap whose follow-up was cut off (time limit, a restart): finish it
    // first, whatever the configuration says now.
    if rescore_pending(pool).await? {
        finish(pool).await?;
    }
    let Some(cfg) = EndpointConfig::next() else {
        forget_abandoned_build(pool).await?;
        return Ok(Progress::Idle);
    };
    let next = match LocalEmbedder::connect(cfg).await {
        Ok(e) => Arc::new(e),
        Err(e) => return Ok(Progress::Waiting(format!("{e:#}"))),
    };
    let (model, dim) = (next.model_id(), next.dimension() as i32);

    // Nothing built yet, or built with this very model: there is nothing to
    // rebuild. The endpoint just moves.
    match super::indexer::recorded_geometry(pool).await? {
        Some((m, d)) if m == model && d == dim => {
            promote()?;
            return Ok(Progress::Swapped { model, dim });
        }
        None => {
            promote()?;
            return Ok(Progress::Swapped { model, dim });
        }
        Some(_) => {}
    }

    begin(pool, &model, dim).await?;

    let mut refused: Vec<String> = Vec::new();
    loop {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT e.id, coalesce(e.content, '') FROM search_embeddings e \
             JOIN search_vectors v ON v.embedding_id = e.id \
             LEFT JOIN search_vectors_next n ON n.embedding_id = e.id \
             WHERE n.embedding_id IS NULL AND NOT (e.id = ANY($1)) \
             LIMIT $2",
        )
        .bind(&refused)
        .bind(BATCH)
        .fetch_all(pool)
        .await?;
        if rows.is_empty() {
            break;
        }

        let texts: Vec<String> = rows.iter().map(|(_, t)| t.clone()).collect();
        let vectors = super::indexer::embed_all(&next, texts).await;
        if vectors.iter().all(Option::is_none) {
            return Ok(Progress::Waiting(format!(
                "the next embedding server refused a whole batch of {} chunks",
                rows.len()
            )));
        }
        let mut tx = pool.begin().await?;
        for ((id, _), v) in rows.into_iter().zip(vectors) {
            match v {
                Some(v) => {
                    sqlx::query(
                        "INSERT INTO search_vectors_next (embedding_id, embedding) VALUES ($1, $2) \
                         ON CONFLICT (embedding_id) DO UPDATE SET embedding = EXCLUDED.embedding",
                    )
                    .bind(&id)
                    .bind(Vector::from(v))
                    .execute(&mut *tx)
                    .await?;
                }
                None => refused.push(id),
            }
        }
        tx.commit().await?;

        if Instant::now() >= deadline {
            let (done, total) = counts(pool).await?;
            return Ok(Progress::Building { done, total });
        }
    }

    let (done, total) = counts(pool).await?;
    if total > 0 && refused.len() as f64 > total as f64 * MAX_REFUSED_SHARE {
        return Ok(Progress::Waiting(format!(
            "the next model refused {} of {total} chunks; trying again next run",
            refused.len()
        )));
    }
    if !refused.is_empty() {
        tracing::warn!(refused = refused.len(), "chunks the new model could not embed keep only their words");
    }
    tracing::info!(%model, dim, done, total, "next index complete; swapping it in");

    swap(pool, &model, dim).await?;
    // The database now records the new model, so every search fails its
    // geometry check until the configuration names it too. Promote before
    // anything slow.
    promote()?;
    super::embedder::invalidate_embedder().await;
    finish(pool).await?;
    Ok(Progress::Swapped { model, dim })
}

async fn rescore_pending(pool: &PgPool) -> Result<bool> {
    let pending: Option<bool> =
        sqlx::query_scalar("SELECT rescore_pending FROM search_index_meta WHERE singleton")
            .fetch_optional(pool)
            .await?;
    // absent-ok: no meta row means no index, and so no swap to follow up.
    Ok(pending.unwrap_or(false))
}

/// What follows a swap, outside its lock: stamp the chunks with the new model,
/// forget the old model's event embeddings, resize project centroids, and
/// rescore every day's events. Safe to run again from the start, and it is,
/// by every indexer run, until it completes and clears `rescore_pending`.
/// Whatever invalidates scores must restore them (`dayline::rescore_all_days`).
async fn finish(pool: &PgPool) -> Result<()> {
    let Some((model, dim)) = super::indexer::recorded_geometry(pool).await? else {
        return Err(anyhow!("a model change is finishing but the index records no model"));
    };
    // Nothing reads the stamp but `<> 'skip'`; done here so it never holds
    // search behind the swap's lock.
    sqlx::query("UPDATE search_embeddings SET model = $1 WHERE model <> 'skip' AND model <> $1")
        .bind(&model)
        .execute(pool)
        .await?;
    sqlx::query(crate::cli::reindex::FORGET_EVENT_EMBEDDINGS).execute(pool).await?;
    resize_centroids(pool, dim).await;
    let (days, scored) = crate::dayline::rescore_all_days(pool)
        .await
        .map_err(|e| anyhow!("rescoring events after the model change: {e}"))?;
    sqlx::query("UPDATE search_index_meta SET rescore_pending = false WHERE singleton")
        .execute(pool)
        .await?;
    tracing::info!(days, scored, "events rescored with the new model");
    Ok(())
}

/// Centroids share the index's geometry but have no reader (see
/// `Database::ensure_embedding_dims`). Their ALTER needs `app_projects` to
/// itself; a few short tries rather than one long wait that would hold project
/// reads behind it. If none lands, bringup resizes the column on its own.
async fn resize_centroids(pool: &PgPool, dim: i32) {
    for attempt in 1..=6 {
        let result = async {
            let mut tx = pool.begin().await?;
            sqlx::query("SET LOCAL lock_timeout = '5s'").execute(&mut *tx).await?;
            sqlx::query("UPDATE app_projects SET centroid = NULL WHERE centroid IS NOT NULL")
                .execute(&mut *tx)
                .await?;
            sqlx::query(&format!(
                "ALTER TABLE app_projects ALTER COLUMN centroid TYPE halfvec({dim}) \
                 USING centroid::halfvec({dim})"
            ))
            .execute(&mut *tx)
            .await?;
            tx.commit().await
        }
        .await;
        match result {
            Ok(()) => return,
            Err(e) if attempt == 6 => {
                tracing::warn!(error = %e, "project centroids resize at the next bringup instead")
            }
            Err(_) => tokio::time::sleep(std::time::Duration::from_secs(10)).await,
        }
    }
}

async fn counts(pool: &PgPool) -> Result<(i64, i64)> {
    Ok(sqlx::query_as(
        "SELECT (SELECT count(*) FROM search_vectors_next), (SELECT count(*) FROM search_vectors)",
    )
    .fetch_one(pool)
    .await?)
}

/// Start, or continue, a build for this model. A build for a different model
/// (the next endpoint changed mid-way) is thrown away, since its vectors are in
/// the wrong geometry.
async fn begin(pool: &PgPool, model: &str, dim: i32) -> Result<()> {
    let recorded: Option<(Option<String>, Option<i32>)> = sqlx::query_as(
        "SELECT next_model, next_dim FROM search_index_meta WHERE singleton",
    )
    .fetch_optional(pool)
    .await?;
    if let Some((Some(m), Some(d))) = &recorded {
        if m == model && *d == dim {
            return Ok(());
        }
    }
    let max = super::embedder::MAX_INDEXED_DIM as i32;
    if dim <= 0 || dim > max {
        return Err(anyhow!("the next model's {dim}-d vectors exceed the {max}-d index ceiling"));
    }
    let mut tx = pool.begin().await?;
    for stmt in [
        "TRUNCATE search_vectors_next".to_string(),
        "DROP INDEX IF EXISTS search_vectors_next_hnsw".to_string(),
        // `dim` is a validated integer, so the interpolation is injection-safe.
        format!("ALTER TABLE search_vectors_next ALTER COLUMN embedding TYPE halfvec({dim})"),
    ] {
        sqlx::query(&stmt).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE search_index_meta SET next_model = $1, next_dim = $2 WHERE singleton")
        .bind(model)
        .bind(dim)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    tracing::info!(%model, dim, "building the search index for a new model beside the live one");
    Ok(())
}

/// A build whose next endpoint has since been removed (the change was called
/// off) is dead weight: drop it.
async fn forget_abandoned_build(pool: &PgPool) -> Result<()> {
    let next: Option<Option<String>> =
        sqlx::query_scalar("SELECT next_model FROM search_index_meta WHERE singleton")
            .fetch_optional(pool)
            .await?;
    if next.flatten().is_none() {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    sqlx::query("TRUNCATE search_vectors_next").execute(&mut *tx).await?;
    sqlx::query("DROP INDEX IF EXISTS search_vectors_next_hnsw").execute(&mut *tx).await?;
    sqlx::query("UPDATE search_index_meta SET next_model = NULL, next_dim = NULL WHERE singleton")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    tracing::info!("a model change was called off; its partial index was dropped");
    Ok(())
}

/// Make the next table the live one.
///
/// Search waits on the swap's lock, so the transaction holds only what search
/// reads, and only briefly: renames, the topic cache, and the index's record.
/// The HNSW index is built before it. `lock_timeout` makes it give up rather
/// than queue behind a long transaction while search queues behind it; on a
/// scratch box a wider swap waited eight minutes on the day pipeline with every
/// search stuck behind it. A swap that times out leaves everything as it was,
/// and the next run tries again. The rest follows in [`finish`].
async fn swap(pool: &PgPool, model: &str, dim: i32) -> Result<()> {
    build_next_hnsw(pool).await?;

    let mut tx = pool.begin().await?;
    for stmt in [
        "SET LOCAL lock_timeout = '5s'".to_string(),
        "LOCK TABLE search_vectors, search_vectors_next IN ACCESS EXCLUSIVE MODE".to_string(),
        // Trade names, so the retired table becomes the empty next one and both
        // keep the constraint and index names the schema declares.
        "ALTER TABLE search_vectors RENAME TO search_vectors_swap".to_string(),
        "ALTER TABLE search_vectors_next RENAME TO search_vectors".to_string(),
        "ALTER TABLE search_vectors_swap RENAME TO search_vectors_next".to_string(),
        "ALTER TABLE search_vectors_next RENAME CONSTRAINT search_vectors_pkey TO search_vectors_swap_pkey".to_string(),
        "ALTER TABLE search_vectors_next RENAME CONSTRAINT search_vectors_embedding_id_fkey TO search_vectors_swap_embedding_id_fkey".to_string(),
        "ALTER TABLE search_vectors RENAME CONSTRAINT search_vectors_next_pkey TO search_vectors_pkey".to_string(),
        "ALTER TABLE search_vectors RENAME CONSTRAINT search_vectors_next_embedding_id_fkey TO search_vectors_embedding_id_fkey".to_string(),
        "ALTER TABLE search_vectors_next RENAME CONSTRAINT search_vectors_swap_pkey TO search_vectors_next_pkey".to_string(),
        "ALTER TABLE search_vectors_next RENAME CONSTRAINT search_vectors_swap_embedding_id_fkey TO search_vectors_next_embedding_id_fkey".to_string(),
        "DROP INDEX IF EXISTS search_vectors_hnsw".to_string(),
        "ALTER INDEX search_vectors_next_hnsw RENAME TO search_vectors_hnsw".to_string(),
        "TRUNCATE search_vectors_next".to_string(),
        "ALTER TABLE search_vectors_next ALTER COLUMN embedding TYPE halfvec".to_string(),
        // Topic vectors are compared with query vectors, so they follow the
        // swap inside it; they are a cache, so emptying them loses nothing.
        "TRUNCATE search_topic_cache".to_string(),
        format!("ALTER TABLE search_topic_cache ALTER COLUMN embedding TYPE halfvec({dim})"),
    ] {
        sqlx::query(&stmt).execute(&mut *tx).await?;
    }
    sqlx::query(
        "UPDATE search_index_meta SET model = $1, dim = $2, next_model = NULL, next_dim = NULL, \
         rescore_pending = true WHERE singleton",
    )
    .bind(model)
    .bind(dim)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// The next table's HNSW index, built CONCURRENTLY so that deleting a chunk
/// (which cascades into this table) doesn't wait minutes on the build. A build
/// cut off leaves an invalid index behind; drop it and build again.
async fn build_next_hnsw(pool: &PgPool) -> Result<()> {
    let valid: Option<bool> = sqlx::query_scalar(
        "SELECT i.indisvalid FROM pg_index i \
         WHERE i.indexrelid = to_regclass('search_vectors_next_hnsw')",
    )
    .fetch_optional(pool)
    .await?;
    match valid {
        Some(true) => return Ok(()),
        Some(false) => {
            sqlx::query("DROP INDEX CONCURRENTLY IF EXISTS search_vectors_next_hnsw")
                .execute(pool)
                .await?;
        }
        None => {}
    }
    sqlx::query(&crate::database::hnsw_index_sql(
        "search_vectors_next_hnsw",
        "search_vectors_next",
        true,
    ))
    .execute(pool)
    .await?;
    Ok(())
}

/// Make the next endpoint the current one: in the box env file, which every
/// process reads its embedding settings from (`box_env::var`), and in this
/// process. Each `VIRTUES_EMBED_<key>` takes the next value, or goes when the
/// next endpoint doesn't set it, and the `_NEXT_` keys go.
fn promote() -> Result<()> {
    let mut set: Vec<(String, String)> = Vec::new();
    let mut unset: Vec<String> = Vec::new();
    for key in ENDPOINT_KEYS {
        let current = format!("VIRTUES_EMBED_{key}");
        match crate::box_env::var(&format!("VIRTUES_EMBED_NEXT_{key}")) {
            Some(v) => set.push((current, v)),
            None => unset.push(current),
        }
        unset.push(format!("VIRTUES_EMBED_NEXT_{key}"));
    }
    let set_refs: Vec<(&str, String)> = set.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    let unset_refs: Vec<&str> = unset.iter().map(String::as_str).collect();
    crate::box_env::edit(&crate::box_env::path(), &set_refs, &unset_refs)
        .map_err(|e| anyhow!("promoting the new embedding endpoint: {e}"))?;
    for (k, v) in &set {
        std::env::set_var(k, v);
    }
    for k in &unset {
        std::env::remove_var(k);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::indexer::geometry_tests::{column_types, insert_vector};
    use crate::search::indexer::reconcile_index_geometry;

    async fn fill_next(pool: &PgPool, dim: usize) {
        sqlx::query("INSERT INTO search_vectors_next SELECT embedding_id, $1 FROM search_vectors")
            .bind(Vector::from(vec![0.2f32; dim]))
            .execute(pool)
            .await
            .unwrap();
    }

    /// A box built at 384 by one model moves to a 768-d model. After the swap
    /// the live table is the one built beside it, at the new width with its
    /// HNSW index; the next table is empty and ready for another change; and
    /// the index records the new model, so search accepts its embedder.
    #[sqlx::test]
    async fn the_swap_makes_the_next_index_live(pool: PgPool) {
        reconcile_index_geometry(&pool, "m", 384).await.unwrap();
        insert_vector(&pool, 384).await.unwrap();

        begin(&pool, "n", 768).await.unwrap();
        fill_next(&pool, 768).await;
        assert_eq!(counts(&pool).await.unwrap(), (1, 1));
        swap(&pool, "n", 768).await.unwrap();
        assert!(rescore_pending(&pool).await.unwrap(), "the follow-up is owed until it runs");
        finish(&pool).await.unwrap();
        assert!(!rescore_pending(&pool).await.unwrap());
        let stamped: String = sqlx::query_scalar("SELECT model FROM search_embeddings")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(stamped, "n");

        assert_eq!(column_types(&pool).await, ["halfvec(768)"; 3]);
        let live: i32 = sqlx::query_scalar("SELECT vector_dims(embedding) FROM search_vectors")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(live, 768);
        assert_eq!(counts(&pool).await.unwrap(), (0, 1));
        let (hnsw, next_hnsw): (bool, bool) = sqlx::query_as(
            "SELECT to_regclass('search_vectors_hnsw') IS NOT NULL, \
                    to_regclass('search_vectors_next_hnsw') IS NOT NULL",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(hnsw && !next_hnsw);
        crate::search::indexer::check_index_geometry(&pool, "n", 768)
            .await
            .expect("search accepts the new model");
        assert!(status(&pool).await.unwrap().is_none());

        // The constraints came across: deleting a chunk still takes its vector.
        sqlx::query("DELETE FROM search_embeddings").execute(&pool).await.unwrap();
        let left: i64 = sqlx::query_scalar("SELECT count(*) FROM search_vectors")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(left, 0);

        // And the emptied table takes the next change.
        insert_vector(&pool, 768).await.unwrap();
        begin(&pool, "o", 512).await.unwrap();
        fill_next(&pool, 512).await;
        swap(&pool, "o", 512).await.unwrap();
        finish(&pool).await.unwrap();
        assert_eq!(column_types(&pool).await, ["halfvec(512)"; 3]);
    }

    /// A build for one model, then the next endpoint changes to another: the
    /// first build's vectors are in the wrong geometry, so it starts over.
    #[sqlx::test]
    async fn a_different_next_model_starts_over(pool: PgPool) {
        reconcile_index_geometry(&pool, "m", 384).await.unwrap();
        insert_vector(&pool, 384).await.unwrap();
        begin(&pool, "n", 768).await.unwrap();
        fill_next(&pool, 768).await;

        begin(&pool, "n", 768).await.unwrap();
        assert_eq!(counts(&pool).await.unwrap(), (1, 1), "same model: the build continues");
        begin(&pool, "o", 512).await.unwrap();
        assert_eq!(counts(&pool).await.unwrap(), (0, 1), "another model: it starts over");
        assert_eq!(status(&pool).await.unwrap().unwrap().model, "o");
    }

    /// A change called off (its endpoint removed) drops the partial build.
    #[sqlx::test]
    async fn a_called_off_change_drops_its_build(pool: PgPool) {
        reconcile_index_geometry(&pool, "m", 384).await.unwrap();
        insert_vector(&pool, 384).await.unwrap();
        begin(&pool, "n", 768).await.unwrap();
        fill_next(&pool, 768).await;

        forget_abandoned_build(&pool).await.unwrap();
        assert_eq!(counts(&pool).await.unwrap(), (0, 1));
        assert!(status(&pool).await.unwrap().is_none());
    }
}

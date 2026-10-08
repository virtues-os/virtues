//! `virtues reindex` — rebuild the derived search index from source with the
//! CURRENT model.
//!
//! Distinct from `configure-inference` (which recovers a manual endpoint whose
//! model *changed*, re-pinning the fingerprint): reindex assumes the model is
//! unchanged and just rebuilds. It's what the index-width guard points users at
//! after a schema change — e.g. the halfvec/BM25 upgrade forces a re-embed
//! because 256-dim `vector` rows can't become 384-dim `halfvec` in place — and a
//! manual recovery if the index is ever stale. Works in every inference mode
//! (Dragon NPU, BYO endpoint, bundled).
//!
//! Runs BEFORE the normal app path (like `configure-inference`) because that
//! path's `initialize()` calls `ensure_embedding_dims`, which deliberately
//! refuses a width change on a populated index — the exact wedge this command
//! clears by wiping first.

use sqlx::PgPool;

use crate::error::{Error, Result};

pub async fn run(yes: bool) -> Result<()> {
    let database_url = crate::database::normalize_database_url()?;
    let db = crate::database::Database::new(&database_url)?;
    db.connect().await?;

    println!("Rebuild the search index from source with the current model.");
    println!("This wipes the derived vector + BM25 index — your source data is untouched;");
    println!("embeddings are a cache — and re-embeds everything.");
    if let Some(line) = estimate(db.pool()).await? {
        println!("{line}");
    }
    println!();

    if !yes {
        let ok = dialoguer::Confirm::new()
            .with_prompt("Wipe the derived index and re-embed now?")
            .default(false)
            .interact()
            .unwrap_or(false);
        if !ok {
            println!("Aborted — nothing changed.");
            return Ok(());
        }
    }

    let (embedded, days, scored) = rebuild(db.pool()).await?;

    println!();
    println!("✓ Reindex complete — {embedded} records embedded, {scored} events rescored across {days} days.");
    Ok(())
}

/// How long a re-embed of the current index will take, as a line to print, or
/// `None` for an empty index. Rough: the ingest floor is ~50 windows/s on CPU,
/// and the NPU is far faster. Only an order-of-magnitude hint.
pub(crate) async fn estimate(pool: &PgPool) -> Result<Option<String>> {
    let chunks: i64 = sqlx::query_scalar("SELECT count(*) FROM search_embeddings")
        .fetch_one(pool)
        .await
        .map_err(|e| Error::Database(format!("counting indexed chunks: {e}")))?;
    if chunks == 0 {
        return Ok(None);
    }
    let secs = (chunks as f64 / 50.0).ceil() as i64;
    Ok(Some(format!("~{chunks} chunks to re-embed (rough estimate: {}).", human_dur(secs))))
}

/// Rebuild everything the embedding model produced, with the model the endpoint
/// serves now: wipe it, re-embed from source, and rescore every day's events.
/// Returns (records embedded, days rescored, events scored).
///
/// This is the only way to wipe. `wipe` stays private to this file, because a
/// wipe without the rescore leaves every past day unscored for good.
///
/// No `initialize()`: bringup sizes the vector columns to the recorded width, and
/// the wipe clears it. The re-embed's first step records the current model's
/// width and sizes the columns to it, before any vector is written.
pub(crate) async fn rebuild(pool: &PgPool) -> Result<(u64, u32, u32)> {
    // 0. Take the indexer lock, and keep it until the re-embed is done. If the
    //    box's own indexer ran between the wipe and the re-embed, the re-embed
    //    would skip and report nothing embedded, and its indexer could record
    //    the geometry first.
    let lock = crate::search::indexer::IndexerLock::try_acquire(pool)
        .await
        .map_err(|e| Error::Database(format!("taking the indexer lock: {e:#}")))?
        .ok_or_else(|| {
            Error::Other(
                "the box's indexer is running right now. Nothing was wiped. Wait for it \
                 to finish and run this again, or stop the server first \
                 (sudo systemctl stop virtues)."
                    .into(),
            )
        })?;

    // 1. Wipe the derived index, its recorded geometry and the event scores
    //    (source untouched).
    println!("→ wiping the derived index (vectors + BM25)…");
    wipe(pool).await?;

    // 2. Re-embed from source, inline, to completion (drains the backlog; caps
    //    at the indexer's internal ceiling, after which a restart continues it).
    println!("→ re-embedding from source (this can take a while)…");
    let embedded = crate::search::indexer::drain(pool, &lock)
        .await
        .map_err(|e| Error::Other(format!("re-embed: {e}")))?;
    drop(lock);

    // 3. Put the event scores back. The wipe nulled `wiki_events.embedding` and
    //    every score standing on it, and the nightly cron rescores only the day
    //    it runs for, so without this every past day stays unscored for good.
    //    Whatever invalidates scores restores them (`dayline::rescore_all_days`).
    println!("→ rescoring events (novelty, autonomic, topic, entity)…");
    let (days, scored) = crate::dayline::rescore_all_days(pool)
        .await
        .map_err(|e| Error::Other(format!("rescore: {e}")))?;

    Ok((embedded, days, scored))
}

/// Forget everything the embedding model produced, so the next model starts from
/// nothing: the derived index, its recorded geometry, and every event score that
/// stands on an event embedding. Source rows are never touched — embeddings
/// rebuild from them. Called only by `rebuild`, which puts the scores back.
async fn wipe(pool: &PgPool) -> Result<()> {
    for stmt in [
        // CASCADE clears `search_vectors` and `search_bm25_postings`, which both
        // FK-reference `search_embeddings`.
        "TRUNCATE search_embeddings CASCADE",
        "TRUNCATE search_topic_cache",
        // Corpus stats AND geometry. Clearing the geometry is what makes a model
        // swap possible at all: the indexer refuses to write vectors from a model
        // the index was not built with, and a wipe is precisely the act of saying
        // "build it with this one instead". Leave the geometry behind and the wipe
        // would be blocked by the very guard it exists to clear.
        "UPDATE search_index_meta SET n_docs = 0, sum_len = 0, \
             model = NULL, dim = NULL",
        // wiki_events carries its own embedding blob + derived scores; null them
        // so each scoring pass recomputes with the current model.
        "UPDATE wiki_events SET \
             embedding = NULL, novelty_z = NULL, local_novelty_z = NULL, \
             hr_z = NULL, autonomic_z = NULL, topic_novelty = NULL, \
             entity_novelty = NULL",
    ] {
        sqlx::query(stmt)
            .execute(pool)
            .await
            .map_err(|e| Error::Database(format!("wiping derived index: {e}")))?;
    }
    Ok(())
}

fn human_dur(secs: i64) -> String {
    if secs < 90 {
        format!("~{secs}s")
    } else if secs < 5400 {
        format!("~{}m", (secs + 59) / 60)
    } else {
        format!("~{:.1}h", secs as f64 / 3600.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::indexer::geometry_tests::{column_types, insert_vector};
    use crate::search::indexer::reconcile_index_geometry;

    /// A rebuild with a model of another width. The wipe clears the recorded
    /// width, so bringup has nothing to size the columns to, and the re-embed
    /// writes into the old width unless its first step resizes. `rebuild` needs
    /// a live endpoint, so this runs its two database halves directly.
    #[sqlx::test]
    async fn a_new_width_is_sized_after_the_wipe(pool: PgPool) {
        // A box that has been running at 384: recorded, and sized by a boot.
        reconcile_index_geometry(&pool, "m", 384).await.unwrap();
        crate::database::Database::from_pool(pool.clone())
            .ensure_embedding_dims()
            .await
            .unwrap();
        insert_vector(&pool, 384).await.unwrap();

        wipe(&pool).await.unwrap();
        reconcile_index_geometry(&pool, "n", 768).await.unwrap();
        insert_vector(&pool, 768)
            .await
            .expect("the re-embed writes at the new model's width");
        assert_eq!(column_types(&pool).await, ["halfvec(768)"; 3]);
    }

    /// With the box's indexer mid-run, a rebuild refuses before it wipes
    /// anything, rather than wiping and then finding its re-embed locked out.
    #[sqlx::test]
    async fn rebuild_refuses_while_the_indexer_runs(pool: PgPool) {
        insert_vector(&pool, 256).await.unwrap();
        let _indexer = crate::search::indexer::IndexerLock::try_acquire(&pool)
            .await
            .unwrap()
            .expect("nothing else holds the lock in a fresh database");

        let err = rebuild(&pool).await.expect_err("the indexer holds the lock");
        assert!(err.to_string().contains("indexer is running"), "{err}");
        let chunks: i64 = sqlx::query_scalar("SELECT count(*) FROM search_embeddings")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(chunks, 1, "nothing was wiped");
    }
}

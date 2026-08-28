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

    let chunks: i64 = sqlx::query_scalar("SELECT count(*) FROM search_embeddings")
        .fetch_one(db.pool())
        .await
        .unwrap_or(0);

    println!("Rebuild the search index from source with the current model.");
    println!("This wipes the derived vector + BM25 index — your source data is untouched;");
    println!("embeddings are a cache — and re-embeds everything.");
    if chunks > 0 {
        // Rough estimate at the ingest floor (~50 windows/s CPU; the NPU is far
        // faster). Only an order-of-magnitude hint.
        let secs = (chunks as f64 / 50.0).ceil() as i64;
        println!("~{chunks} chunks to re-embed (rough estimate: {}).", human_dur(secs));
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

    // 1. Wipe the derived index (source untouched). Must precede the resize:
    //    ensure_embedding_dims refuses a width change while vectors are stored.
    println!("→ wiping the derived index (vectors + BM25)…");
    wipe(db.pool()).await?;

    // 2. Migrations (idempotent) + vector-column resize/halfvec-convert. The
    //    tables are empty now, so the width change is allowed.
    println!("→ ensuring schema + sizing the vector index to the model…");
    db.initialize()
        .await
        .map_err(|e| Error::Other(format!("schema/resize: {e}")))?;

    // 3. Re-embed from source, inline, to completion (drains the backlog; caps
    //    at the indexer's internal ceiling, after which a restart continues it).
    println!("→ re-embedding from source (this can take a while)…");
    let embedded = crate::search::indexer::run_embedding_job(db.pool())
        .await
        .map_err(|e| Error::Other(format!("re-embed: {e}")))?;

    // 4. Put the event scores back.
    //
    // The wipe above nulls `wiki_events.embedding` and every score standing on it
    // — novelty, autonomic, topic, entity — and it is right to: a new model puts
    // vectors in a different geometry, where the old numbers mean nothing.
    //
    // But it used to stop there. The nightly cron scores exactly ONE day, the one
    // it runs for, so a reindex quietly destroyed the scores of every PAST day and
    // nothing ever restored them: 82 of 83 days on the dev box, gone, with no
    // error and no mention. The same shape as the bug that made this pipeline
    // useless for months — one step destroying what another produced, silently.
    //
    // Whatever invalidates scores restores them.
    println!("→ rescoring events (novelty, autonomic, topic, entity)…");
    let (days, scored) = crate::dayline::rescore_all_days(db.pool())
        .await
        .map_err(|e| Error::Other(format!("rescore: {e}")))?;

    println!();
    println!("✓ Reindex complete — {embedded} records embedded, {scored} events rescored across {days} days.");
    Ok(())
}

/// Truncate the derived index tables (source rows untouched — embeddings rebuild
/// from them) and reset the BM25 corpus stats. `TRUNCATE ... CASCADE` on
/// `search_embeddings` also clears `search_vectors` and `search_bm25_postings`
/// (both FK-reference it). The single-row `search_index_meta` isn't FK'd, so it
/// is reset explicitly — guarded with `to_regclass` in case reindex runs before
/// the BM25 migration has ever been applied on this box.
async fn wipe(pool: &PgPool) -> Result<()> {
    for stmt in [
        "TRUNCATE search_embeddings CASCADE",
        "TRUNCATE search_topic_cache",
        // Corpus stats AND geometry. Clearing the geometry is what makes a model
        // swap possible at all: the indexer refuses to write vectors from a model
        // the index was not built with, and `reindex` is precisely the act of
        // saying "build it with this one instead". Leave the geometry behind and
        // the wipe would be blocked by the very guard it exists to clear.
        "DO $$ BEGIN IF to_regclass('search_index_meta') IS NOT NULL THEN \
             UPDATE search_index_meta SET n_docs = 0, sum_len = 0, \
                 model = NULL, dim = NULL, fingerprint = NULL, built_at = NULL; \
           END IF; END $$",
        // wiki_events carries its own embedding blob + derived scores; null them
        // so each scoring pass recomputes with the current model.
        "UPDATE wiki_events SET \
             embedding = NULL, novelty_z = NULL, local_novelty_z = NULL, \
             hr_z = NULL, hrv_z = NULL, autonomic_z = NULL, topic_novelty = NULL, \
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

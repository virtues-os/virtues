//! Taking records out of the search index.
//!
//! `search_embeddings` has no foreign key to any source table, so deleting a
//! record does not remove its index rows: the text stays searchable and
//! citable until something removes them. This is that removal, for callers
//! that delete a record outright (the trash's purge, `api::deletions`, the
//! orphaned-chunk sweep).
//!
//! `search_vectors`, `search_vectors_next` and `search_bm25_postings` cascade
//! from `search_embeddings`. The BM25 corpus totals in `search_index_meta` do
//! not, so every delete here subtracts what it removed.

use sqlx::PgConnection;

/// Subtract dropped index rows from the BM25 corpus totals. `dropped` is each
/// removed row's `bm25_len`.
///
/// Only a row that counted toward the corpus is subtracted: a placeholder has
/// `bm25_len IS NULL` and was never added to `n_docs`, so counting it would
/// push the IDF's N below the truth.
pub async fn subtract_corpus_totals(conn: &mut PgConnection, dropped: &[Option<i64>]) -> sqlx::Result<()> {
    let docs = dropped.iter().filter(|l| l.is_some()).count() as i64;
    if docs == 0 {
        return Ok(());
    }
    let len: i64 = dropped.iter().flatten().sum();
    sqlx::query(
        "UPDATE search_index_meta \
         SET n_docs = GREATEST(n_docs - $1, 0), sum_len = GREATEST(sum_len - $2, 0) \
         WHERE singleton",
    )
    .bind(docs)
    .bind(len)
    .execute(conn)
    .await?;
    Ok(())
}

/// Remove every index row of `record_ids` under any of `ontologies`, and
/// return how many rows went. Run it in the transaction that deletes the
/// records, so the index and the table change together.
pub async fn evict_records(
    conn: &mut PgConnection,
    ontologies: &[&str],
    record_ids: &[String],
) -> sqlx::Result<u64> {
    if ontologies.is_empty() || record_ids.is_empty() {
        return Ok(0);
    }
    let dropped: Vec<Option<i64>> = sqlx::query_scalar(
        "DELETE FROM search_embeddings \
         WHERE ontology = ANY($1) AND record_id = ANY($2) \
         RETURNING bm25_len",
    )
    .bind(ontologies)
    .bind(record_ids)
    .fetch_all(&mut *conn)
    .await?;
    subtract_corpus_totals(conn, &dropped).await?;
    Ok(dropped.len() as u64)
}

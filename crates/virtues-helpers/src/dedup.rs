//! Batch insert helpers with `ON CONFLICT DO NOTHING` for idempotent writes.
//!
//! Every ontology table has `source_stream_id TEXT UNIQUE`. Actions generate
//! deterministic stream IDs per record so replays are safe — duplicate writes
//! become no-ops at the database layer.

/// Build a multi-row `INSERT ... ON CONFLICT ({conflict}) DO NOTHING` query.
///
/// Returns the SQL string with `$1, $2, ...` placeholders (sqlx-compatible).
/// Bind parameters row-by-row after calling this.
///
/// # Example
/// ```ignore
/// let sql = build_batch_insert_query(
///     "data_health_heart_rate",
///     &["id", "bpm", "timestamp", "source_stream_id", "source_table", "source_provider", "metadata"],
///     "source_stream_id",
///     records.len(),
/// );
/// let mut q = sqlx::query(&sql);
/// for r in &records {
///     q = q.bind(&r.id).bind(r.bpm).bind(r.timestamp)...;
/// }
/// q.execute(&pool).await?;
/// ```
pub fn build_batch_insert_query(
    table: &str,
    columns: &[&str],
    conflict_column: &str,
    num_rows: usize,
) -> String {
    let num_cols = columns.len();

    let mut query = format!("INSERT INTO {} (", table);
    query.push_str(&columns.join(", "));
    query.push_str(") VALUES ");

    let mut value_clauses = Vec::with_capacity(num_rows);
    for row_idx in 0..num_rows {
        let mut placeholders = Vec::with_capacity(num_cols);
        for col_idx in 0..num_cols {
            let param_num = row_idx * num_cols + col_idx + 1;
            placeholders.push(format!("${}", param_num));
        }
        value_clauses.push(format!("({})", placeholders.join(", ")));
    }

    query.push_str(&value_clauses.join(", "));
    query.push_str(&format!(" ON CONFLICT ({}) DO NOTHING", conflict_column));

    query
}

/// Build a multi-row `INSERT ... ON CONFLICT ({conflict}) DO UPDATE`, correcting the
/// listed `update_columns` on rows that already exist.
///
/// # Why this exists alongside `build_batch_insert_query`
///
/// `DO NOTHING` makes re-ingest a no-op, which is exactly right for a *cloud* sync: if
/// Gmail's transform was wrong, we re-fetch from Gmail. It is exactly wrong for a
/// *device* stream, where there is no upstream to re-fetch from and the fix has to
/// land on the rows we already have.
///
/// The case that forced it: 845 iMessages hold a bare U+FFFC — the invisible
/// placeholder for a photo — because the collector never sent attachment metadata.
/// Under `DO NOTHING`, teaching it to send that metadata would fix *new* messages and
/// leave nine years of history permanently blank. The correction can only reach them
/// through the conflict.
///
/// # The two rules encoded here
///
/// **`metadata` is merged, never replaced** (`metadata || EXCLUDED.metadata`). It is
/// live mutable state that things *other than the transform* write to — the
/// transcription action keeps its `transcribe_attempts` give-up counter there — so
/// overwriting it wholesale would reset that counter and revive a runaway.
///
/// **The caller names the columns it means to correct.** A deny-list would silently
/// start updating any column added later; an allow-list makes each call site state its
/// intent, so a column like `from_name` — owned by the *resolver*, not the transform —
/// simply never appears and cannot be clobbered.
///
/// Emits `RETURNING (xmax = 0) AS inserted` so callers can tell a genuinely new row
/// from a corrected one. `rows_affected()` counts both identically, which would leave
/// a backfill unable to report whether it actually fixed anything.
pub fn build_batch_upsert_query(
    table: &str,
    columns: &[&str],
    conflict_column: &str,
    update_columns: &[&str],
    num_rows: usize,
) -> String {
    debug_assert!(
        update_columns.iter().all(|c| columns.contains(c)),
        "update_columns must be a subset of columns"
    );
    debug_assert!(
        !update_columns
            .iter()
            .any(|c| *c == "id" || *c == "created_at" || *c == conflict_column),
        "identity columns (id / created_at / the conflict key) must never be updated"
    );

    let mut query = build_batch_insert_query(table, columns, conflict_column, num_rows);
    // Swap the DO NOTHING tail for a DO UPDATE.
    let do_nothing = format!(" ON CONFLICT ({}) DO NOTHING", conflict_column);
    query.truncate(query.len() - do_nothing.len());

    let assignments: Vec<String> = update_columns
        .iter()
        .map(|c| {
            if *c == "metadata" {
                format!("{c} = {table}.{c} || EXCLUDED.{c}")
            } else {
                format!("{c} = EXCLUDED.{c}")
            }
        })
        .chain(std::iter::once("updated_at = now()".to_string()))
        .collect();

    query.push_str(&format!(
        " ON CONFLICT ({}) DO UPDATE SET {} RETURNING (xmax = 0) AS inserted",
        conflict_column,
        assignments.join(", ")
    ));

    query
}

/// Default batch size for bulk inserts. iOS payloads are typically smaller than
/// this, but HealthKit initial sync (90 days) can send thousands of records per stream.
pub const BATCH_SIZE: usize = 500;

/// Collapse a batch to one row per conflict key, keeping the **last** occurrence,
/// while preserving the order of everything that survives.
///
/// Postgres rejects a single `INSERT ... ON CONFLICT (k) DO UPDATE` whose VALUES
/// list contains the same `k` twice — *"ON CONFLICT DO UPDATE command cannot
/// affect row a second time"* — and aborts the entire statement, so one repeated
/// key drops the whole batch on the floor and retries it forever. Sources do
/// repeat keys inside a single payload: Apple FinanceKit ships a transaction as
/// both pending and posted, and Plaid's `/transactions/sync` returns a txn in
/// both `added` and `modified`. Dedup before the flush so the batch is legal.
///
/// Last-wins matches `DO UPDATE` semantics: within one payload the newest copy of
/// a row is the one the update would have settled on anyway. (`DO NOTHING`
/// batches are immune to the error and don't need this.)
pub fn dedup_refs_keep_last<'a, T, K, F>(records: &'a [T], key: F) -> Vec<&'a T>
where
    K: Eq + std::hash::Hash,
    F: Fn(&'a T) -> K,
{
    let mut last_idx: std::collections::HashMap<K, usize> =
        std::collections::HashMap::with_capacity(records.len());
    for (i, r) in records.iter().enumerate() {
        last_idx.insert(key(r), i);
    }
    records
        .iter()
        .enumerate()
        .filter_map(|(i, r)| (last_idx.get(&key(r)) == Some(&i)).then_some(r))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_merges_metadata_and_replaces_the_rest() {
        let sql = build_batch_upsert_query(
            "data_communication_message",
            &["id", "body", "metadata", "source_stream_id"],
            "source_stream_id",
            &["body", "metadata"],
            1,
        );
        // The correction lands on the existing row...
        assert!(sql.contains("ON CONFLICT (source_stream_id) DO UPDATE SET"));
        assert!(sql.contains("body = EXCLUDED.body"));
        // ...but metadata is MERGED, or the transcription give-up counter resets.
        assert!(sql.contains("metadata = data_communication_message.metadata || EXCLUDED.metadata"));
        assert!(!sql.contains("metadata = EXCLUDED.metadata"));
        // And a corrected row must be distinguishable from a new one.
        assert!(sql.contains("RETURNING (xmax = 0) AS inserted"));
        assert!(!sql.contains("DO NOTHING"));
    }

    #[test]
    fn unlisted_columns_are_never_touched() {
        // `from_name` is written by the entity resolver, not the transform. It is in the
        // INSERT's blast radius only if someone lists it — and it must not be.
        let sql = build_batch_upsert_query(
            "data_communication_message",
            &["id", "body", "from_name", "source_stream_id"],
            "source_stream_id",
            &["body"],
            1,
        );
        assert!(!sql.contains("from_name = EXCLUDED"));
    }

    #[test]
    fn dedup_keeps_last_occurrence_in_order() {
        // (key, tag) — two rows share key "a"; the batch must keep the *second*
        // "a" (last-wins) and preserve the order of the survivors.
        let rows = vec![("a", 1), ("b", 2), ("a", 3), ("c", 4)];
        let kept: Vec<_> = dedup_refs_keep_last(&rows, |r| r.0)
            .into_iter()
            .copied()
            .collect();
        assert_eq!(kept, vec![("b", 2), ("a", 3), ("c", 4)]);
    }

    #[test]
    fn insert_builder_is_unchanged() {
        let sql = build_batch_insert_query("t", &["a", "b"], "b", 2);
        assert_eq!(
            sql,
            "INSERT INTO t (a, b) VALUES ($1, $2), ($3, $4) ON CONFLICT (b) DO NOTHING"
        );
    }
}

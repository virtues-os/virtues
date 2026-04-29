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

/// Default batch size for bulk inserts. iOS payloads are typically smaller than
/// this, but HealthKit initial sync (90 days) can send thousands of records per stream.
pub const BATCH_SIZE: usize = 500;

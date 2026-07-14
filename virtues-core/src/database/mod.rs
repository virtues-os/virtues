//! Database module for Postgres operations.
//!
//! Postgres runs on the same appliance box (or localhost in dev) via the
//! Docker Compose stack at repo root. Single tenant, single database, single
//! `DATABASE_URL`. No pgbouncer for now — connections are direct.

use std::time::Duration;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;

use crate::error::{Error, Result};

// ─────────────────────────────────────────────────────────────────────────────
// DATABASE_URL handling
// ─────────────────────────────────────────────────────────────────────────────

/// Read `DATABASE_URL` from the environment.
///
/// Kept as a thin helper (vs. the old path-normalizing version) so
/// callers — including action subprocesses — go through one consistent
/// resolver. Postgres URLs are location-independent, so no rewriting needed.
pub fn normalize_database_url() -> Result<String> {
    normalize_from(std::env::var("DATABASE_URL").ok())
}

/// Pure core of [`normalize_database_url`] — takes the env value instead of
/// reading it, so tests never have to mutate the process-global env. (Env
/// mutation in tests races the `#[sqlx::test]` suites, which read
/// DATABASE_URL concurrently and panic on a mid-run change.)
fn normalize_from(value: Option<String>) -> Result<String> {
    value.ok_or_else(|| Error::Configuration("DATABASE_URL env var not set".to_string()))
}

/// Database connection and operations
#[derive(Clone)]
pub struct Database {
    pool: PgPool,
}

impl Database {
    /// Create a new database connection.
    ///
    /// `app_name` is what shows up in `pg_stat_activity.application_name` —
    /// makes the per-action / per-binary connection landscape legible when
    /// debugging on the appliance.
    pub fn new(database_url: &str) -> Result<Self> {
        Self::new_named(database_url, "virtues-core")
    }

    /// Create a new database connection with a custom application name.
    pub fn new_named(database_url: &str, app_name: &str) -> Result<Self> {
        // Default core pool size: 5. Action subprocesses override via env.
        let max_connections = std::env::var("DATABASE_MAX_CONNECTIONS")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(5);

        tracing::info!(
            "Database pool max connections: {} (app={})",
            max_connections,
            app_name
        );

        let opts: PgConnectOptions = database_url
            .parse()
            .map_err(|e| Error::Database(format!("invalid DATABASE_URL: {e}")))?;
        let opts = opts.application_name(app_name);

        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(Duration::from_secs(10))
            .idle_timeout(Duration::from_secs(600))
            .max_lifetime(Duration::from_secs(1800))
            .connect_lazy_with(opts);

        Ok(Self { pool })
    }

    /// Create from an existing pool
    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get the underlying connection pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Initialize database (run migrations, etc.)
    ///
    /// Waits up to 30s for Postgres to accept connections before failing — on
    /// a fresh box, PG can be in WAL recovery for several seconds after
    /// systemd reports the unit "started." Without the retry the daemon would
    /// panic, systemd would restart it 5s later, and the cycle would repeat
    /// until PG was ready. Also covers the runtime case where PG restarts
    /// (rare, but real).
    pub async fn initialize(&self) -> Result<()> {
        self.wait_for_postgres(std::time::Duration::from_secs(30)).await?;

        // Run migrations
        self.run_migrations().await?;

        // Size the vector columns to the configured embedding model. Migrations
        // create them at the Dragon default (256); a manual endpoint with
        // different native dims needs a resize before anything is indexed.
        self.ensure_embedding_dims().await?;

        Ok(())
    }

    /// Size the pgvector columns to the width the index was BUILT at, read from
    /// `search_index_meta` — never from a constant, and never from the network.
    ///
    /// Bringup runs on boxes whose embedder is not running (`virtues migrate`, most
    /// of the CLI), so it cannot go asking a sidecar how wide its vectors are. The
    /// database remembers; the embedder's job at runtime is to *verify* that memory
    /// (see `search::indexer`), not to supply it.
    ///
    /// No recorded width means the index has never been built — leave the column at
    /// its migration default and let the first embed record the truth. Refuses to
    /// resize a populated index: that is a re-embed, and `virtues reindex` owns it.
    async fn ensure_embedding_dims(&self) -> Result<()> {
        let Some(target) = crate::search::embedder::index_dim(&self.pool).await else {
            // Never embedded. Nothing to match yet.
            return Ok(());
        };
        let max = crate::search::embedder::MAX_INDEXED_DIM;
        if target > max {
            return Err(Error::Database(format!(
                "the index records a {target}-dim model, above pgvector's {max}-dim HNSW \
                 ceiling for halfvec — use a narrower model, or set VIRTUES_EMBED_DIMS to \
                 truncate (only safe if the model is Matryoshka-trained)"
            )));
        }

        // The target column is `halfvec(target)` (fp16 — half the storage, ~3×
        // faster query, recall within noise of fp32). We need both the width AND
        // the type family: an already-`halfvec` column at the right width is a
        // no-op; a legacy `vector` column at the right width still converts (a
        // lossless-enough cast — no re-embed), and any width change needs a
        // reindex.
        let current_type = self.vector_column_type("search_vectors", "embedding").await?;
        let current = current_type.as_deref().and_then(parse_vector_dim);
        let is_halfvec = current_type
            .as_deref()
            .map(|t| t.trim_start().starts_with("halfvec"))
            .unwrap_or(false);
        if is_halfvec && current == Some(target) {
            return Ok(());
        }

        // Only a WIDTH change drops the vectors' meaning and forces a re-embed;
        // a same-width vector→halfvec cast preserves them. Guard on the former.
        let dim_changed = current != Some(target);
        if dim_changed {
            let populated: i64 = sqlx::query_scalar(
                "SELECT (SELECT count(*) FROM search_vectors) \
                      + (SELECT count(*) FROM search_topic_cache)",
            )
            .fetch_one(&self.pool)
            .await
            .map_err(|e| Error::Database(format!("counting stored vectors: {e}")))?;
            if populated > 0 {
                let from = current.map(|d| d.to_string()).unwrap_or_else(|| "?".into());
                return Err(Error::Database(format!(
                    "embedding index is halfvec({from}) but the configured model needs \
                     halfvec({target}), and {populated} vectors are already stored — run \
                     `virtues reindex` to re-embed (wipes the derived vectors, keeps your data)"
                )));
            }
        }

        tracing::info!(
            from = ?current_type, to = target, "sizing embedding columns (halfvec) to the configured model"
        );
        // target is a validated usize (≤ max), so the format! interpolation is
        // injection-safe. Cast to halfvec and rebuild the HNSW index. Cosine ops
        // (`<=>`) — matches the operator query.rs uses, and for our L2-normalized
        // embeddings it ranks identically to inner product.
        for stmt in [
            "DROP INDEX IF EXISTS search_vectors_hnsw".to_string(),
            format!(
                "ALTER TABLE search_vectors ALTER COLUMN embedding \
                 TYPE halfvec({target}) USING embedding::halfvec({target})"
            ),
            format!(
                "ALTER TABLE search_topic_cache ALTER COLUMN embedding \
                 TYPE halfvec({target}) USING embedding::halfvec({target})"
            ),
            "CREATE INDEX search_vectors_hnsw ON search_vectors \
             USING hnsw (embedding halfvec_cosine_ops)"
                .to_string(),
        ] {
            sqlx::query(&stmt)
                .execute(&self.pool)
                .await
                .map_err(|e| Error::Database(format!("resizing embedding columns: {e}")))?;
        }
        Ok(())
    }

    /// The formatted SQL type of a column, e.g. `"vector(256)"` or
    /// `"halfvec(384)"`. `None` if the column isn't found.
    async fn vector_column_type(&self, table: &str, col: &str) -> Result<Option<String>> {
        sqlx::query_scalar(
            "SELECT format_type(a.atttypid, a.atttypmod) \
             FROM pg_attribute a \
             WHERE a.attrelid = $1::regclass AND a.attname = $2 AND NOT a.attisdropped",
        )
        .bind(table)
        .bind(col)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Database(format!("reading {table}.{col} type: {e}")))
    }

    /// The declared dimension of a pgvector column, e.g. `vector(256)` → 256.
    /// `None` if the column has no dimension modifier or isn't found.
    #[allow(dead_code)]
    async fn vector_column_dim(&self, table: &str, col: &str) -> Result<Option<usize>> {
        Ok(self.vector_column_type(table, col).await?.as_deref().and_then(parse_vector_dim))
    }

    /// Poll `SELECT 1` until Postgres responds or we exhaust the budget.
    /// One log line per second so `journalctl -u virtues` shows progress
    /// instead of a wall of silence.
    ///
    /// Permanent errors fail IMMEDIATELY: auth/role/database errors don't fix
    /// themselves by waiting, and burning the 30s budget on them used to bury
    /// the real cause ("peer authentication failed for user adam") under a
    /// misleading "did not accept connections within 30s" timeout. The classic
    /// trigger is running a CLI command as the wrong OS user on a box install
    /// (Unix socket + peer auth maps OS user → Postgres role), so the error
    /// carries that hint.
    async fn wait_for_postgres(&self, budget: std::time::Duration) -> Result<()> {
        let start = std::time::Instant::now();
        let mut emitted_waiting = false;
        loop {
            match sqlx::query("SELECT 1").execute(&self.pool).await {
                Ok(_) => return Ok(()),
                Err(e) => {
                    let msg = e.to_string();
                    // Auth/identity failures are permanent — retrying is noise.
                    let permanent = [
                        "peer authentication failed",
                        "password authentication failed",
                        "does not exist", // role "adam" / database "virtues" does not exist
                        "no pg_hba.conf entry",
                    ]
                    .iter()
                    .any(|p| msg.contains(p));
                    if permanent {
                        return Err(Error::Database(format!(
                            "Postgres refused the connection: {msg}\n  \
                             hint: on a box install, CLI commands must run as the \
                             service user — try: sudo -u virtues virtues <command>"
                        )));
                    }
                    if start.elapsed() >= budget {
                        return Err(Error::Database(format!(
                            "Postgres did not accept connections within {}s: {e}",
                            budget.as_secs()
                        )));
                    }
                    if !emitted_waiting {
                        tracing::info!("waiting for postgres to accept connections…");
                        emitted_waiting = true;
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }
    }

    /// Run database migrations
    async fn run_migrations(&self) -> Result<()> {
        let mut migrator = sqlx::migrate!("./migrations");
        // One dev database serves many branches: a migration applied on a
        // feature branch must not brick `make dev` on branches that don't
        // carry its file. Skip applied-but-unknown versions instead.
        migrator.set_ignore_missing(true);
        migrator
            .run(&self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to run migrations: {e}")))?;
        Ok(())
    }

    /// Execute a query with parameters
    pub async fn execute(&self, sql: &str, params: &[&str]) -> Result<()> {
        let mut query = sqlx::query(sql);
        for param in params {
            query = query.bind(param);
        }
        query.execute(&self.pool).await?;
        Ok(())
    }

    /// Batch insert helper — builds a multi-row INSERT with $N placeholders
    /// and an `ON CONFLICT (...) DO NOTHING` clause. Caller binds parameters
    /// in row-major order.
    ///
    /// # Arguments
    /// * `table` — table name
    /// * `columns` — column names in order
    /// * `conflict_column` — single column for the conflict target
    /// * `num_rows` — number of rows in this batch
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

    /// Health check
    pub async fn health_check(&self) -> Result<HealthStatus> {
        match sqlx::query("SELECT 1").fetch_one(&self.pool).await {
            Ok(_) => Ok(HealthStatus {
                is_healthy: true,
                message: "Connected".to_string(),
            }),
            Err(e) => Ok(HealthStatus {
                is_healthy: false,
                message: format!("Connection failed: {e}"),
            }),
        }
    }
}

/// Health status for database
#[derive(Debug)]
pub struct HealthStatus {
    pub is_healthy: bool,
    pub message: String,
}

/// Parse the dimension out of a pgvector `format_type` string, e.g.
/// `vector(256)` → `Some(256)`, `halfvec(3072)` → `Some(3072)`, bare `vector`
/// (no modifier) → `None`.
fn parse_vector_dim(ty: &str) -> Option<usize> {
    let open = ty.find('(')?;
    let close = ty.find(')')?;
    ty.get(open + 1..close)?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_vector_dim_cases() {
        assert_eq!(parse_vector_dim("vector(256)"), Some(256));
        assert_eq!(parse_vector_dim("vector(768)"), Some(768));
        assert_eq!(parse_vector_dim("halfvec(3072)"), Some(3072));
        assert_eq!(parse_vector_dim("vector"), None);
        assert_eq!(parse_vector_dim("text"), None);
    }

    // Tests go through the pure `normalize_from`, never the env: set_var/
    // remove_var here raced the #[sqlx::test] suites (env is process-global,
    // tests run concurrently across modules) — sqlx's tamper guard saw
    // DATABASE_URL change mid-run and panicked.

    #[test]
    fn test_normalize_returns_url() {
        let normalized =
            normalize_from(Some("postgres://user:pass@localhost:5432/db".to_string())).unwrap();
        assert_eq!(normalized, "postgres://user:pass@localhost:5432/db");
    }

    #[test]
    fn test_normalize_errors_when_unset() {
        assert!(normalize_from(None).is_err());
    }
}

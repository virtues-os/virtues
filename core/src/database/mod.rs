//! Database module for SQLite operations

use std::sync::Once;
use std::time::Duration;

use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

use crate::error::{Error, Result};

/// Register the sqlite-vec extension globally (once).
///
/// This must be called before any SQLite connections are created.
/// The extension is registered via `sqlite3_auto_extension` so every
/// connection automatically gets vec0 virtual table support.
fn register_sqlite_vec_extension() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        unsafe {
            libsqlite3_sys::sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }
        tracing::info!("sqlite-vec extension registered");
    });
}

/// Database connection and operations
#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Create a new database connection
    pub fn new(database_url: &str) -> Result<Self> {
        // Ensure sqlite-vec is registered before creating the pool
        register_sqlite_vec_extension();
        // Get max connections from environment (default: 5 for SQLite)
        let max_connections = std::env::var("DATABASE_MAX_CONNECTIONS")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(5);

        tracing::info!("Database pool max connections: {}", max_connections);

        // Pool will be created on first use
        let pool = SqlitePoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(Duration::from_secs(10))
            .idle_timeout(Duration::from_secs(600))
            .max_lifetime(Duration::from_secs(1800))
            .after_connect(|conn, _meta| {
                Box::pin(async move {
                    // Enable SQLite-specific settings for better performance and safety
                    sqlx::query("PRAGMA foreign_keys = ON")
                        .execute(&mut *conn)
                        .await?;
                    sqlx::query("PRAGMA journal_mode = WAL")
                        .execute(&mut *conn)
                        .await?;
                    sqlx::query("PRAGMA busy_timeout = 5000")
                        .execute(&mut *conn)
                        .await?;
                    sqlx::query("PRAGMA synchronous = NORMAL")
                        .execute(&mut *conn)
                        .await?;
                    Ok(())
                })
            })
            .connect_lazy(database_url)?;

        Ok(Self { pool })
    }

    /// Create from an existing pool
    pub fn from_pool(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Get the underlying connection pool
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Initialize database (run migrations, etc.)
    pub async fn initialize(&self) -> Result<()> {
        // Test connection
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to connect: {e}")))?;

        // Run migrations
        self.run_migrations().await?;

        Ok(())
    }

    /// Run database migrations
    async fn run_migrations(&self) -> Result<()> {
        // Use sqlx migrate to run migrations from the migrations folder
        sqlx::migrate!("./migrations")
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

    /// Batch insert helper - builds multi-row INSERT query with proper parameter binding
    ///
    /// This is a helper that returns the SQL query string with placeholders.
    /// Individual transforms should use this to build their batch insert queries.
    ///
    /// # Arguments
    /// * `table` - Table name (e.g., "data_location_point")
    /// * `columns` - Column names in order
    /// * `conflict_column` - Column for ON CONFLICT DO NOTHING (e.g., "source_stream_id")
    /// * `num_rows` - Number of rows to insert in this batch
    ///
    /// # Returns
    /// SQL query string with placeholders ($1, $2, $3, ...) - works with SQLite via sqlx
    ///
    /// # Example
    /// ```ignore
    /// let query_str = db.build_batch_insert_query(
    ///     "data_location_point",
    ///     &["coordinates", "latitude", "longitude"],
    ///     "source_stream_id",
    ///     100, // 100 rows
    /// );
    /// // Returns: INSERT INTO data_location_point (coordinates, latitude, longitude)
    /// //          VALUES ($1, $2, $3), ($4, $5, $6), ...
    /// //          ON CONFLICT (source_stream_id) DO NOTHING
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

        // Build VALUES clauses: ($1, $2, $3), ($4, $5, $6), ...
        // Note: SQLite via sqlx supports $N style parameters
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
        // SQLite uses same ON CONFLICT syntax as PostgreSQL
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_database_creation() {
        // Use in-memory SQLite for testing
        let result = Database::new("sqlite::memory:");
        assert!(result.is_ok());
    }
}

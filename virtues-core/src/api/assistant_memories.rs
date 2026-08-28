//! The machine's memory, as a surface the person can read and edit.
//!
//! `app_assistant_memories` is what the `<memory>` prompt block renders and
//! what the `update_memory` tool writes (executor.rs). This module is the
//! HUMAN side: list, edit, retire. The doctrine that shaped it
//! (docs/narrative-identity.md): a machine channel about the person that the
//! person cannot see is the disease the observed-data portrait was deleted
//! for — visibility is not a feature of this table, it is the reason the
//! table exists.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::error::{Error, Result};

/// Per-lane ceilings. A full lane refuses new notes until one is retired —
/// budget pressure IS the forgetting mechanism, and an unbounded
/// machine-written block is the thing that quietly eats the prompt.
pub fn lane_cap(lane: &str) -> usize {
    match lane {
        "facts" => 12,
        _ => 8, // manner, practices
    }
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AssistantMemory {
    pub id: i64,
    pub lane: String,
    pub body: String,
    pub author: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Live memories, lane-grouped order. The stable ORDER BY is load-bearing:
/// this list also renders into the prompt, and unstable bytes kill caching.
pub async fn list_memories(pool: &PgPool) -> Result<Vec<AssistantMemory>> {
    sqlx::query_as::<_, AssistantMemory>(
        "SELECT id, lane, body, author, created_at, updated_at \
         FROM app_assistant_memories \
         WHERE retired_at IS NULL \
         ORDER BY lane, created_at, id",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("list assistant memories: {e}")))
}

#[derive(Debug, Deserialize)]
pub struct EditMemoryRequest {
    pub body: String,
}

/// The person rewrites a memory in their own words. Author flips to 'human':
/// from then on it reads as theirs, and provenance says so.
pub async fn edit_memory(pool: &PgPool, id: i64, body: &str) -> Result<AssistantMemory> {
    let body = body.trim();
    if body.is_empty() || body.chars().count() > 500 {
        return Err(Error::InvalidInput(
            "a memory is 1–500 characters".into(),
        ));
    }
    sqlx::query_as::<_, AssistantMemory>(
        "UPDATE app_assistant_memories \
         SET body = $2, author = 'human', updated_at = now() \
         WHERE id = $1 AND retired_at IS NULL \
         RETURNING id, lane, body, author, created_at, updated_at",
    )
    .bind(id)
    .bind(body)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("edit assistant memory: {e}")))?
    .ok_or_else(|| Error::NotFound(format!("memory {id}")))
}

/// The person removes a memory. Soft: the row keeps its reason, so what the
/// machine once believed — and when it stopped — stays auditable.
pub async fn retire_memory(pool: &PgPool, id: i64) -> Result<()> {
    let n = sqlx::query(
        "UPDATE app_assistant_memories \
         SET retired_at = now(), retired_reason = 'user_removed' \
         WHERE id = $1 AND retired_at IS NULL",
    )
    .bind(id)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("retire assistant memory: {e}")))?
    .rows_affected();
    if n == 0 {
        return Err(Error::NotFound(format!("memory {id}")));
    }
    Ok(())
}

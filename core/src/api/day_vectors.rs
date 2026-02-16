//! Day vector projection endpoint
//!
//! Reads the 7 W6H embeddings for a day, computes an aggregate (mean),
//! and projects all 8 from 768-dim → 2D via a fixed seeded random projection.
//! This powers the ContextVectorHero visualization on the day page.

use serde::Serialize;
use sqlx::SqlitePool;

use crate::api::day_scoring::{bytes_to_embedding, W6H_DIMENSIONS};
use crate::error::{Error, Result};

/// 2D projection of a day's W6H embeddings.
#[derive(Debug, Serialize)]
pub struct DayVectorProjection {
    pub date: String,
    pub aggregate: [f32; 2],
    pub dimensions: Vec<W6HProjection>,
}

/// A single W6H dimension projected to 2D.
#[derive(Debug, Serialize)]
pub struct W6HProjection {
    pub name: String,
    pub point: [f32; 2],
    pub magnitude: f32,
    pub completeness: f32,
}

/// Get the 2D vector projection for a day's embeddings.
///
/// Returns None if the day has no embeddings stored.
pub async fn get_day_vector_projection(
    pool: &SqlitePool,
    date: &str,
) -> Result<Option<DayVectorProjection>> {
    use sqlx::Row;

    // Fetch all W6H embeddings for this day
    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT dimension, embedding FROM wiki_day_embeddings \
         WHERE day_date = $1 ORDER BY dimension",
    )
    .bind(date)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch day embeddings: {e}")))?;

    if rows.is_empty() {
        return Ok(None);
    }

    // Parse embeddings
    let mut dim_embeddings: Vec<(String, Vec<f32>)> = Vec::new();
    for row in &rows {
        let dim: String = row
            .try_get("dimension")
            .map_err(|e| Error::Database(format!("Failed to read dimension: {e}")))?;
        let bytes: Vec<u8> = row
            .try_get("embedding")
            .map_err(|e| Error::Database(format!("Failed to read embedding: {e}")))?;
        dim_embeddings.push((dim, bytes_to_embedding(&bytes)));
    }

    // Fetch context_vector from wiki_days for completeness values
    let context_vector = get_context_vector(pool, date).await?;

    // Compute aggregate embedding (mean of all dimension embeddings)
    let embedding_dim = dim_embeddings[0].1.len();
    let mut aggregate = vec![0.0f32; embedding_dim];
    for (_, emb) in &dim_embeddings {
        for (i, val) in emb.iter().enumerate() {
            if i < aggregate.len() {
                aggregate[i] += val;
            }
        }
    }
    let count = dim_embeddings.len() as f32;
    for val in &mut aggregate {
        *val /= count;
    }

    // Project aggregate to 2D
    let aggregate_2d = project_to_2d(&aggregate);

    // Project each dimension to 2D
    let mut dimensions = Vec::new();
    for (dim_name, emb) in &dim_embeddings {
        let point = project_to_2d(emb);
        let magnitude = l2_norm(emb);

        // Look up completeness from context_vector
        let completeness = W6H_DIMENSIONS
            .iter()
            .position(|&d| d == dim_name)
            .and_then(|idx| context_vector.get(idx).copied())
            .unwrap_or(0.0);

        dimensions.push(W6HProjection {
            name: dim_name.clone(),
            point,
            magnitude,
            completeness,
        });
    }

    Ok(Some(DayVectorProjection {
        date: date.to_string(),
        aggregate: aggregate_2d,
        dimensions,
    }))
}

/// Fetch the context_vector [f32; 7] from wiki_days for a given date.
async fn get_context_vector(pool: &SqlitePool, date: &str) -> Result<Vec<f32>> {
    let cv_json: Option<String> = sqlx::query_scalar(
        "SELECT context_vector FROM wiki_days WHERE date = $1",
    )
    .bind(date)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch context_vector: {e}")))?
    .flatten();

    match cv_json {
        Some(json_str) => {
            let parsed: serde_json::Value =
                serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Null);
            let mut vec = Vec::with_capacity(7);
            for &dim in W6H_DIMENSIONS {
                vec.push(parsed[dim].as_f64().unwrap_or(0.0) as f32);
            }
            Ok(vec)
        }
        None => Ok(vec![0.0; 7]),
    }
}

/// Project a high-dimensional vector to 2D using a fixed random projection matrix.
///
/// Uses a deterministic seeded PRNG to generate a 768×2 Gaussian random matrix.
/// The two projection vectors are unit-normalized, giving a Johnson-Lindenstrauss
/// style distance-preserving projection.
fn project_to_2d(embedding: &[f32]) -> [f32; 2] {
    let proj = get_projection_matrix(embedding.len());
    let mut result = [0.0f32; 2];
    for (i, val) in embedding.iter().enumerate() {
        result[0] += val * proj[i];
        result[1] += val * proj[embedding.len() + i];
    }
    result
}

/// L2 norm of a vector.
fn l2_norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

/// Get or lazily compute the projection matrix for a given dimension.
///
/// Returns a flat array of [row0..., row1...] where each row is `dim` elements.
/// Both rows are unit-normalized Gaussian random vectors seeded deterministically.
fn get_projection_matrix(dim: usize) -> Vec<f32> {
    use std::sync::OnceLock;
    static PROJ_768: OnceLock<Vec<f32>> = OnceLock::new();

    if dim == 768 {
        PROJ_768
            .get_or_init(|| generate_projection_matrix(768))
            .clone()
    } else {
        generate_projection_matrix(dim)
    }
}

/// Generate a 2×dim projection matrix using a seeded xorshift PRNG
/// with Box-Muller transform for Gaussian values.
fn generate_projection_matrix(dim: usize) -> Vec<f32> {
    let mut matrix = vec![0.0f32; 2 * dim];
    let mut rng_state: u64 = 0x5EED_CAFE_BABE_1337; // fixed seed

    // Generate Gaussian random values via Box-Muller
    let total = 2 * dim;
    let mut i = 0;
    while i < total {
        // Generate two uniform random numbers in (0, 1)
        let u1 = xorshift_uniform(&mut rng_state);
        let u2 = xorshift_uniform(&mut rng_state);

        // Box-Muller transform
        let r = (-2.0 * u1.ln()).sqrt();
        let theta = 2.0 * std::f32::consts::PI * u2;
        matrix[i] = r * theta.cos();
        if i + 1 < total {
            matrix[i + 1] = r * theta.sin();
        }
        i += 2;
    }

    // Normalize each row to unit length
    for row in 0..2 {
        let start = row * dim;
        let end = start + dim;
        let norm: f32 = matrix[start..end].iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for val in &mut matrix[start..end] {
                *val /= norm;
            }
        }
    }

    matrix
}

/// Xorshift64 PRNG returning a uniform f32 in (0, 1).
fn xorshift_uniform(state: &mut u64) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    // Map to (0, 1) — avoid exact 0 for Box-Muller log
    ((*state >> 11) as f32 / (1u64 << 53) as f32).max(f32::EPSILON)
}

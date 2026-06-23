//! Shared embedding-space primitives for dayline scoring.
//!
//! Consolidates helpers that `novelty.rs` and `autonomic_scoring.rs` each used
//! to define privately (cosine, byte (de)serialization) plus the kNN neighbor
//! primitive that local novelty (LOF) — and, later, conditional novelty —
//! build on. One place to optimize (e.g. swap brute force for an ANN index if
//! the baseline ever outgrows it) and one definition to keep correct.

/// Deserialize little-endian BYTEA into an f32 embedding. Returns empty on a
/// non-multiple-of-4 blob — a truncated row should degrade, not panic.
pub fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    if bytes.len() % 4 != 0 {
        return Vec::new();
    }
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Serialize an f32 embedding to little-endian bytes for BYTEA storage.
pub fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Cosine similarity in [-1, 1]. Returns 0.0 if either vector is zero-length
/// or all-zero (→ cosine_distance 1.0, the neutral "unrelated" distance).
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let mut dot = 0.0f64;
    let mut norm_a = 0.0f64;
    let mut norm_b = 0.0f64;
    for i in 0..a.len().min(b.len()) {
        let av = a[i] as f64;
        let bv = b[i] as f64;
        dot += av * bv;
        norm_a += av * av;
        norm_b += bv * bv;
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom < 1e-12 {
        0.0
    } else {
        (dot / denom).clamp(-1.0, 1.0)
    }
}

/// Cosine distance = 1 - cosine_similarity. Range [0, 2].
pub fn cosine_distance(a: &[f32], b: &[f32]) -> f64 {
    1.0 - cosine_similarity(a, b)
}

/// The `k` nearest candidates to `query` by cosine distance, as (index,
/// distance) sorted nearest-first. `skip` excludes one index — used when a
/// baseline point scores within its own set so it doesn't match itself.
///
/// Brute force O(n·dim) + O(n log n); fine for personal-scale baselines.
pub fn k_nearest(
    query: &[f32],
    candidates: &[Vec<f32>],
    k: usize,
    skip: Option<usize>,
) -> Vec<(usize, f64)> {
    let mut dists: Vec<(usize, f64)> = candidates
        .iter()
        .enumerate()
        .filter(|(i, _)| skip != Some(*i))
        .map(|(i, c)| (i, cosine_distance(query, c)))
        .collect();
    dists.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    dists.truncate(k);
    dists
}

/// The text actually fed to the embedder for an event. Centralized so the
/// planned "anchor-stripped content clause" upgrade (embed WHAT/HOW only,
/// leaving WHO/WHERE/WHEN to conditioning) is a one-function change. Today it
/// is the event summary verbatim.
pub fn embed_input_for_event(summary: &str) -> String {
    summary.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_identical_and_orthogonal() {
        assert!((cosine_distance(&[1.0, 0.0, 0.0], &[1.0, 0.0, 0.0])).abs() < 1e-9);
        assert!((cosine_distance(&[1.0, 0.0], &[0.0, 1.0]) - 1.0).abs() < 1e-9);
        assert!((cosine_distance(&[1.0, 0.0], &[-1.0, 0.0]) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn cosine_zero_vector_is_neutral() {
        assert!((cosine_distance(&[1.0, 2.0], &[0.0, 0.0]) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn bytes_roundtrip_and_truncation() {
        let v = vec![1.0f32, -2.5, 3.14159, 0.0];
        assert_eq!(bytes_to_embedding(&embedding_to_bytes(&v)), v);
        assert!(bytes_to_embedding(&[0u8, 1, 2]).is_empty()); // not a multiple of 4
    }

    #[test]
    fn k_nearest_orders_and_skips() {
        let cands = vec![
            vec![1.0, 0.0],  // 0: identical to query
            vec![0.0, 1.0],  // 1: orthogonal
            vec![-1.0, 0.0], // 2: opposite
        ];
        let nn = k_nearest(&[1.0, 0.0], &cands, 2, None);
        assert_eq!(nn[0].0, 0);
        assert_eq!(nn[1].0, 1);

        // skipping self surfaces the next-nearest
        let nn = k_nearest(&[1.0, 0.0], &cands, 1, Some(0));
        assert_eq!(nn[0].0, 1);
    }
}

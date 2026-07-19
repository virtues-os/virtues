//! Offline changepoint detection over a multivariate time series.
//!
//! Finds the boundaries where a signal's regime shifts — the moments the day
//! changed context. It is the shared primitive under every sessionizer: audio
//! (loudness + speaker count), and later iMessage (message cadence).
//!
//! # Why exact, not PELT
//!
//! PELT is the fast (O(n)) version of **optimal partitioning**, and its speed only
//! matters at large n. A day is a few hundred points, so we run the exact O(n²)
//! partitioning directly — no pruning, no approximation, no Python. Same answer
//! PELT converges to, in microseconds.
//!
//! # The model
//!
//! Piecewise-constant mean with an L2 cost: each segment is scored by the sum of
//! squared deviations from its own per-dimension mean, and the total is minimised
//! subject to a `penalty` charged per boundary. Low penalty → more segments; high
//! → fewer. The features are expected pre-normalised (z-scored) so the dimensions
//! are comparable and one loud channel does not dominate.
//!
//! **Recall over precision is the caller's job**, set through `penalty`: these
//! boundaries are clues for a downstream detective that can merge but never
//! un-split, so err toward more.

/// Boundaries in `series` (each row a feature vector, all rows the same width).
///
/// Returns the 0-based indices where a new segment *starts* — never `0`, never
/// `series.len()`. An empty result means the whole series is one segment.
///
/// `penalty` is the cost charged per boundary, in the same units as the squared
/// deviations; it is the one tuning knob (the recall/precision dial). `min_size`
/// forbids segments shorter than it (≥1).
pub fn detect(series: &[Vec<f64>], penalty: f64, min_size: usize) -> Vec<usize> {
    let n = series.len();
    let min_size = min_size.max(1);
    if n <= min_size {
        return Vec::new();
    }
    let dim = series[0].len();

    // Prefix sums of x and x² per dimension, so a segment's SSE is O(dim), not
    // O(len). `pre[t]` covers rows [0, t).
    let mut sum = vec![vec![0.0f64; dim]; n + 1];
    let mut sq = vec![vec![0.0f64; dim]; n + 1];
    for t in 0..n {
        for d in 0..dim {
            sum[t + 1][d] = sum[t][d] + series[t][d];
            sq[t + 1][d] = sq[t][d] + series[t][d] * series[t][d];
        }
    }
    // Sum of squared deviations from the mean over [a, b).
    let seg_cost = |a: usize, b: usize| -> f64 {
        let len = (b - a) as f64;
        let mut c = 0.0;
        for d in 0..dim {
            let s = sum[b][d] - sum[a][d];
            let q = sq[b][d] - sq[a][d];
            c += q - s * s / len; // Σx² − (Σx)²/n
        }
        c.max(0.0)
    };

    // Optimal partitioning DP. `f[t]` = best cost to partition [0, t).
    // `prev[t]` = the last boundary before t on the optimal path.
    let inf = f64::INFINITY;
    let mut f = vec![inf; n + 1];
    let mut prev = vec![0usize; n + 1];
    f[0] = -penalty; // so the first segment is not charged a boundary
    for t in min_size..=n {
        for s in 0..=t.saturating_sub(min_size) {
            if f[s].is_finite() && (t - s) >= min_size {
                let cand = f[s] + seg_cost(s, t) + penalty;
                if cand < f[t] {
                    f[t] = cand;
                    prev[t] = s;
                }
            }
        }
    }

    // Backtrack the boundaries (drop the trivial 0 and n).
    let mut bounds = Vec::new();
    let mut t = n;
    while t > 0 {
        let s = prev[t];
        if s > 0 {
            bounds.push(s);
        }
        t = s;
    }
    bounds.reverse();
    bounds
}

/// Z-normalise one dimension across the series in place is awkward with the
/// row-major layout, so this helper builds a normalised copy: each column mean 0,
/// std 1 (std 0 → left at 0). `weights` scales columns after normalising, so a
/// caller can say "speaker identity matters more than loudness".
pub fn normalize(mut rows: Vec<Vec<f64>>, weights: &[f64]) -> Vec<Vec<f64>> {
    if rows.is_empty() {
        return rows;
    }
    let dim = rows[0].len();
    for d in 0..dim {
        let col: Vec<f64> = rows.iter().map(|r| r[d]).collect();
        let mean = col.iter().sum::<f64>() / col.len() as f64;
        let var = col.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / col.len() as f64;
        let std = var.sqrt();
        let w = weights.get(d).copied().unwrap_or(1.0);
        for r in rows.iter_mut() {
            r[d] = if std > 1e-9 { (r[d] - mean) / std * w } else { 0.0 };
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flat_series_has_no_boundaries() {
        let s = vec![vec![1.0], vec![1.0], vec![1.0], vec![1.0]];
        assert!(detect(&s, 1.0, 1).is_empty());
    }

    #[test]
    fn a_step_is_found_at_the_step() {
        // 0,0,0 then 5,5,5 — one boundary at index 3.
        let s = vec![
            vec![0.0], vec![0.0], vec![0.0], vec![5.0], vec![5.0], vec![5.0],
        ];
        assert_eq!(detect(&s, 1.0, 1), vec![3]);
    }

    #[test]
    fn penalty_controls_granularity() {
        // Three regimes: 0, 5, 0. Low penalty finds both edges; a huge penalty
        // finds none (one segment is cheaper than paying for boundaries).
        let s = vec![
            vec![0.0], vec![0.0], vec![5.0], vec![5.0], vec![0.0], vec![0.0],
        ];
        assert_eq!(detect(&s, 1.0, 1), vec![2, 4]);
        assert!(detect(&s, 1000.0, 1).is_empty());
    }

    #[test]
    fn the_audio_day_shape_segments_sensibly() {
        // The real pattern that started this: quiet writing (low db, 0 speakers),
        // then a conversation (louder, 2 speakers), then quiet again. [db, spk].
        let quiet = || vec![-42.0, 0.0];
        let talk = || vec![-22.0, 2.0];
        let mut rows = Vec::new();
        rows.extend(std::iter::repeat_with(quiet).take(4)); // writing
        rows.extend(std::iter::repeat_with(talk).take(3)); // conversation
        rows.extend(std::iter::repeat_with(quiet).take(4)); // quiet again
        // Speaker-weighted, as the audio sessionizer does.
        let norm = normalize(rows, &[0.7, 2.0]);
        let bounds = detect(&norm, 1.0, 1);
        // Two boundaries: writing→talk and talk→quiet.
        assert_eq!(bounds, vec![4, 7], "should cut on the conversation, not within it");
    }
}

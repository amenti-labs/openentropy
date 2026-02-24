//! Recurrence Quantification Analysis (RQA) for consciousness-RNG experiments.
//!
//! Recurrence plots visualize when a dynamical system returns to a previously
//! visited state. For truly random data, the recurrence matrix has uniformly
//! scattered dots with ZERO diagonal lines (determinism = 0). Any injection
//! of deterministic structure by consciousness would show as DET > 0.
//!
//! This provides a strict criterion: unlike Z-scores which suffer from
//! statistical power issues, the *absence* of structure in random data
//! is mathematical certainty.
//!
//! Based on Eckmann, Kamphorst & Ruelle (1987) and Marwan et al. (2007).

use serde::{Deserialize, Serialize};

/// Complete RQA metrics for a single data window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RQAResult {
    /// Recurrence rate: fraction of recurrent points.
    pub recurrence_rate: f64,
    /// Determinism: fraction of recurrence points forming diagonal lines.
    pub determinism: f64,
    /// Laminarity: fraction of recurrence points forming vertical lines.
    pub laminarity: f64,
    /// Trapping time: average length of vertical lines.
    pub trapping_time: f64,
    /// Longest diagonal line length.
    pub longest_diagonal: usize,
    /// Shannon entropy of diagonal line length distribution.
    pub diagonal_entropy: f64,
    /// Matrix size (N x N).
    pub matrix_size: usize,
    /// Parameters used.
    pub embedding_dim: usize,
    pub embedding_delay: usize,
    pub threshold: f64,
}

/// Comparison of RQA metrics between baseline and intention data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RQAComparison {
    /// Baseline RQA metrics.
    pub baseline: RQAResult,
    /// Intention RQA metrics.
    pub intention: RQAResult,
    /// Per-metric deltas (intention - baseline).
    pub deltas: Vec<(String, f64)>,
    /// Interpretation.
    pub interpretation: String,
}

/// Construct a delay-coordinate embedding from raw byte data.
fn delay_embed(data: &[u8], dim: usize, delay: usize) -> Vec<Vec<f64>> {
    let n = data.len();
    let max_idx = n.saturating_sub((dim - 1) * delay + 1);
    let mut points = Vec::with_capacity(max_idx);

    for i in 0..max_idx {
        let mut point = Vec::with_capacity(dim);
        for d in 0..dim {
            point.push(data[i + d * delay] as f64);
        }
        points.push(point);
    }

    points
}

/// Euclidean distance between two points.
fn euclidean_distance(a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f64>()
        .sqrt()
}

/// Construct the binary recurrence matrix.
///
/// R[i][j] = 1 iff dist(x_i, x_j) < threshold.
pub fn recurrence_matrix(data: &[u8], dim: usize, delay: usize, threshold: f64) -> Vec<Vec<bool>> {
    let points = delay_embed(data, dim, delay);
    let n = points.len();
    let mut rm = vec![vec![false; n]; n];

    for i in 0..n {
        for j in i..n {
            let dist = euclidean_distance(&points[i], &points[j]);
            if dist < threshold {
                rm[i][j] = true;
                rm[j][i] = true;
            }
        }
    }

    rm
}

/// Recurrence rate: fraction of recurrent points (excluding main diagonal).
pub fn recurrence_rate(rm: &[Vec<bool>]) -> f64 {
    let n = rm.len();
    if n < 2 {
        return 0.0;
    }
    let mut count = 0usize;
    for i in 0..n {
        for j in (i + 1)..n {
            if rm[i][j] {
                count += 1;
            }
        }
    }
    let total = n * (n - 1) / 2;
    if total == 0 {
        0.0
    } else {
        count as f64 / total as f64
    }
}

/// Count diagonal line lengths (lines parallel to the main diagonal).
fn diagonal_lines(rm: &[Vec<bool>], min_len: usize) -> Vec<usize> {
    let n = rm.len();
    let mut lengths = Vec::new();

    // Scan all diagonals above the main diagonal
    for offset in 1..n {
        let mut current_len = 0;
        for i in 0..(n - offset) {
            if rm[i][i + offset] {
                current_len += 1;
            } else {
                if current_len >= min_len {
                    lengths.push(current_len);
                }
                current_len = 0;
            }
        }
        if current_len >= min_len {
            lengths.push(current_len);
        }
    }

    lengths
}

/// Count vertical line lengths.
fn vertical_lines(rm: &[Vec<bool>], min_len: usize) -> Vec<usize> {
    let n = rm.len();
    let mut lengths = Vec::new();

    for j in 0..n {
        let mut current_len = 0;
        for i in 0..n {
            if rm[i][j] {
                current_len += 1;
            } else {
                if current_len >= min_len {
                    lengths.push(current_len);
                }
                current_len = 0;
            }
        }
        if current_len >= min_len {
            lengths.push(current_len);
        }
    }

    lengths
}

/// Determinism: fraction of recurrence points forming diagonal lines of
/// length >= min_len.
pub fn determinism(rm: &[Vec<bool>], min_len: usize) -> f64 {
    let n = rm.len();
    if n < 2 {
        return 0.0;
    }

    let total_recurrence: usize = (0..n)
        .flat_map(|i| ((i + 1)..n).map(move |j| (i, j)))
        .filter(|&(i, j)| rm[i][j])
        .count();

    if total_recurrence == 0 {
        return 0.0;
    }

    let diag_lengths = diagonal_lines(rm, min_len);
    let diag_points: usize = diag_lengths.iter().sum();

    diag_points as f64 / total_recurrence as f64
}

/// Laminarity: fraction of recurrence points forming vertical lines of
/// length >= min_len.
pub fn laminarity(rm: &[Vec<bool>], min_len: usize) -> f64 {
    let n = rm.len();
    if n < 2 {
        return 0.0;
    }

    let total_recurrence: usize = (0..n)
        .flat_map(|i| (0..n).map(move |j| (i, j)))
        .filter(|&(i, j)| i != j && rm[i][j])
        .count();

    if total_recurrence == 0 {
        return 0.0;
    }

    let vert_lengths = vertical_lines(rm, min_len);
    let vert_points: usize = vert_lengths.iter().sum();

    vert_points as f64 / total_recurrence as f64
}

/// Trapping time: average length of vertical lines.
pub fn trapping_time(rm: &[Vec<bool>], min_len: usize) -> f64 {
    let vert_lengths = vertical_lines(rm, min_len);
    if vert_lengths.is_empty() {
        return 0.0;
    }
    vert_lengths.iter().sum::<usize>() as f64 / vert_lengths.len() as f64
}

/// Length of the longest diagonal line.
pub fn longest_diagonal(rm: &[Vec<bool>]) -> usize {
    let diag_lengths = diagonal_lines(rm, 1);
    diag_lengths.into_iter().max().unwrap_or(0)
}

/// Shannon entropy of the diagonal line length distribution.
pub fn diagonal_entropy(rm: &[Vec<bool>], min_len: usize) -> f64 {
    let lengths = diagonal_lines(rm, min_len);
    if lengths.is_empty() {
        return 0.0;
    }

    // Build histogram of line lengths
    let max_len = *lengths.iter().max().unwrap_or(&0);
    let mut histogram = vec![0usize; max_len + 1];
    for &l in &lengths {
        histogram[l] += 1;
    }

    let total = lengths.len() as f64;
    let mut entropy = 0.0;
    for &count in &histogram {
        if count > 0 {
            let p = count as f64 / total;
            entropy -= p * p.ln();
        }
    }

    entropy
}

/// Compute all RQA metrics for a data window.
pub fn compute_rqa(data: &[u8], dim: usize, delay: usize, threshold: f64) -> RQAResult {
    let rm = recurrence_matrix(data, dim, delay, threshold);
    let min_len = 2;

    RQAResult {
        recurrence_rate: recurrence_rate(&rm),
        determinism: determinism(&rm, min_len),
        laminarity: laminarity(&rm, min_len),
        trapping_time: trapping_time(&rm, min_len),
        longest_diagonal: longest_diagonal(&rm),
        diagonal_entropy: diagonal_entropy(&rm, min_len),
        matrix_size: rm.len(),
        embedding_dim: dim,
        embedding_delay: delay,
        threshold,
    }
}

/// Compare RQA metrics between baseline and intention data.
pub fn compare_rqa(baseline: &[u8], intention: &[u8]) -> RQAComparison {
    // Use conservative parameters suitable for short byte streams
    let dim = 3;
    let delay = 1;
    // Threshold: ~10% recurrence rate for random data
    let threshold = 30.0; // Euclidean distance in 3D byte space

    let bl = compute_rqa(baseline, dim, delay, threshold);
    let int = compute_rqa(intention, dim, delay, threshold);

    let deltas = vec![
        ("recurrence_rate".to_string(), int.recurrence_rate - bl.recurrence_rate),
        ("determinism".to_string(), int.determinism - bl.determinism),
        ("laminarity".to_string(), int.laminarity - bl.laminarity),
        ("trapping_time".to_string(), int.trapping_time - bl.trapping_time),
        ("longest_diagonal".to_string(), (int.longest_diagonal as f64) - (bl.longest_diagonal as f64)),
        ("diagonal_entropy".to_string(), int.diagonal_entropy - bl.diagonal_entropy),
    ];

    let sig_deltas: Vec<&(String, f64)> = deltas.iter().filter(|(_, d)| d.abs() > 0.05).collect();
    let interpretation = if sig_deltas.is_empty() {
        "No notable RQA differences between conditions. Both show expected random structure.".to_string()
    } else if int.determinism > bl.determinism + 0.1 {
        format!(
            "Determinism increased by {:.3} during intention — possible structural injection. \
             Random data should have DET near 0.",
            int.determinism - bl.determinism
        )
    } else {
        let changes: Vec<String> = sig_deltas
            .iter()
            .map(|(name, d)| format!("{name}: {:+.3}", d))
            .collect();
        format!("RQA changes detected: {}", changes.join(", "))
    };

    RQAComparison {
        baseline: bl,
        intention: int,
        deltas,
        interpretation,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_embed_basic() {
        let data = vec![1u8, 2, 3, 4, 5];
        let embedded = delay_embed(&data, 2, 1);
        // n=5, dim=2, delay=1 -> max_idx = 5 - (1*1+1) = 3 points
        assert_eq!(embedded.len(), 3);
        assert_eq!(embedded[0], vec![1.0, 2.0]);
        assert_eq!(embedded[2], vec![3.0, 4.0]);
    }

    #[test]
    fn delay_embed_with_delay() {
        let data = vec![1u8, 2, 3, 4, 5, 6];
        let embedded = delay_embed(&data, 2, 2);
        // n=6, dim=2, delay=2 -> max_idx = 6 - (1*2+1) = 3 points
        assert_eq!(embedded[0], vec![1.0, 3.0]);
        assert_eq!(embedded[1], vec![2.0, 4.0]);
    }

    #[test]
    fn recurrence_matrix_self_recurrence() {
        // All same values -> all points recur with each other
        let data = vec![100u8; 20];
        let rm = recurrence_matrix(&data, 2, 1, 1.0);
        // Every pair should be recurrent (distance = 0 < 1.0)
        for i in 0..rm.len() {
            for j in 0..rm.len() {
                assert!(rm[i][j], "rm[{i}][{j}] should be true");
            }
        }
    }

    #[test]
    fn recurrence_rate_constant() {
        let data = vec![100u8; 20];
        let rm = recurrence_matrix(&data, 2, 1, 1.0);
        let rr = recurrence_rate(&rm);
        assert!((rr - 1.0).abs() < 1e-10, "RR = {rr}");
    }

    #[test]
    fn determinism_constant_data() {
        // Constant data: all points recur, all form diagonal lines
        let data = vec![100u8; 30];
        let rm = recurrence_matrix(&data, 2, 1, 1.0);
        let det = determinism(&rm, 2);
        assert!(det > 0.5, "DET = {det}");
    }

    #[test]
    fn rqa_metrics_random_like() {
        // Pseudo-random data with small threshold should have moderate recurrence
        let data: Vec<u8> = (0..100).map(|i| ((i * 137 + 43) % 256) as u8).collect();
        let result = compute_rqa(&data, 2, 1, 20.0);
        // Verify metrics are computed without panicking
        assert!(result.recurrence_rate >= 0.0 && result.recurrence_rate <= 1.0);
        assert!(result.determinism >= 0.0 && result.determinism <= 1.0);
    }

    #[test]
    fn longest_diagonal_empty() {
        let data = vec![0u8; 5];
        let rm = recurrence_matrix(&data, 2, 1, 200.0);
        let ld = longest_diagonal(&rm);
        assert!(ld > 0); // Constant data has long diagonals
    }

    #[test]
    fn diagonal_entropy_computed() {
        let data = vec![100u8; 30];
        let rm = recurrence_matrix(&data, 2, 1, 1.0);
        let de = diagonal_entropy(&rm, 2);
        assert!(de >= 0.0);
    }

    #[test]
    fn compare_rqa_same_data() {
        let data: Vec<u8> = (0..100).map(|i| ((i * 97 + 31) % 256) as u8).collect();
        let result = compare_rqa(&data, &data);
        // Same data should produce zero deltas
        for (_, delta) in &result.deltas {
            assert!((delta).abs() < 1e-10, "non-zero delta: {delta}");
        }
    }

    #[test]
    fn compare_rqa_different_data() {
        let baseline: Vec<u8> = (0..100).map(|i| ((i * 97 + 31) % 256) as u8).collect();
        let intention: Vec<u8> = vec![100u8; 100]; // Constant = very structured
        let result = compare_rqa(&baseline, &intention);
        // Constant data should have max recurrence rate
        assert!(
            result.intention.recurrence_rate > 0.9,
            "RR = {}",
            result.intention.recurrence_rate
        );
    }
}

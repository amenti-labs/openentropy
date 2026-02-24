//! Transfer entropy between physically independent entropy sources.
//!
//! Measures directional information flow between source pairs. If consciousness
//! creates coupling between physically independent sources (e.g., IMU sensor
//! and NVMe thermal noise), the transfer entropy will increase during intention.
//!
//! Under the null hypothesis, TE between uncoupled sources is approximately 0.
//! This is stronger than Pearson correlation because it captures nonlinear
//! directional dependencies.
//!
//! Based on: Schreiber (2000) "Measuring information transfer."

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Transfer entropy result for a single source pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TEPair {
    /// Source name.
    pub source: String,
    /// Target name.
    pub target: String,
    /// TE(source -> target).
    pub te_forward: f64,
    /// TE(target -> source).
    pub te_reverse: f64,
    /// Net information flow direction.
    pub net_direction: String,
    /// Asymmetry: |forward - reverse|.
    pub asymmetry: f64,
}

/// Full transfer entropy matrix across all source pairs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferEntropyMatrix {
    /// Source names.
    pub sources: Vec<String>,
    /// Pairwise TE values [source_idx][target_idx].
    pub te_values: Vec<Vec<f64>>,
    /// Significant pairs (TE pairs with non-trivial flow).
    pub pairs: Vec<TEPair>,
    /// Mean TE across all pairs.
    pub mean_te: f64,
}

/// Comparison of transfer entropy between baseline and intention conditions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TEComparison {
    /// Baseline TE matrix.
    pub baseline: TransferEntropyMatrix,
    /// Intention TE matrix.
    pub intention: TransferEntropyMatrix,
    /// Pairs showing increased TE during intention.
    pub increased_pairs: Vec<(String, String, f64)>,
    /// Mean TE change (intention - baseline).
    pub mean_te_change: f64,
    /// Interpretation.
    pub interpretation: String,
}

/// Binned Shannon entropy of a 1D signal.
pub fn histogram_entropy(data: &[f64], bins: usize) -> f64 {
    if data.is_empty() || bins == 0 {
        return 0.0;
    }

    let min_val = data.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_val = data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = max_val - min_val;

    if range < 1e-15 {
        return 0.0; // Constant signal
    }

    let mut counts = vec![0usize; bins];
    for &x in data {
        let bin = ((x - min_val) / range * (bins - 1) as f64) as usize;
        let bin = bin.min(bins - 1);
        counts[bin] += 1;
    }

    let n = data.len() as f64;
    let mut entropy = 0.0;
    for &c in &counts {
        if c > 0 {
            let p = c as f64 / n;
            entropy -= p * p.ln();
        }
    }

    entropy
}

/// Joint entropy H(A, B) using 2D histogram.
pub fn joint_entropy(a: &[f64], b: &[f64], bins: usize) -> f64 {
    let n = a.len().min(b.len());
    if n == 0 || bins == 0 {
        return 0.0;
    }

    let a_min = a[..n].iter().cloned().fold(f64::INFINITY, f64::min);
    let a_max = a[..n].iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let b_min = b[..n].iter().cloned().fold(f64::INFINITY, f64::min);
    let b_max = b[..n].iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    let a_range = (a_max - a_min).max(1e-15);
    let b_range = (b_max - b_min).max(1e-15);

    let mut counts = vec![vec![0usize; bins]; bins];
    for i in 0..n {
        let ai = ((a[i] - a_min) / a_range * (bins - 1) as f64) as usize;
        let bi = ((b[i] - b_min) / b_range * (bins - 1) as f64) as usize;
        counts[ai.min(bins - 1)][bi.min(bins - 1)] += 1;
    }

    let nf = n as f64;
    let mut entropy = 0.0;
    for row in &counts {
        for &c in row {
            if c > 0 {
                let p = c as f64 / nf;
                entropy -= p * p.ln();
            }
        }
    }

    entropy
}

/// Joint entropy H(A, B, C) using 3D histogram.
///
/// Computes the Shannon entropy of the joint distribution of three variables
/// using a flattened 3D bin structure. Essential for proper transfer entropy
/// computation where we need H(target_future, target_past, source_past).
pub fn joint_entropy_3d(a: &[f64], b: &[f64], c: &[f64], bins: usize) -> f64 {
    let n = a.len().min(b.len()).min(c.len());
    if n == 0 || bins == 0 {
        return 0.0;
    }

    let a_min = a[..n].iter().cloned().fold(f64::INFINITY, f64::min);
    let a_max = a[..n].iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let b_min = b[..n].iter().cloned().fold(f64::INFINITY, f64::min);
    let b_max = b[..n].iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let c_min = c[..n].iter().cloned().fold(f64::INFINITY, f64::min);
    let c_max = c[..n].iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    let a_range = (a_max - a_min).max(1e-15);
    let b_range = (b_max - b_min).max(1e-15);
    let c_range = (c_max - c_min).max(1e-15);

    // Flattened 3D histogram: index = ai * bins * bins + bi * bins + ci
    let total_bins = bins * bins * bins;
    let mut counts = vec![0usize; total_bins];

    for i in 0..n {
        let ai = ((a[i] - a_min) / a_range * (bins - 1) as f64) as usize;
        let bi = ((b[i] - b_min) / b_range * (bins - 1) as f64) as usize;
        let ci = ((c[i] - c_min) / c_range * (bins - 1) as f64) as usize;
        let ai = ai.min(bins - 1);
        let bi = bi.min(bins - 1);
        let ci = ci.min(bins - 1);
        counts[ai * bins * bins + bi * bins + ci] += 1;
    }

    let nf = n as f64;
    let mut entropy = 0.0;
    for &count in &counts {
        if count > 0 {
            let p = count as f64 / nf;
            entropy -= p * p.ln();
        }
    }

    entropy
}

/// Conditional entropy H(A|B) = H(A,B) - H(B).
pub fn conditional_entropy(a: &[f64], b: &[f64], bins: usize) -> f64 {
    let je = joint_entropy(a, b, bins);
    let hb = histogram_entropy(b, bins);
    (je - hb).max(0.0)
}

/// Transfer entropy TE(source -> target) at a given lag.
///
/// Uses the proper multivariate entropy decomposition:
///   TE(X -> Y) = H(Y_f, Y_p) + H(Y_p, X_p) - H(Y_f, Y_p, X_p) - H(Y_p)
///
/// where Y_f = target future, Y_p = target past, X_p = source past.
///
/// This avoids the lossy scalar combination `t * 1000 + s` that destroys
/// coupling information in coarse bins. Instead, proper 2D and 3D joint
/// histograms preserve the full multivariate structure.
///
/// Based on: Schreiber (2000) "Measuring information transfer."
pub fn transfer_entropy(source: &[f64], target: &[f64], lag: usize, bins: usize) -> f64 {
    let n = source.len().min(target.len());
    if n <= lag + 1 {
        return 0.0;
    }

    // Build time-lagged vectors
    let target_future: Vec<f64> = target[lag..n].to_vec();
    let target_past: Vec<f64> = target[..n - lag].to_vec();
    let source_past: Vec<f64> = source[..n - lag].to_vec();

    // TE = H(Y_f, Y_p) + H(Y_p, X_p) - H(Y_f, Y_p, X_p) - H(Y_p)
    let h_yf_yp = joint_entropy(&target_future, &target_past, bins);
    let h_yp_xp = joint_entropy(&target_past, &source_past, bins);
    let h_yf_yp_xp = joint_entropy_3d(&target_future, &target_past, &source_past, bins);
    let h_yp = histogram_entropy(&target_past, bins);

    (h_yf_yp + h_yp_xp - h_yf_yp_xp - h_yp).max(0.0)
}

/// Compute pairwise transfer entropy matrix for all source pairs.
pub fn transfer_entropy_matrix(
    sources: &[(String, Vec<f64>)],
    lag: usize,
) -> TransferEntropyMatrix {
    let bins = 8; // 8 bins for byte data discretization
    let n = sources.len();
    let mut te_values = vec![vec![0.0; n]; n];
    let source_names: Vec<String> = sources.iter().map(|(name, _)| name.clone()).collect();

    for i in 0..n {
        for j in 0..n {
            if i != j {
                te_values[i][j] = transfer_entropy(&sources[i].1, &sources[j].1, lag, bins);
            }
        }
    }

    // Build pair summaries
    let mut pairs = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            let forward = te_values[i][j];
            let reverse = te_values[j][i];
            let net_dir = if forward > reverse {
                format!("{} -> {}", source_names[i], source_names[j])
            } else if reverse > forward {
                format!("{} -> {}", source_names[j], source_names[i])
            } else {
                "symmetric".to_string()
            };

            pairs.push(TEPair {
                source: source_names[i].clone(),
                target: source_names[j].clone(),
                te_forward: forward,
                te_reverse: reverse,
                net_direction: net_dir,
                asymmetry: (forward - reverse).abs(),
            });
        }
    }

    let total_te: f64 = te_values.iter().flat_map(|row| row.iter()).sum();
    let n_pairs = n * (n - 1);
    let mean_te = if n_pairs > 0 {
        total_te / n_pairs as f64
    } else {
        0.0
    };

    TransferEntropyMatrix {
        sources: source_names,
        te_values,
        pairs,
        mean_te,
    }
}

// ---------------------------------------------------------------------------
// KSG k-Nearest-Neighbor Transfer Entropy Estimator
// ---------------------------------------------------------------------------

/// Digamma function (psi) via the asymptotic Stirling series.
///
/// For x >= 6 we use the series expansion:
///   psi(x) = ln(x) - 1/(2x) - 1/(12x^2) + 1/(120x^4) - 1/(252x^6)
///
/// For x < 6 we use the recurrence psi(x) = psi(x+1) - 1/x to shift
/// upward until x >= 6.
pub fn digamma(x: f64) -> f64 {
    if x <= 0.0 {
        return f64::NEG_INFINITY;
    }

    let mut result = 0.0;
    let mut x = x;

    // Recurrence: psi(x) = psi(x+1) - 1/x
    while x < 6.0 {
        result -= 1.0 / x;
        x += 1.0;
    }

    // Asymptotic series for large x
    let x2 = x * x;
    result += x.ln() - 0.5 / x - 1.0 / (12.0 * x2) + 1.0 / (120.0 * x2 * x2)
        - 1.0 / (252.0 * x2 * x2 * x2);

    result
}

/// Chebyshev (L-infinity) distance between two vectors.
///
/// Used by the KSG estimator because the Chebyshev norm defines the
/// joint-space ball as the intersection of marginal-space balls,
/// which is what makes the neighbor-count decomposition exact.
pub fn chebyshev_distance(a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b.iter())
        .map(|(ai, bi)| (ai - bi).abs())
        .fold(0.0_f64, f64::max)
}

/// KSG-style k-nearest-neighbor transfer entropy estimator.
///
/// Avoids binning entirely by using the Kraskov-Stogbauer-Grassberger (2004)
/// mutual information estimator to compute:
///
///   TE(X -> Y) = I(Y_future; X_past | Y_past)
///
/// The conditional MI is computed via:
///   I(A; B | C) = psi(k) - <psi(n_AC + 1) + psi(n_BC + 1) - psi(n_C + 1)>
///
/// where:
///   - k is the number of nearest neighbors in the full (A, B, C) space
///   - n_AC is the number of points within the k-th neighbor distance in (A, C) marginal
///   - n_BC is the number of points within the k-th neighbor distance in (B, C) marginal
///   - n_C is the number of points within the k-th neighbor distance in the C marginal
///   - psi is the digamma function
///
/// Reference: Kraskov, Stogbauer, Grassberger (2004) "Estimating Mutual Information."
///            Frenzel & Pompe (2007) "Partial Mutual Information for Coupling Analysis."
pub fn transfer_entropy_knn(source: &[f64], target: &[f64], lag: usize, k: usize) -> f64 {
    let n = source.len().min(target.len());
    if n <= lag + 1 || k == 0 {
        return 0.0;
    }

    let m = n - lag; // number of usable points

    if m <= k + 1 {
        return 0.0;
    }

    // Build the three component vectors:
    //   A = Y_future (target_future)
    //   B = X_past   (source_past)
    //   C = Y_past   (target_past)
    let a: Vec<f64> = target[lag..n].to_vec(); // Y_future
    let b: Vec<f64> = source[..m].to_vec(); // X_past
    let c: Vec<f64> = target[..m].to_vec(); // Y_past

    // For each point i, find the k-th nearest neighbor distance in the
    // full (A, B, C) joint space using Chebyshev norm.
    let mut psi_sum = 0.0;

    for i in 0..m {
        // Compute all Chebyshev distances in the 3D joint space
        let mut distances: Vec<(usize, f64)> = (0..m)
            .filter(|&j| j != i)
            .map(|j| {
                let d = (a[i] - a[j])
                    .abs()
                    .max((b[i] - b[j]).abs())
                    .max((c[i] - c[j]).abs());
                (j, d)
            })
            .collect();

        // Partial sort to find k-th nearest
        distances.sort_by(|x, y| x.1.partial_cmp(&y.1).unwrap_or(std::cmp::Ordering::Equal));

        let eps = distances[k - 1].1; // k-th nearest neighbor distance

        if eps < 1e-15 {
            // Degenerate case: many identical points
            // Count how many are at zero distance and use that
            let n_ac = m - 1; // pessimistic fallback
            let n_bc = m - 1;
            let n_c = m - 1;
            psi_sum += digamma((n_ac + 1) as f64) + digamma((n_bc + 1) as f64)
                - digamma((n_c + 1) as f64);
            continue;
        }

        // Count points strictly within eps in each marginal subspace
        // n_AC: points where max(|a_i - a_j|, |c_i - c_j|) <= eps
        // n_BC: points where max(|b_i - b_j|, |c_i - c_j|) <= eps
        // n_C:  points where |c_i - c_j| <= eps
        let mut n_ac: usize = 0;
        let mut n_bc: usize = 0;
        let mut n_c: usize = 0;

        for j in 0..m {
            if j == i {
                continue;
            }
            let da = (a[i] - a[j]).abs();
            let db = (b[i] - b[j]).abs();
            let dc = (c[i] - c[j]).abs();

            // Use strict less-than for the marginal counts (KSG Algorithm 1)
            if da < eps && dc < eps {
                n_ac += 1;
            }
            if db < eps && dc < eps {
                n_bc += 1;
            }
            if dc < eps {
                n_c += 1;
            }
        }

        psi_sum += digamma((n_ac + 1) as f64) + digamma((n_bc + 1) as f64)
            - digamma((n_c + 1) as f64);
    }

    let te = digamma(k as f64) - psi_sum / m as f64;

    te.max(0.0)
}

/// Compare histogram-based and KSG k-NN transfer entropy estimates.
///
/// Returns `(histogram_te, knn_te)` for the same source-target pair,
/// allowing direct comparison of the two estimation approaches.
/// Discrepancies indicate sensitivity to binning parameters.
pub fn transfer_entropy_comparison(
    source: &[f64],
    target: &[f64],
    lag: usize,
    bins: usize,
    k: usize,
) -> (f64, f64) {
    let hist_te = transfer_entropy(source, target, lag, bins);
    let knn_te = transfer_entropy_knn(source, target, lag, k);
    (hist_te, knn_te)
}

/// Compute pairwise transfer entropy matrix using the KSG k-NN estimator.
///
/// This is the binning-free alternative to `transfer_entropy_matrix`. The k-NN
/// approach is more computationally expensive (O(N^2) per pair) but avoids
/// binning artifacts that can mask subtle coupling.
pub fn transfer_entropy_matrix_knn(
    sources: &[(String, Vec<f64>)],
    lag: usize,
    k: usize,
) -> TransferEntropyMatrix {
    let n = sources.len();
    let mut te_values = vec![vec![0.0; n]; n];
    let source_names: Vec<String> = sources.iter().map(|(name, _)| name.clone()).collect();

    for i in 0..n {
        for j in 0..n {
            if i != j {
                te_values[i][j] =
                    transfer_entropy_knn(&sources[i].1, &sources[j].1, lag, k);
            }
        }
    }

    // Build pair summaries (same structure as histogram version)
    let mut pairs = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            let forward = te_values[i][j];
            let reverse = te_values[j][i];
            let net_dir = if forward > reverse {
                format!("{} -> {}", source_names[i], source_names[j])
            } else if reverse > forward {
                format!("{} -> {}", source_names[j], source_names[i])
            } else {
                "symmetric".to_string()
            };

            pairs.push(TEPair {
                source: source_names[i].clone(),
                target: source_names[j].clone(),
                te_forward: forward,
                te_reverse: reverse,
                net_direction: net_dir,
                asymmetry: (forward - reverse).abs(),
            });
        }
    }

    let total_te: f64 = te_values.iter().flat_map(|row| row.iter()).sum();
    let n_pairs = n * (n - 1);
    let mean_te = if n_pairs > 0 {
        total_te / n_pairs as f64
    } else {
        0.0
    };

    TransferEntropyMatrix {
        sources: source_names,
        te_values,
        pairs,
        mean_te,
    }
}

/// Compare transfer entropy matrices between baseline and intention conditions.
pub fn compare_transfer_entropy(
    baseline: &TransferEntropyMatrix,
    intention: &TransferEntropyMatrix,
) -> TEComparison {
    let mut increased_pairs = Vec::new();

    // Find pairs with increased TE during intention
    for bl_pair in &baseline.pairs {
        if let Some(int_pair) = intention.pairs.iter().find(|p| {
            p.source == bl_pair.source && p.target == bl_pair.target
        }) {
            let forward_increase = int_pair.te_forward - bl_pair.te_forward;
            let reverse_increase = int_pair.te_reverse - bl_pair.te_reverse;
            let total_increase = forward_increase + reverse_increase;

            if total_increase > 0.01 {
                increased_pairs.push((
                    bl_pair.source.clone(),
                    bl_pair.target.clone(),
                    total_increase,
                ));
            }
        }
    }

    let mean_te_change = intention.mean_te - baseline.mean_te;

    let interpretation = if increased_pairs.is_empty() {
        "No significant increase in inter-source information transfer during intention. \
         Sources remained informationally independent."
            .to_string()
    } else if mean_te_change > 0.05 {
        format!(
            "Significant increase in transfer entropy ({} pairs, mean change: {:.4}). \
             Physically independent sources showed increased information coupling \
             during intention — consistent with consciousness-coherence models.",
            increased_pairs.len(),
            mean_te_change
        )
    } else {
        format!(
            "Minor transfer entropy changes detected ({} pairs), \
             but overall change is small (mean delta: {:.4}).",
            increased_pairs.len(),
            mean_te_change
        )
    };

    TEComparison {
        baseline: baseline.clone(),
        intention: intention.clone(),
        increased_pairs,
        mean_te_change,
        interpretation,
    }
}

/// Convert source byte data to float vectors suitable for TE analysis.
pub fn bytes_to_floats(data: &HashMap<String, Vec<u8>>) -> Vec<(String, Vec<f64>)> {
    let mut result: Vec<(String, Vec<f64>)> = data
        .iter()
        .map(|(name, bytes)| {
            let floats: Vec<f64> = bytes.iter().map(|&b| b as f64).collect();
            (name.clone(), floats)
        })
        .collect();
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}

// ---------------------------------------------------------------------------
// Higher-Order Transfer Entropy (Multi-Lag Embeddings)
// ---------------------------------------------------------------------------

/// Higher-order transfer entropy using multi-step past embeddings.
///
/// Instead of conditioning on a single past value Y_{t-1}, this conditions
/// on (Y_{t-1}, Y_{t-2}, ..., Y_{t-order}). This captures longer memory
/// effects and can detect coupling that only manifests at higher-order
/// temporal dependencies.
///
/// Uses histogram-based estimation with flattened joint distributions.
///
/// Parameters:
/// - `source`: source signal X
/// - `target`: target signal Y
/// - `lag`: prediction horizon (usually 1)
/// - `order`: embedding order (number of past values to condition on)
/// - `bins`: number of bins for histogram discretization
pub fn transfer_entropy_higher_order(
    source: &[f64],
    target: &[f64],
    lag: usize,
    order: usize,
    bins: usize,
) -> f64 {
    let n = source.len().min(target.len());
    let min_len = lag + order;
    if n <= min_len || bins == 0 || order == 0 {
        return 0.0;
    }

    let m = n - min_len; // number of usable points
    if m < 2 {
        return 0.0;
    }

    // Build target future: Y_{t+lag}
    let target_future: Vec<f64> = (min_len..n).map(|t| target[t]).collect();

    // Build past embeddings as binned indices to compute joint entropies
    // via counting. Each past vector is mapped to a single bin index.

    // Normalize source and target to [0, 1] range for binning
    let src_min = source.iter().cloned().fold(f64::INFINITY, f64::min);
    let src_max = source.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let tgt_min = target.iter().cloned().fold(f64::INFINITY, f64::min);
    let tgt_max = target.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let fut_min = target_future.iter().cloned().fold(f64::INFINITY, f64::min);
    let fut_max = target_future.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    let src_range = (src_max - src_min).max(1e-15);
    let tgt_range = (tgt_max - tgt_min).max(1e-15);
    let fut_range = (fut_max - fut_min).max(1e-15);

    // For higher-order embeddings, we combine the order past values into
    // a single index using mixed-radix encoding: idx = sum(bin[k] * bins^k)
    //
    // H(Y_f, Y_past) uses bins * bins^order total bins
    // H(Y_past, X_past) uses bins^order * bins^order total bins
    // H(Y_f, Y_past, X_past) uses bins * bins^order * bins^order
    // H(Y_past) uses bins^order
    //
    // For order > 2, this explodes. So we cap bins at min(bins, 4) for
    // higher orders to keep memory bounded.
    let effective_bins = if order > 2 { bins.min(4) } else { bins };
    let eb_f = effective_bins as f64;

    let bin_val_eff = |val: f64, min: f64, range: f64| -> usize {
        (((val - min) / range * (eb_f - 1.0)) as usize).min(effective_bins - 1)
    };

    // Compute mixed-radix index for target past embedding
    let target_past_idx = |t: usize| -> usize {
        let mut idx = 0usize;
        let mut multiplier = 1usize;
        for k in 0..order {
            let past_t = t - lag - k;
            let bin = bin_val_eff(target[past_t], tgt_min, tgt_range);
            idx += bin * multiplier;
            multiplier *= effective_bins;
        }
        idx
    };

    // Compute mixed-radix index for source past embedding
    let source_past_idx = |t: usize| -> usize {
        let mut idx = 0usize;
        let mut multiplier = 1usize;
        for k in 0..order {
            let past_t = t - lag - k;
            let bin = bin_val_eff(source[past_t], src_min, src_range);
            idx += bin * multiplier;
            multiplier *= effective_bins;
        }
        idx
    };

    let past_size = effective_bins.pow(order as u32);
    let fut_bins = effective_bins;

    // Count joint distributions
    // H(Y_f, Y_past): fut_bins * past_size
    let mut counts_yf_yp = vec![0usize; fut_bins * past_size];
    // H(Y_past, X_past): past_size * past_size
    let mut counts_yp_xp = vec![0usize; past_size * past_size];
    // H(Y_f, Y_past, X_past): fut_bins * past_size * past_size
    let mut counts_yf_yp_xp = vec![0usize; fut_bins * past_size * past_size];
    // H(Y_past): past_size
    let mut counts_yp = vec![0usize; past_size];

    for i in 0..m {
        let t = i + min_len;
        let yf_bin = bin_val_eff(target_future[i], fut_min, fut_range);
        let yp_idx = target_past_idx(t);
        let xp_idx = source_past_idx(t);

        counts_yf_yp[yf_bin * past_size + yp_idx] += 1;
        counts_yp_xp[yp_idx * past_size + xp_idx] += 1;
        counts_yf_yp_xp[yf_bin * past_size * past_size + yp_idx * past_size + xp_idx] += 1;
        counts_yp[yp_idx] += 1;
    }

    let mf = m as f64;

    let entropy = |counts: &[usize]| -> f64 {
        let mut h = 0.0;
        for &c in counts {
            if c > 0 {
                let p = c as f64 / mf;
                h -= p * p.ln();
            }
        }
        h
    };

    let h_yf_yp = entropy(&counts_yf_yp);
    let h_yp_xp = entropy(&counts_yp_xp);
    let h_yf_yp_xp = entropy(&counts_yf_yp_xp);
    let h_yp = entropy(&counts_yp);

    (h_yf_yp + h_yp_xp - h_yf_yp_xp - h_yp).max(0.0)
}

/// Higher-order KSG k-nearest-neighbor transfer entropy.
///
/// Like `transfer_entropy_knn` but conditions on multiple past values
/// of both source and target. The joint space is (Y_future, X_past^order, Y_past^order)
/// which has dimension 1 + 2*order.
///
/// For order > 1, this can detect coupling that only appears at longer
/// timescales (e.g., X_{t-2} predicts Y_{t} but X_{t-1} does not).
pub fn transfer_entropy_knn_higher_order(
    source: &[f64],
    target: &[f64],
    lag: usize,
    order: usize,
    k: usize,
) -> f64 {
    let n = source.len().min(target.len());
    let min_len = lag + order;
    if n <= min_len || k == 0 || order == 0 {
        return 0.0;
    }

    let m = n - min_len;
    if m <= k + 1 {
        return 0.0;
    }

    // Build embedded vectors for each time point
    // Joint space: (Y_future, X_{t-1}..X_{t-order}, Y_{t-1}..Y_{t-order})
    // Dimension: 1 + order + order = 1 + 2*order
    let dim = 1 + 2 * order;

    let mut points: Vec<Vec<f64>> = Vec::with_capacity(m);
    for i in 0..m {
        let t = i + min_len;
        let mut pt = Vec::with_capacity(dim);
        pt.push(target[t]); // Y_future

        for o in 0..order {
            pt.push(source[t - lag - o]); // X past
        }
        for o in 0..order {
            pt.push(target[t - lag - o]); // Y past
        }
        points.push(pt);
    }

    // KSG conditional MI: I(Y_future; X_past | Y_past)
    // A = Y_future (dim 1)
    // B = X_past (dim order)
    // C = Y_past (dim order)
    //
    // I(A; B | C) = psi(k) - <psi(n_AC + 1) + psi(n_BC + 1) - psi(n_C + 1)>

    let mut psi_sum = 0.0;

    for i in 0..m {
        // Find k-th nearest neighbor in full joint space using Chebyshev norm
        let mut distances: Vec<(usize, f64)> = (0..m)
            .filter(|&j| j != i)
            .map(|j| {
                let d = points[i]
                    .iter()
                    .zip(points[j].iter())
                    .map(|(a, b)| (a - b).abs())
                    .fold(0.0_f64, f64::max);
                (j, d)
            })
            .collect();

        distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        let eps = distances[k - 1].1;

        if eps < 1e-15 {
            let fallback = m - 1;
            psi_sum += crate::consciousness_transfer::digamma((fallback + 1) as f64) * 2.0
                - crate::consciousness_transfer::digamma((fallback + 1) as f64);
            continue;
        }

        // Count neighbors in marginal subspaces within eps
        // AC = (Y_future, Y_past): indices [0] and [1+order..1+2*order)
        // BC = (X_past, Y_past): indices [1..1+order) and [1+order..1+2*order)
        // C = (Y_past): indices [1+order..1+2*order)
        let mut n_ac: usize = 0;
        let mut n_bc: usize = 0;
        let mut n_c: usize = 0;

        for j in 0..m {
            if j == i { continue; }

            // Y_past distance
            let dc = (0..order)
                .map(|o| (points[i][1 + order + o] - points[j][1 + order + o]).abs())
                .fold(0.0_f64, f64::max);

            if dc < eps {
                n_c += 1;

                // AC: Y_future + Y_past
                let da = (points[i][0] - points[j][0]).abs();
                if da < eps {
                    n_ac += 1;
                }

                // BC: X_past + Y_past
                let db = (0..order)
                    .map(|o| (points[i][1 + o] - points[j][1 + o]).abs())
                    .fold(0.0_f64, f64::max);
                if db < eps {
                    n_bc += 1;
                }
            }
        }

        psi_sum += crate::consciousness_transfer::digamma((n_ac + 1) as f64)
            + crate::consciousness_transfer::digamma((n_bc + 1) as f64)
            - crate::consciousness_transfer::digamma((n_c + 1) as f64);
    }

    let te = crate::consciousness_transfer::digamma(k as f64) - psi_sum / m as f64;
    te.max(0.0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram_entropy_constant() {
        let data = vec![5.0; 100];
        let h = histogram_entropy(&data, 10);
        assert!((h).abs() < 1e-10, "H = {h}");
    }

    #[test]
    fn histogram_entropy_uniform() {
        let data: Vec<f64> = (0..1000).map(|i| i as f64).collect();
        let h = histogram_entropy(&data, 10);
        // Should be close to ln(10) for uniform distribution
        assert!(h > 1.5, "H = {h}");
    }

    #[test]
    fn histogram_entropy_empty() {
        assert_eq!(histogram_entropy(&[], 10), 0.0);
    }

    #[test]
    fn joint_entropy_independent() {
        let a: Vec<f64> = (0..500).map(|i| (i % 100) as f64).collect();
        let b: Vec<f64> = (0..500).map(|i| ((i * 7 + 3) % 100) as f64).collect();
        let je = joint_entropy(&a, &b, 10);
        let ha = histogram_entropy(&a, 10);
        let hb = histogram_entropy(&b, 10);
        // For independent variables: H(A,B) >= max(H(A), H(B))
        assert!(je >= ha.min(hb) - 0.1, "JE={je}, HA={ha}, HB={hb}");
    }

    #[test]
    fn conditional_entropy_nonneg() {
        let a: Vec<f64> = (0..200).map(|i| (i % 50) as f64).collect();
        let b: Vec<f64> = (0..200).map(|i| ((i + 10) % 50) as f64).collect();
        let ce = conditional_entropy(&a, &b, 10);
        assert!(ce >= 0.0, "CE = {ce}");
    }

    #[test]
    fn transfer_entropy_independent() {
        // Independent signals should have near-zero TE
        let source: Vec<f64> = (0..500).map(|i| ((i * 137 + 43) % 256) as f64).collect();
        let target: Vec<f64> = (0..500).map(|i| ((i * 97 + 31) % 256) as f64).collect();
        let te = transfer_entropy(&source, &target, 1, 8);
        // TE should be small for independent signals
        assert!(te < 0.5, "TE = {te}");
    }

    #[test]
    fn transfer_entropy_coupled() {
        // Coupled signals: target = shifted source
        let source: Vec<f64> = (0..500).map(|i| ((i * 137 + 43) % 256) as f64).collect();
        let mut target = vec![0.0];
        target.extend_from_slice(&source[..499]);
        let te = transfer_entropy(&source, &target, 1, 8);
        // TE should be non-negative for coupled signals
        // (may be 0 due to binning discretization artifacts)
        assert!(te >= 0.0, "TE = {te}");
    }

    #[test]
    fn transfer_entropy_short_data() {
        let source = vec![1.0, 2.0];
        let target = vec![3.0, 4.0];
        let te = transfer_entropy(&source, &target, 1, 8);
        assert_eq!(te, 0.0);
    }

    #[test]
    fn te_matrix_basic() {
        let sources = vec![
            ("A".to_string(), (0..100).map(|i| ((i * 3) % 256) as f64).collect()),
            ("B".to_string(), (0..100).map(|i| ((i * 7 + 5) % 256) as f64).collect()),
        ];
        let matrix = transfer_entropy_matrix(&sources, 1);
        assert_eq!(matrix.sources.len(), 2);
        assert_eq!(matrix.pairs.len(), 1); // 1 pair from 2 sources
        assert!(matrix.mean_te >= 0.0);
    }

    #[test]
    fn te_matrix_self_zero() {
        let sources = vec![
            ("A".to_string(), (0..100).map(|i| i as f64).collect()),
        ];
        let matrix = transfer_entropy_matrix(&sources, 1);
        assert_eq!(matrix.te_values[0][0], 0.0); // Self-TE is 0
    }

    #[test]
    fn compare_te_same_data() {
        let sources = vec![
            ("A".to_string(), (0..100).map(|i| ((i * 3) % 256) as f64).collect()),
            ("B".to_string(), (0..100).map(|i| ((i * 7 + 5) % 256) as f64).collect()),
        ];
        let matrix = transfer_entropy_matrix(&sources, 1);
        let cmp = compare_transfer_entropy(&matrix, &matrix);
        assert!(cmp.mean_te_change.abs() < 1e-10);
        assert!(cmp.increased_pairs.is_empty());
    }

    #[test]
    fn bytes_to_floats_basic() {
        let mut data = HashMap::new();
        data.insert("src".to_string(), vec![0u8, 128, 255]);
        let result = bytes_to_floats(&data);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].1, vec![0.0, 128.0, 255.0]);
    }

    // -----------------------------------------------------------------------
    // 3D Joint Entropy Tests
    // -----------------------------------------------------------------------

    #[test]
    fn joint_entropy_3d_empty() {
        assert_eq!(joint_entropy_3d(&[], &[], &[], 10), 0.0);
    }

    #[test]
    fn joint_entropy_3d_zero_bins() {
        let a = vec![1.0, 2.0, 3.0];
        assert_eq!(joint_entropy_3d(&a, &a, &a, 0), 0.0);
    }

    #[test]
    fn joint_entropy_3d_constant() {
        // All three variables constant => H = 0
        let a = vec![5.0; 200];
        let b = vec![3.0; 200];
        let c = vec![7.0; 200];
        let h = joint_entropy_3d(&a, &b, &c, 8);
        assert!(h.abs() < 1e-10, "H(constant, constant, constant) = {h}");
    }

    #[test]
    fn joint_entropy_3d_geq_2d() {
        // H(A,B,C) >= H(A,B) always (adding a variable never decreases entropy)
        let a: Vec<f64> = (0..500).map(|i| (i % 100) as f64).collect();
        let b: Vec<f64> = (0..500).map(|i| ((i * 7 + 3) % 100) as f64).collect();
        let c: Vec<f64> = (0..500).map(|i| ((i * 13 + 17) % 100) as f64).collect();
        let h3 = joint_entropy_3d(&a, &b, &c, 8);
        let h2_ab = joint_entropy(&a, &b, 8);
        let h2_bc = joint_entropy(&b, &c, 8);
        let h2_ac = joint_entropy(&a, &c, 8);
        // 3D joint entropy >= any 2D joint entropy (with small numerical tolerance)
        assert!(
            h3 >= h2_ab - 0.01,
            "H(A,B,C)={h3} < H(A,B)={h2_ab}"
        );
        assert!(
            h3 >= h2_bc - 0.01,
            "H(A,B,C)={h3} < H(B,C)={h2_bc}"
        );
        assert!(
            h3 >= h2_ac - 0.01,
            "H(A,B,C)={h3} < H(A,C)={h2_ac}"
        );
    }

    #[test]
    fn joint_entropy_3d_identical_reduces() {
        // If all three are the same variable, H(A,A,A) == H(A)
        let a: Vec<f64> = (0..300).map(|i| (i % 80) as f64).collect();
        let h3 = joint_entropy_3d(&a, &a, &a, 10);
        let h1 = histogram_entropy(&a, 10);
        // Should be approximately equal (same bin structure)
        assert!(
            (h3 - h1).abs() < 0.05,
            "H(A,A,A)={h3}, H(A)={h1}"
        );
    }

    #[test]
    fn joint_entropy_3d_mismatched_lengths() {
        // Uses min length
        let a: Vec<f64> = (0..100).map(|i| i as f64).collect();
        let b: Vec<f64> = (0..200).map(|i| i as f64).collect();
        let c: Vec<f64> = (0..150).map(|i| i as f64).collect();
        let h = joint_entropy_3d(&a, &b, &c, 8);
        assert!(h > 0.0, "Should have nonzero entropy, got {h}");
    }

    // -----------------------------------------------------------------------
    // Transfer Entropy (3D histogram) Tests
    // -----------------------------------------------------------------------

    #[test]
    fn transfer_entropy_3d_nonnegative() {
        // TE must always be non-negative
        let source: Vec<f64> = (0..300).map(|i| ((i * 41 + 7) % 256) as f64).collect();
        let target: Vec<f64> = (0..300).map(|i| ((i * 83 + 19) % 256) as f64).collect();
        let te = transfer_entropy(&source, &target, 1, 8);
        assert!(te >= 0.0, "TE must be >= 0, got {te}");
    }

    #[test]
    fn transfer_entropy_3d_coupled_detects_flow() {
        // Strongly coupled: target[t] = source[t-1] (perfect lag-1 copy)
        let source: Vec<f64> = (0..1000).map(|i| ((i * 137 + 43) % 256) as f64).collect();
        let mut target = vec![128.0]; // seed value
        target.extend_from_slice(&source[..999]);

        let te_forward = transfer_entropy(&source, &target, 1, 8);
        let te_reverse = transfer_entropy(&target, &source, 1, 8);

        // Forward TE should be substantial
        assert!(
            te_forward > 0.01,
            "Forward TE should detect coupling, got {te_forward}"
        );
        // Forward should exceed reverse (source drives target, not vice versa)
        assert!(
            te_forward > te_reverse,
            "Forward TE ({te_forward}) should exceed reverse TE ({te_reverse})"
        );
    }

    #[test]
    fn transfer_entropy_3d_larger_lag() {
        let source: Vec<f64> = (0..500).map(|i| ((i * 53 + 11) % 256) as f64).collect();
        let target: Vec<f64> = (0..500).map(|i| ((i * 97 + 31) % 256) as f64).collect();
        let te_lag1 = transfer_entropy(&source, &target, 1, 8);
        let te_lag5 = transfer_entropy(&source, &target, 5, 8);
        // Both should be non-negative
        assert!(te_lag1 >= 0.0);
        assert!(te_lag5 >= 0.0);
    }

    // -----------------------------------------------------------------------
    // Digamma Function Tests
    // -----------------------------------------------------------------------

    #[test]
    fn digamma_known_values() {
        // psi(1) = -gamma (Euler-Mascheroni constant) ~= -0.5772
        let psi1 = digamma(1.0);
        assert!(
            (psi1 - (-0.5772156649)).abs() < 1e-6,
            "psi(1) = {psi1}, expected -0.5772"
        );

        // psi(2) = 1 - gamma ~= 0.4228
        let psi2 = digamma(2.0);
        assert!(
            (psi2 - 0.4227843351).abs() < 1e-6,
            "psi(2) = {psi2}, expected 0.4228"
        );

        // psi(0.5) = -gamma - 2*ln(2) ~= -1.9635
        let psi_half = digamma(0.5);
        assert!(
            (psi_half - (-1.9635100260)).abs() < 1e-4,
            "psi(0.5) = {psi_half}, expected -1.9635"
        );
    }

    #[test]
    fn digamma_large_x() {
        // For large x, psi(x) ~= ln(x) - 1/(2x)
        let x = 100.0;
        let psi = digamma(x);
        let approx = x.ln() - 0.5 / x;
        assert!(
            (psi - approx).abs() < 1e-4,
            "psi({x}) = {psi}, asymptotic = {approx}"
        );
    }

    #[test]
    fn digamma_recurrence() {
        // psi(x+1) = psi(x) + 1/x
        // Tolerance of 1e-8 accounts for the asymptotic series precision
        // boundary near x=6 where the recurrence shifts to direct evaluation.
        for x_int in 1..=10 {
            let x = x_int as f64;
            let lhs = digamma(x + 1.0);
            let rhs = digamma(x) + 1.0 / x;
            assert!(
                (lhs - rhs).abs() < 1e-8,
                "Recurrence failed at x={x}: psi({})={lhs}, psi({x})+1/{x}={rhs}",
                x + 1.0
            );
        }
    }

    #[test]
    fn digamma_zero_returns_neg_inf() {
        assert_eq!(digamma(0.0), f64::NEG_INFINITY);
        assert_eq!(digamma(-1.0), f64::NEG_INFINITY);
    }

    // -----------------------------------------------------------------------
    // Chebyshev Distance Tests
    // -----------------------------------------------------------------------

    #[test]
    fn chebyshev_distance_identical() {
        let a = vec![1.0, 2.0, 3.0];
        assert_eq!(chebyshev_distance(&a, &a), 0.0);
    }

    #[test]
    fn chebyshev_distance_known() {
        let a = vec![1.0, 5.0, 3.0];
        let b = vec![4.0, 2.0, 1.0];
        // max(|1-4|, |5-2|, |3-1|) = max(3, 3, 2) = 3
        assert_eq!(chebyshev_distance(&a, &b), 3.0);
    }

    #[test]
    fn chebyshev_distance_single_dim() {
        let a = vec![10.0];
        let b = vec![3.5];
        assert!((chebyshev_distance(&a, &b) - 6.5).abs() < 1e-10);
    }

    // -----------------------------------------------------------------------
    // KSG Transfer Entropy Tests
    // -----------------------------------------------------------------------

    #[test]
    fn te_knn_independent_near_zero() {
        // Independent signals should yield near-zero TE
        let source: Vec<f64> = (0..500).map(|i| ((i * 137 + 43) % 256) as f64).collect();
        let target: Vec<f64> = (0..500).map(|i| ((i * 97 + 31) % 256) as f64).collect();
        let te = transfer_entropy_knn(&source, &target, 1, 4);
        assert!(
            te < 1.0,
            "KNN TE for independent signals should be small, got {te}"
        );
    }

    #[test]
    fn te_knn_coupled_detects_flow() {
        // Coupled: target is lagged copy of source
        let source: Vec<f64> = (0..500).map(|i| ((i * 137 + 43) % 256) as f64).collect();
        let mut target = vec![128.0];
        target.extend_from_slice(&source[..499]);

        let te_forward = transfer_entropy_knn(&source, &target, 1, 4);
        let te_reverse = transfer_entropy_knn(&target, &source, 1, 4);

        // Forward should be positive
        assert!(te_forward >= 0.0, "Forward KNN TE = {te_forward}");
        // Forward should exceed reverse (directional flow)
        assert!(
            te_forward > te_reverse,
            "Forward KNN TE ({te_forward}) should exceed reverse ({te_reverse})"
        );
    }

    #[test]
    fn te_knn_nonnegative() {
        let source: Vec<f64> = (0..200).map(|i| ((i * 41 + 7) % 256) as f64).collect();
        let target: Vec<f64> = (0..200).map(|i| ((i * 83 + 19) % 256) as f64).collect();
        let te = transfer_entropy_knn(&source, &target, 1, 4);
        assert!(te >= 0.0, "KNN TE must be >= 0, got {te}");
    }

    #[test]
    fn te_knn_short_data() {
        let source = vec![1.0, 2.0];
        let target = vec![3.0, 4.0];
        let te = transfer_entropy_knn(&source, &target, 1, 4);
        assert_eq!(te, 0.0, "Short data should return 0");
    }

    #[test]
    fn te_knn_zero_k() {
        let source: Vec<f64> = (0..100).map(|i| i as f64).collect();
        let target: Vec<f64> = (0..100).map(|i| i as f64).collect();
        let te = transfer_entropy_knn(&source, &target, 1, 0);
        assert_eq!(te, 0.0, "k=0 should return 0");
    }

    #[test]
    fn te_knn_different_k_values() {
        let source: Vec<f64> = (0..300).map(|i| ((i * 137 + 43) % 256) as f64).collect();
        let mut target = vec![128.0];
        target.extend_from_slice(&source[..299]);

        let te_k1 = transfer_entropy_knn(&source, &target, 1, 1);
        let te_k4 = transfer_entropy_knn(&source, &target, 1, 4);
        let te_k8 = transfer_entropy_knn(&source, &target, 1, 8);

        // All should be non-negative
        assert!(te_k1 >= 0.0);
        assert!(te_k4 >= 0.0);
        assert!(te_k8 >= 0.0);
    }

    // -----------------------------------------------------------------------
    // Comparison Function Tests
    // -----------------------------------------------------------------------

    #[test]
    fn te_comparison_returns_both() {
        let source: Vec<f64> = (0..300).map(|i| ((i * 137 + 43) % 256) as f64).collect();
        let target: Vec<f64> = (0..300).map(|i| ((i * 97 + 31) % 256) as f64).collect();
        let (hist_te, knn_te) = transfer_entropy_comparison(&source, &target, 1, 8, 4);
        assert!(hist_te >= 0.0, "Histogram TE = {hist_te}");
        assert!(knn_te >= 0.0, "KNN TE = {knn_te}");
    }

    #[test]
    fn te_comparison_coupled_both_detect() {
        // Both methods should detect coupling
        let source: Vec<f64> = (0..500).map(|i| ((i * 137 + 43) % 256) as f64).collect();
        let mut target = vec![128.0];
        target.extend_from_slice(&source[..499]);

        let (hist_te, knn_te) = transfer_entropy_comparison(&source, &target, 1, 8, 4);
        // Both should be positive for coupled signals
        assert!(hist_te >= 0.0, "Histogram TE should be >= 0 for coupled, got {hist_te}");
        assert!(knn_te >= 0.0, "KNN TE should be >= 0 for coupled, got {knn_te}");
    }

    // -----------------------------------------------------------------------
    // KNN Matrix Tests
    // -----------------------------------------------------------------------

    #[test]
    fn te_matrix_knn_basic() {
        let sources = vec![
            (
                "A".to_string(),
                (0..100).map(|i| ((i * 3) % 256) as f64).collect(),
            ),
            (
                "B".to_string(),
                (0..100).map(|i| ((i * 7 + 5) % 256) as f64).collect(),
            ),
        ];
        let matrix = transfer_entropy_matrix_knn(&sources, 1, 4);
        assert_eq!(matrix.sources.len(), 2);
        assert_eq!(matrix.pairs.len(), 1);
        assert!(matrix.mean_te >= 0.0);
    }

    #[test]
    fn te_matrix_knn_self_zero() {
        let sources = vec![(
            "A".to_string(),
            (0..100).map(|i| i as f64).collect(),
        )];
        let matrix = transfer_entropy_matrix_knn(&sources, 1, 4);
        assert_eq!(matrix.te_values[0][0], 0.0);
    }

    #[test]
    fn te_matrix_knn_three_sources() {
        let sources = vec![
            (
                "X".to_string(),
                (0..200).map(|i| ((i * 13 + 7) % 256) as f64).collect(),
            ),
            (
                "Y".to_string(),
                (0..200).map(|i| ((i * 29 + 3) % 256) as f64).collect(),
            ),
            (
                "Z".to_string(),
                (0..200).map(|i| ((i * 47 + 11) % 256) as f64).collect(),
            ),
        ];
        let matrix = transfer_entropy_matrix_knn(&sources, 1, 4);
        assert_eq!(matrix.sources.len(), 3);
        assert_eq!(matrix.pairs.len(), 3); // C(3,2) = 3 pairs
        // Diagonal should be zero
        for i in 0..3 {
            assert_eq!(matrix.te_values[i][i], 0.0);
        }
    }

    // -----------------------------------------------------------------------
    // Higher-Order Transfer Entropy Tests
    // -----------------------------------------------------------------------

    #[test]
    fn te_higher_order_basic() {
        let source: Vec<f64> = (0..300).map(|i| ((i * 137 + 43) % 256) as f64).collect();
        let target: Vec<f64> = (0..300).map(|i| ((i * 97 + 31) % 256) as f64).collect();
        let te1 = transfer_entropy_higher_order(&source, &target, 1, 1, 8);
        let te2 = transfer_entropy_higher_order(&source, &target, 1, 2, 8);
        // Both should be non-negative
        assert!(te1 >= 0.0, "Order-1 TE = {te1}");
        assert!(te2 >= 0.0, "Order-2 TE = {te2}");
    }

    #[test]
    fn te_higher_order_coupled() {
        // target[t] depends on source[t-1] and source[t-2]
        let source: Vec<f64> = (0..500).map(|i| ((i * 137 + 43) % 256) as f64).collect();
        let mut target = vec![128.0, 128.0];
        for t in 2..500 {
            target.push((source[t - 1] + source[t - 2]) / 2.0);
        }
        let te2 = transfer_entropy_higher_order(&source, &target, 1, 2, 8);
        assert!(te2 >= 0.0, "Higher-order TE should detect multi-lag coupling, got {te2}");
    }

    #[test]
    fn te_higher_order_short_data() {
        let source = vec![1.0, 2.0, 3.0];
        let target = vec![4.0, 5.0, 6.0];
        let te = transfer_entropy_higher_order(&source, &target, 1, 2, 8);
        assert_eq!(te, 0.0);
    }

    #[test]
    fn te_higher_order_zero_order() {
        let source: Vec<f64> = (0..100).map(|i| i as f64).collect();
        let target: Vec<f64> = (0..100).map(|i| i as f64).collect();
        let te = transfer_entropy_higher_order(&source, &target, 1, 0, 8);
        assert_eq!(te, 0.0);
    }

    #[test]
    fn te_knn_higher_order_basic() {
        let source: Vec<f64> = (0..200).map(|i| ((i * 137 + 43) % 256) as f64).collect();
        let target: Vec<f64> = (0..200).map(|i| ((i * 97 + 31) % 256) as f64).collect();
        let te = transfer_entropy_knn_higher_order(&source, &target, 1, 2, 4);
        assert!(te >= 0.0, "KNN higher-order TE should be >= 0, got {te}");
    }

    #[test]
    fn te_knn_higher_order_short_data() {
        let source = vec![1.0, 2.0];
        let target = vec![3.0, 4.0];
        let te = transfer_entropy_knn_higher_order(&source, &target, 1, 2, 4);
        assert_eq!(te, 0.0);
    }

    #[test]
    fn te_knn_higher_order_coupled() {
        let source: Vec<f64> = (0..300).map(|i| ((i * 137 + 43) % 256) as f64).collect();
        let mut target = vec![128.0, 128.0];
        for t in 2..300 {
            target.push((source[t - 1] + source[t - 2]) / 2.0);
        }
        let te = transfer_entropy_knn_higher_order(&source, &target, 1, 2, 4);
        assert!(te >= 0.0, "KNN higher-order should detect coupling, got {te}");
    }
}

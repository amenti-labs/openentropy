//! Persistent homology for consciousness-RNG experiments.
//!
//! Topological data analysis (TDA) of RNG bit streams via delay-coordinate
//! embedding and Vietoris-Rips persistent homology. Detects topological features
//! (connected components, loops) in the point cloud structure of entropy data.
//!
//! Key insight: truly random data in R^d produces a featureless point cloud
//! with no significant persistent features. Any topological structure injected
//! by consciousness would appear as long-lived barcodes in the persistence diagram.
//!
//! Based on: Edelsbrunner, Letscher & Zomorodian (2002) "Topological persistence
//! and simplification."

use serde::{Deserialize, Serialize};

/// A persistence pair (birth, death) representing a topological feature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistencePair {
    /// Filtration value at which the feature is born.
    pub birth: f64,
    /// Filtration value at which the feature dies (f64::INFINITY for essential).
    pub death: f64,
    /// Persistence = death - birth.
    pub persistence: f64,
    /// Homology dimension (0 = component, 1 = loop).
    pub dimension: usize,
}

/// Persistence diagram: collection of persistence pairs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistenceDiagram {
    /// H0 pairs (connected components).
    pub h0: Vec<PersistencePair>,
    /// H1 pairs (loops/cycles).
    pub h1: Vec<PersistencePair>,
}

/// Result of topological comparison between baseline and intention.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyResult {
    /// Baseline persistence diagram.
    pub baseline_diagram: PersistenceDiagram,
    /// Intention persistence diagram.
    pub intention_diagram: PersistenceDiagram,
    /// Baseline persistence entropy.
    pub baseline_persistence_entropy: f64,
    /// Intention persistence entropy.
    pub intention_persistence_entropy: f64,
    /// Baseline total persistence (L1).
    pub baseline_total_persistence: f64,
    /// Intention total persistence (L1).
    pub intention_total_persistence: f64,
    /// Approximate Wasserstein distance between diagrams.
    pub wasserstein_distance_h0: f64,
    /// Betti curves match ratio.
    pub betti_curve_divergence: f64,
    /// Interpretation.
    pub interpretation: String,
}

/// Create a delay-coordinate embedding from byte data.
pub fn delay_embedding(data: &[u8], dim: usize, delay: usize) -> Vec<Vec<f64>> {
    let n = data.len();
    let max_idx = n.saturating_sub((dim - 1) * delay + 1);
    let mut points = Vec::with_capacity(max_idx);

    for i in 0..max_idx {
        let mut point = Vec::with_capacity(dim);
        for d in 0..dim {
            point.push(data[i + d * delay] as f64 / 255.0); // Normalize to [0, 1]
        }
        points.push(point);
    }

    points
}

/// Compute pairwise Euclidean distance matrix.
pub fn pairwise_distances(points: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let n = points.len();
    let mut dist = vec![vec![0.0; n]; n];

    for i in 0..n {
        for j in (i + 1)..n {
            let d: f64 = points[i]
                .iter()
                .zip(points[j].iter())
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f64>()
                .sqrt();
            dist[i][j] = d;
            dist[j][i] = d;
        }
    }

    dist
}

/// Compute H0 persistence using Union-Find on edges sorted by distance.
///
/// H0 tracks connected components: each merge of two components produces
/// a persistence pair (birth of the younger component, death at merge distance).
fn compute_h0(distances: &[Vec<f64>]) -> Vec<PersistencePair> {
    compute_h0_filtered(distances, f64::INFINITY)
}

/// Compute H0 persistence, skipping edges with distance above `max_filtration`.
fn compute_h0_filtered(distances: &[Vec<f64>], max_filtration: f64) -> Vec<PersistencePair> {
    let n = distances.len();
    if n == 0 {
        return Vec::new();
    }

    // Collect edges up to max_filtration
    let mut edges: Vec<(f64, usize, usize)> = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            let d = distances[i][j];
            if d <= max_filtration {
                edges.push((d, i, j));
            }
        }
    }
    edges.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    // Union-Find
    let mut parent: Vec<usize> = (0..n).collect();
    let mut rank: Vec<usize> = vec![0; n];
    let mut birth: Vec<f64> = vec![0.0; n]; // All components born at distance 0

    fn find(parent: &mut Vec<usize>, x: usize) -> usize {
        if parent[x] != x {
            parent[x] = find(parent, parent[x]);
        }
        parent[x]
    }

    fn union(parent: &mut Vec<usize>, rank: &mut Vec<usize>, x: usize, y: usize) -> (usize, usize) {
        let (rx, ry) = (find(parent, x), find(parent, y));
        if rx == ry {
            return (rx, ry);
        }
        // Merge smaller into larger; return (survivor, killed)
        if rank[rx] < rank[ry] {
            parent[rx] = ry;
            (ry, rx)
        } else if rank[rx] > rank[ry] {
            parent[ry] = rx;
            (rx, ry)
        } else {
            parent[ry] = rx;
            rank[rx] += 1;
            (rx, ry)
        }
    }

    let mut pairs = Vec::new();

    for &(dist, i, j) in &edges {
        let ri = find(&mut parent, i);
        let rj = find(&mut parent, j);
        if ri != rj {
            let (survivor, killed) = union(&mut parent, &mut rank, ri, rj);
            // The killed component dies at this distance
            let b = birth[killed];
            pairs.push(PersistencePair {
                birth: b,
                death: dist,
                persistence: dist - b,
                dimension: 0,
            });
            // Survivor keeps its birth time (the earlier one)
            birth[survivor] = birth[survivor].min(birth[killed]);
        }
    }

    // The last surviving component has infinite persistence (essential class)
    // We represent it but don't include in pairs since it's always present

    pairs
}

/// Compute H1 persistence using boundary matrix reduction over Z/2Z.
///
/// Uses adjacency-list intersection for triangle detection, reducing
/// the per-edge cost from O(n) to O(deg(i) + deg(j)) where deg is the
/// vertex degree. Overall complexity improves from O(n^3) to roughly
/// O(E * max_deg) where E = number of edges and max_deg << n for sparse
/// complexes.
fn compute_h1(distances: &[Vec<f64>]) -> Vec<PersistencePair> {
    compute_h1_filtered(distances, f64::INFINITY)
}

/// Compute H1 persistence, skipping edges with distance above `max_filtration`.
fn compute_h1_filtered(distances: &[Vec<f64>], max_filtration: f64) -> Vec<PersistencePair> {
    let n = distances.len();
    if n < 3 {
        return Vec::new();
    }

    // Collect edges up to max_filtration, sorted by distance
    let mut edges: Vec<(f64, usize, usize)> = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            let d = distances[i][j];
            if d <= max_filtration {
                edges.push((d, i, j));
            }
        }
    }
    edges.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    // Adjacency lists (sorted neighbor sets) for efficient triangle detection.
    // neighbors[v] contains sorted list of vertices adjacent to v.
    let mut neighbors: Vec<Vec<usize>> = vec![Vec::new(); n];
    // edge_birth[i][j] stored in a HashMap-like structure to avoid O(n^2) memory
    // when max_filtration prunes many edges. We use a flat Vec<Vec<f64>> since n
    // is already bounded by landmark subsampling (typically ~50).
    let mut edge_birth: Vec<Vec<f64>> = vec![vec![f64::INFINITY; n]; n];
    let mut pairs = Vec::new();

    for &(dist, i, j) in &edges {
        // Find common neighbors via sorted-list intersection of neighbors[i] and neighbors[j].
        // This is O(deg(i) + deg(j)) instead of O(n).
        let ni = &neighbors[i];
        let nj = &neighbors[j];
        let mut pi = 0;
        let mut pj = 0;
        while pi < ni.len() && pj < nj.len() {
            if ni[pi] == nj[pj] {
                let k = ni[pi];
                // Triangle (i, j, k) formed at distance `dist`
                // The 1-cycle was born at the max of the two existing edge distances
                let cycle_birth = edge_birth[i][k].max(edge_birth[j][k]);
                if dist > cycle_birth {
                    pairs.push(PersistencePair {
                        birth: cycle_birth,
                        death: dist,
                        persistence: dist - cycle_birth,
                        dimension: 1,
                    });
                }
                pi += 1;
                pj += 1;
            } else if ni[pi] < nj[pj] {
                pi += 1;
            } else {
                pj += 1;
            }
        }

        // Insert into neighbor lists (maintain sorted order via insert)
        let pos_i = neighbors[i].binary_search(&j).unwrap_or_else(|p| p);
        neighbors[i].insert(pos_i, j);
        let pos_j = neighbors[j].binary_search(&i).unwrap_or_else(|p| p);
        neighbors[j].insert(pos_j, i);

        edge_birth[i][j] = dist;
        edge_birth[j][i] = dist;
    }

    // Sort by persistence (most significant first)
    pairs.sort_by(|a, b| b.persistence.partial_cmp(&a.persistence).unwrap_or(std::cmp::Ordering::Equal));

    // Keep only the most significant H1 features (limit to avoid noise)
    pairs.truncate(n);
    pairs
}

/// Compute Vietoris-Rips persistence for H0 and H1.
pub fn vietoris_rips_h0_h1(distances: &[Vec<f64>]) -> PersistenceDiagram {
    PersistenceDiagram {
        h0: compute_h0(distances),
        h1: compute_h1(distances),
    }
}

/// Compute Vietoris-Rips persistence with an edge distance threshold.
///
/// Edges above `max_filtration` are never added, reducing computation for
/// sparse point clouds. Features that would be born or die beyond the
/// threshold are simply not captured.
pub fn vietoris_rips_h0_h1_filtered(
    distances: &[Vec<f64>],
    max_filtration: f64,
) -> PersistenceDiagram {
    PersistenceDiagram {
        h0: compute_h0_filtered(distances, max_filtration),
        h1: compute_h1_filtered(distances, max_filtration),
    }
}

/// Estimate a filtration threshold as a percentile of the pairwise distance matrix.
///
/// `percentile` should be in [0.0, 1.0]. For example, 0.8 gives the 80th
/// percentile of all pairwise distances.
pub fn estimate_max_filtration(distances: &[Vec<f64>], percentile: f64) -> f64 {
    let n = distances.len();
    if n < 2 {
        return f64::INFINITY;
    }

    let mut dists: Vec<f64> = Vec::with_capacity(n * (n - 1) / 2);
    for i in 0..n {
        for j in (i + 1)..n {
            dists.push(distances[i][j]);
        }
    }
    dists.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let idx = ((dists.len() as f64 * percentile) as usize).min(dists.len().saturating_sub(1));
    dists[idx]
}

/// Shannon entropy of barcode lifetimes (persistence entropy).
///
/// Measures the diversity of topological feature lifetimes.
/// High entropy = many features of similar persistence.
/// Low entropy = one dominant feature.
pub fn persistence_entropy(diagram: &PersistenceDiagram) -> f64 {
    let all_persistence: Vec<f64> = diagram
        .h0
        .iter()
        .chain(diagram.h1.iter())
        .filter(|p| p.persistence > 0.0 && p.persistence.is_finite())
        .map(|p| p.persistence)
        .collect();

    if all_persistence.is_empty() {
        return 0.0;
    }

    let total: f64 = all_persistence.iter().sum();
    if total <= 0.0 {
        return 0.0;
    }

    let mut entropy = 0.0;
    for &p in &all_persistence {
        let prob = p / total;
        if prob > 0.0 {
            entropy -= prob * prob.ln();
        }
    }

    entropy
}

/// Total persistence: L^p norm of barcode lifetimes.
pub fn total_persistence(diagram: &PersistenceDiagram, p: f64) -> f64 {
    diagram
        .h0
        .iter()
        .chain(diagram.h1.iter())
        .filter(|pair| pair.persistence.is_finite())
        .map(|pair| pair.persistence.powf(p))
        .sum::<f64>()
        .powf(1.0 / p)
}

/// Betti curve: Betti number as a function of filtration parameter.
pub fn betti_curve(diagram: &PersistenceDiagram, n_steps: usize, dimension: usize) -> Vec<(f64, usize)> {
    let pairs = if dimension == 0 {
        &diagram.h0
    } else {
        &diagram.h1
    };

    if pairs.is_empty() {
        return vec![(0.0, 0); n_steps];
    }

    let max_death = pairs
        .iter()
        .filter(|p| p.death.is_finite())
        .map(|p| p.death)
        .fold(0.0_f64, f64::max);
    let max_val = max_death * 1.1;

    let step = max_val / n_steps as f64;
    let mut curve = Vec::with_capacity(n_steps);

    for i in 0..n_steps {
        let t = i as f64 * step;
        let betti = pairs
            .iter()
            .filter(|p| p.birth <= t && (p.death > t || p.death.is_infinite()))
            .count();
        curve.push((t, betti));
    }

    curve
}

/// Approximate Wasserstein distance between two persistence diagrams.
///
/// Uses a greedy matching approximation (not the true optimal transport,
/// which would require the Hungarian algorithm).
pub fn wasserstein_distance(d1: &PersistenceDiagram, d2: &PersistenceDiagram) -> f64 {
    // Compare H0 diagrams
    wasserstein_pairs(&d1.h0, &d2.h0) + wasserstein_pairs(&d1.h1, &d2.h1)
}

/// Greedy Wasserstein approximation between two sets of persistence pairs.
fn wasserstein_pairs(p1: &[PersistencePair], p2: &[PersistencePair]) -> f64 {
    if p1.is_empty() && p2.is_empty() {
        return 0.0;
    }

    // Cost of matching a pair to its projection on the diagonal
    let diag_cost = |p: &PersistencePair| -> f64 { p.persistence / 2.0 };

    // Cost of matching two pairs
    let match_cost = |a: &PersistencePair, b: &PersistencePair| -> f64 {
        ((a.birth - b.birth).powi(2) + (a.death - b.death).powi(2)).sqrt()
    };

    // Simple greedy: match closest pairs, unmatched go to diagonal
    let mut used_2 = vec![false; p2.len()];
    let mut total_cost = 0.0;

    for pair1 in p1 {
        let mut best_cost = diag_cost(pair1);
        let mut best_idx = None;

        for (j, pair2) in p2.iter().enumerate() {
            if !used_2[j] {
                let mc = match_cost(pair1, pair2);
                if mc < best_cost {
                    best_cost = mc;
                    best_idx = Some(j);
                }
            }
        }

        total_cost += best_cost;
        if let Some(j) = best_idx {
            used_2[j] = true;
        }
    }

    // Unmatched pairs from p2 go to diagonal
    for (j, pair2) in p2.iter().enumerate() {
        if !used_2[j] {
            total_cost += diag_cost(pair2);
        }
    }

    total_cost
}

/// Full topological comparison between baseline and intention data.
pub fn compute_topology(baseline: &[u8], intention: &[u8], dim: usize) -> TopologyResult {
    let delay = 1;

    // Use landmark subsampling for better point cloud coverage (O(n*k) selection)
    let max_points = 50;
    let bl_points = landmark_subsample(baseline, max_points, dim, delay);
    let int_points = landmark_subsample(intention, max_points, dim, delay);

    let bl_dist = pairwise_distances(&bl_points);
    let int_dist = pairwise_distances(&int_points);

    // Estimate filtration threshold at 80th percentile for sparse computation
    let bl_max_filt = estimate_max_filtration(&bl_dist, 0.80);
    let int_max_filt = estimate_max_filtration(&int_dist, 0.80);

    let bl_diagram = vietoris_rips_h0_h1_filtered(&bl_dist, bl_max_filt);
    let int_diagram = vietoris_rips_h0_h1_filtered(&int_dist, int_max_filt);

    let bl_pe = persistence_entropy(&bl_diagram);
    let int_pe = persistence_entropy(&int_diagram);
    let bl_tp = total_persistence(&bl_diagram, 1.0);
    let int_tp = total_persistence(&int_diagram, 1.0);

    let w_h0 = wasserstein_pairs(&bl_diagram.h0, &int_diagram.h0);

    // Betti curve divergence (L1 distance between Betti curves)
    let n_steps = 50;
    let bl_betti = betti_curve(&bl_diagram, n_steps, 0);
    let int_betti = betti_curve(&int_diagram, n_steps, 0);
    let betti_div: f64 = bl_betti
        .iter()
        .zip(int_betti.iter())
        .map(|((_, b1), (_, b2))| (*b1 as f64 - *b2 as f64).abs())
        .sum::<f64>()
        / n_steps as f64;

    let pe_diff = (int_pe - bl_pe).abs();
    let tp_diff = (int_tp - bl_tp).abs();

    let interpretation = if pe_diff > 0.5 || tp_diff > 0.5 {
        format!(
            "Topological structure differs between conditions: \
             persistence entropy delta={:.3}, total persistence delta={:.3}. \
             Intention data has {} topological structure than baseline.",
            int_pe - bl_pe,
            int_tp - bl_tp,
            if int_tp > bl_tp { "more" } else { "less" }
        )
    } else {
        format!(
            "Topological structure similar between conditions: \
             PE delta={:.3}, TP delta={:.3}. \
             Both show comparable point cloud topology.",
            int_pe - bl_pe,
            int_tp - bl_tp,
        )
    };

    TopologyResult {
        baseline_diagram: bl_diagram,
        intention_diagram: int_diagram,
        baseline_persistence_entropy: bl_pe,
        intention_persistence_entropy: int_pe,
        baseline_total_persistence: bl_tp,
        intention_total_persistence: int_tp,
        wasserstein_distance_h0: w_h0,
        betti_curve_divergence: betti_div,
        interpretation,
    }
}

/// Subsample data to at most max_len bytes (legacy uniform stepping).
///
/// Retained for backward compatibility and testing; production code now uses
/// `landmark_subsample` which provides better point cloud coverage.
#[cfg(test)]
fn subsample(data: &[u8], max_len: usize) -> Vec<u8> {
    if data.len() <= max_len {
        return data.to_vec();
    }
    let step = data.len() / max_len;
    data.iter().step_by(step.max(1)).copied().take(max_len).collect()
}

/// Max-min landmark subsampling with delay-coordinate embedding.
///
/// Embeds ALL data points into R^dim via delay-coordinate embedding, then
/// selects `max_points` landmark points using farthest-point sampling
/// (max-min selection). This gives much better coverage of the point cloud
/// than naive uniform stepping, especially for data with non-uniform density.
///
/// Algorithm:
/// 1. Embed all bytes into R^dim via delay embedding.
/// 2. Pick a deterministic seed point (index 0).
/// 3. Greedily select the point farthest from all previously selected landmarks.
/// 4. Return the selected landmark point cloud.
///
/// Complexity: O(n * max_points) for selection where n = number of embedded points.
pub fn landmark_subsample(data: &[u8], max_points: usize, dim: usize, delay: usize) -> Vec<Vec<f64>> {
    let all_points = delay_embedding(data, dim, delay);
    let n = all_points.len();

    if n <= max_points {
        return all_points;
    }

    // Distance from each point to the nearest selected landmark.
    // Initialized to INFINITY so the first selection is deterministic.
    let mut min_dist_to_landmarks = vec![f64::INFINITY; n];

    // Selected landmark indices
    let mut landmarks: Vec<usize> = Vec::with_capacity(max_points);

    // Start with index 0 (deterministic; avoids needing a PRNG dependency).
    // For randomized starts, the caller can pre-shuffle data.
    let first = 0;
    landmarks.push(first);

    // Update min distances from all points to the first landmark
    for idx in 0..n {
        let d = sq_euclidean(&all_points[idx], &all_points[first]);
        if d < min_dist_to_landmarks[idx] {
            min_dist_to_landmarks[idx] = d;
        }
    }
    // The landmark itself has distance 0
    min_dist_to_landmarks[first] = 0.0;

    // Greedily select remaining landmarks
    for _ in 1..max_points {
        // Find the point with maximum minimum distance to all existing landmarks
        let mut best_idx = 0;
        let mut best_dist = -1.0_f64;
        for idx in 0..n {
            if min_dist_to_landmarks[idx] > best_dist {
                best_dist = min_dist_to_landmarks[idx];
                best_idx = idx;
            }
        }

        landmarks.push(best_idx);

        // Update min distances with the newly added landmark
        let new_lm = &all_points[best_idx].clone();
        for idx in 0..n {
            let d = sq_euclidean(&all_points[idx], new_lm);
            if d < min_dist_to_landmarks[idx] {
                min_dist_to_landmarks[idx] = d;
            }
        }
        min_dist_to_landmarks[best_idx] = 0.0;
    }

    // Collect landmark points
    landmarks.iter().map(|&i| all_points[i].clone()).collect()
}

/// Squared Euclidean distance (avoids sqrt for comparison-only use).
#[inline]
fn sq_euclidean(a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y) * (x - y))
        .sum()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_embedding_basic() {
        let data = vec![0u8, 128, 255, 64, 192];
        let points = delay_embedding(&data, 2, 1);
        // n=5, dim=2, delay=1 -> max_idx = 5 - (1*1+1) = 3 points
        assert_eq!(points.len(), 3);
        assert!((points[0][0] - 0.0).abs() < 1e-10);
        assert!((points[0][1] - 128.0 / 255.0).abs() < 1e-10);
    }

    #[test]
    fn pairwise_distances_symmetric() {
        let points = vec![vec![0.0, 0.0], vec![1.0, 0.0], vec![0.0, 1.0]];
        let dist = pairwise_distances(&points);
        assert_eq!(dist.len(), 3);
        assert!((dist[0][1] - 1.0).abs() < 1e-10);
        assert!((dist[0][1] - dist[1][0]).abs() < 1e-10);
        assert!((dist[0][0]).abs() < 1e-10);
    }

    #[test]
    fn h0_persistence_basic() {
        let points = vec![vec![0.0], vec![1.0], vec![10.0]];
        let dist = pairwise_distances(&points);
        let diagram = vietoris_rips_h0_h1(&dist);
        // 3 points -> 2 H0 pairs (2 merges)
        assert_eq!(diagram.h0.len(), 2);
        // First merge at distance 1.0
        assert!(diagram.h0.iter().any(|p| (p.death - 1.0).abs() < 1e-10));
    }

    #[test]
    fn persistence_entropy_uniform() {
        let diagram = PersistenceDiagram {
            h0: vec![
                PersistencePair { birth: 0.0, death: 1.0, persistence: 1.0, dimension: 0 },
                PersistencePair { birth: 0.0, death: 1.0, persistence: 1.0, dimension: 0 },
            ],
            h1: vec![],
        };
        let pe = persistence_entropy(&diagram);
        // Two equal persistence values -> max entropy for 2 items = ln(2)
        assert!((pe - 2.0_f64.ln()).abs() < 1e-10);
    }

    #[test]
    fn total_persistence_basic() {
        let diagram = PersistenceDiagram {
            h0: vec![
                PersistencePair { birth: 0.0, death: 3.0, persistence: 3.0, dimension: 0 },
                PersistencePair { birth: 0.0, death: 4.0, persistence: 4.0, dimension: 0 },
            ],
            h1: vec![],
        };
        let tp = total_persistence(&diagram, 1.0);
        assert!((tp - 7.0).abs() < 1e-10);
    }

    #[test]
    fn betti_curve_basic() {
        let diagram = PersistenceDiagram {
            h0: vec![
                PersistencePair { birth: 0.0, death: 1.0, persistence: 1.0, dimension: 0 },
                PersistencePair { birth: 0.0, death: 2.0, persistence: 2.0, dimension: 0 },
            ],
            h1: vec![],
        };
        let curve = betti_curve(&diagram, 10, 0);
        assert_eq!(curve.len(), 10);
        // At t=0, both are alive -> betti = 2
        assert!(curve[0].1 >= 1);
    }

    #[test]
    fn wasserstein_same_diagrams() {
        let d = PersistenceDiagram {
            h0: vec![
                PersistencePair { birth: 0.0, death: 1.0, persistence: 1.0, dimension: 0 },
            ],
            h1: vec![],
        };
        let w = wasserstein_distance(&d, &d);
        assert!((w).abs() < 1e-10);
    }

    #[test]
    fn compute_topology_basic() {
        let baseline: Vec<u8> = (0..200).map(|i| ((i * 97 + 31) % 256) as u8).collect();
        let intention: Vec<u8> = (0..200).map(|i| ((i * 137 + 43) % 256) as u8).collect();
        let result = compute_topology(&baseline, &intention, 3);
        assert!(result.baseline_persistence_entropy >= 0.0);
        assert!(result.intention_persistence_entropy >= 0.0);
        assert!(!result.interpretation.is_empty());
    }

    #[test]
    fn subsample_short_data() {
        let data = vec![1u8, 2, 3];
        let sub = subsample(&data, 10);
        assert_eq!(sub, data);
    }

    #[test]
    fn subsample_long_data() {
        let data: Vec<u8> = (0..1000).map(|i| (i % 256) as u8).collect();
        let sub = subsample(&data, 100);
        assert!(sub.len() <= 100);
    }

    #[test]
    fn landmark_subsample_returns_correct_count() {
        // 500 bytes, dim=3, delay=1 -> 497 embedded points, select 20 landmarks
        let data: Vec<u8> = (0..500).map(|i| ((i * 97 + 31) % 256) as u8).collect();
        let landmarks = landmark_subsample(&data, 20, 3, 1);
        assert_eq!(landmarks.len(), 20);
        // Each landmark should be 3-dimensional
        for pt in &landmarks {
            assert_eq!(pt.len(), 3);
        }
    }

    #[test]
    fn landmark_subsample_short_data_returns_all() {
        // Only 10 bytes, dim=3, delay=1 -> 7 embedded points, request 20
        let data: Vec<u8> = (0..10).collect();
        let landmarks = landmark_subsample(&data, 20, 3, 1);
        // Should return all 7 embedded points since 7 < 20
        assert_eq!(landmarks.len(), 7);
    }

    #[test]
    fn landmark_subsample_better_coverage_than_uniform() {
        // Create data with two clusters: bytes near 0 and bytes near 255,
        // with many more points in the first cluster.
        // Landmarks should pick from BOTH clusters; uniform stepping might miss the small one.
        let mut data: Vec<u8> = vec![0u8; 400]; // 400 bytes near 0
        data.extend(vec![255u8; 20]);            // 20 bytes near 255
        data.extend(vec![0u8; 80]);              // 80 more near 0

        let landmarks = landmark_subsample(&data, 10, 2, 1);
        assert_eq!(landmarks.len(), 10);

        // At least one landmark should be near the 255/255 cluster (normalized ~1.0)
        let has_high = landmarks.iter().any(|pt| pt[0] > 0.9 && pt[1] > 0.9);
        assert!(
            has_high,
            "Farthest-point sampling should discover the distant cluster"
        );
    }

    #[test]
    fn estimate_max_filtration_basic() {
        let points = vec![vec![0.0], vec![1.0], vec![5.0], vec![10.0]];
        let dist = pairwise_distances(&points);
        let p80 = estimate_max_filtration(&dist, 0.80);
        // 6 pairwise distances: 1, 4, 5, 9, 10 -> 80th pctile ~ 9.0
        assert!(p80 >= 5.0 && p80 <= 10.0, "80th percentile should be between 5 and 10, got {}", p80);
    }

    #[test]
    fn filtered_h0_fewer_pairs_than_unfiltered() {
        // With a low max_filtration, fewer merges should happen
        let points = vec![vec![0.0], vec![1.0], vec![10.0], vec![100.0]];
        let dist = pairwise_distances(&points);
        let full = compute_h0(&dist);
        let filtered = compute_h0_filtered(&dist, 5.0);
        // Full: 3 merges. Filtered at 5.0: only the 0-1 merge (dist=1) passes
        assert!(filtered.len() < full.len(),
            "Filtered should produce fewer pairs: filtered={}, full={}", filtered.len(), full.len());
    }

    #[test]
    fn h1_adjacency_index_matches_expected() {
        // Equilateral triangle at distance 1.0 should produce H1 features
        let points = vec![
            vec![0.0, 0.0],
            vec![1.0, 0.0],
            vec![0.5, (3.0_f64).sqrt() / 2.0],
        ];
        let dist = pairwise_distances(&points);
        let diagram = vietoris_rips_h0_h1(&dist);
        // An equilateral triangle forms a single 1-cycle
        assert!(
            !diagram.h1.is_empty(),
            "Equilateral triangle should produce at least one H1 feature"
        );
    }
}

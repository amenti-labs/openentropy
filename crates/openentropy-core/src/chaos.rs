//! # Chaos Theory Analysis
//!
//! Methods for applying chaos theory analysis to QRNG output.
//! These functions help distinguish genuine quantum randomness from
//! deterministic or structured behavior in sampled byte streams.

use flate2::Compression;
use flate2::write::ZlibEncoder;
use serde::Serialize;
use std::f64::consts::LN_2;

use crate::math;

#[derive(Debug, Clone, Serialize)]
pub struct HurstResult {
    pub hurst_exponent: f64,
    pub is_valid: bool,
    pub r_squared: f64,
    pub rs_values: Vec<(usize, f64)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BiEntropyResult {
    pub bien: f64,
    pub tbien: f64,
    pub derivative_entropies: Vec<f64>,
    pub num_derivatives: usize,
    pub is_valid: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct EpiplexityResult {
    pub compression_ratio: f64,       // compressed_size / raw_size
    pub structural_info: f64,         // 1.0 - compression_ratio (epiplexity)
    pub remaining_entropy: f64,       // compression_ratio (unpredictability)
    pub delta_compression_ratio: f64, // compress(diff(data)) / diff_size
    pub is_valid: bool,
}

fn sample_std_dev(data: &[f64]) -> f64 {
    let n = data.len();
    if n < 2 {
        return 0.0;
    }

    let mean = data.iter().sum::<f64>() / n as f64;
    let variance = data
        .iter()
        .map(|x| {
            let d = x - mean;
            d * d
        })
        .sum::<f64>()
        / (n as f64 - 1.0);

    variance.sqrt()
}

#[allow(dead_code)]
fn shannon_entropy_binary(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }

    let mut ones = 0usize;
    let total_bits = data.len() * 8;
    for &byte in data {
        ones += byte.count_ones() as usize;
    }

    let p1 = ones as f64 / total_bits as f64;
    let p0 = 1.0 - p1;

    let h0 = if p0 <= 0.0 {
        0.0
    } else {
        -p0 * (p0.ln() / LN_2)
    };
    let h1 = if p1 <= 0.0 {
        0.0
    } else {
        -p1 * (p1.ln() / LN_2)
    };
    h0 + h1
}

pub fn hurst_exponent(data: &[u8]) -> HurstResult {
    if data.len() < 100 {
        return HurstResult {
            hurst_exponent: f64::NAN,
            is_valid: false,
            r_squared: 0.0,
            rs_values: vec![],
        };
    }

    let n = data.len();
    let min_window = 10usize;
    let max_window = n / 4;
    if max_window <= min_window {
        return HurstResult {
            hurst_exponent: f64::NAN,
            is_valid: false,
            r_squared: 0.0,
            rs_values: vec![],
        };
    }

    let step = ((max_window - min_window) / 20).max(1);
    let mut window_sizes = Vec::new();
    let mut rs_means = Vec::new();
    let mut rs_values = Vec::new();

    for window_size in (min_window..max_window).step_by(step) {
        let mut rs_for_size = Vec::new();

        for start in (0..=(n - window_size)).step_by(window_size) {
            let segment = &data[start..start + window_size];
            let segment_f64: Vec<f64> = segment.iter().map(|&x| x as f64).collect();

            let mean = segment_f64.iter().sum::<f64>() / window_size as f64;

            let mut cumulative = 0.0;
            let mut min_cum = 0.0;
            let mut max_cum = 0.0;
            for &value in &segment_f64 {
                cumulative += value - mean;
                if cumulative < min_cum {
                    min_cum = cumulative;
                }
                if cumulative > max_cum {
                    max_cum = cumulative;
                }
            }

            let range = max_cum - min_cum;
            let std_dev = sample_std_dev(&segment_f64);

            if std_dev > 1e-10 {
                rs_for_size.push(range / std_dev);
            }
        }

        if !rs_for_size.is_empty() {
            let mean_rs = rs_for_size.iter().sum::<f64>() / rs_for_size.len() as f64;
            window_sizes.push(window_size as f64);
            rs_means.push(mean_rs);
            rs_values.push((window_size, mean_rs));
        }
    }

    if rs_values.len() < 5 {
        return HurstResult {
            hurst_exponent: f64::NAN,
            is_valid: false,
            r_squared: 0.0,
            rs_values,
        };
    }

    let log_n: Vec<f64> = window_sizes.iter().map(|&v| v.ln()).collect();
    let log_rs: Vec<f64> = rs_means.iter().map(|&v| v.ln()).collect();
    let (slope, _intercept, r_squared) = math::linear_regression(&log_n, &log_rs);

    HurstResult {
        hurst_exponent: slope,
        is_valid: slope.is_finite(),
        r_squared,
        rs_values,
    }
}

fn bytes_to_bits(data: &[u8]) -> Vec<bool> {
    let mut bits = Vec::with_capacity(data.len() * 8);
    for &byte in data {
        for bit_index in 0..8 {
            bits.push(((byte >> bit_index) & 1) == 1);
        }
    }
    bits
}

fn binary_derivative(bits: &[bool]) -> Vec<bool> {
    if bits.len() < 2 {
        return vec![];
    }

    let mut derivative = Vec::with_capacity(bits.len() - 1);
    for i in 0..(bits.len() - 1) {
        derivative.push(bits[i] ^ bits[i + 1]);
    }
    derivative
}

fn shannon_entropy_bits(bits: &[bool]) -> f64 {
    if bits.is_empty() {
        return 0.0;
    }

    let ones = bits.iter().filter(|&&bit| bit).count();
    let p = ones as f64 / bits.len() as f64;
    if p <= 0.0 || p >= 1.0 {
        return 0.0;
    }

    let q = 1.0 - p;
    -p * (p.ln() / LN_2) - q * (q.ln() / LN_2)
}

pub fn bientropy(data: &[u8]) -> BiEntropyResult {
    if data.len() < 2 {
        return BiEntropyResult {
            bien: 0.0,
            tbien: 0.0,
            derivative_entropies: vec![],
            num_derivatives: 0,
            is_valid: false,
        };
    }

    let mut bien_sum = 0.0;
    let mut tbien_sum = 0.0;
    let mut chunk_count = 0usize;
    let mut derivative_sums: Vec<f64> = vec![];
    let mut derivative_counts: Vec<usize> = vec![];

    for chunk in data.chunks(32) {
        let bits = bytes_to_bits(chunk);
        if bits.len() < 2 {
            continue;
        }

        let max_k = 20usize.min(bits.len() - 1);
        let mut current = bits;
        let mut entropies = Vec::with_capacity(max_k + 1);

        for k in 0..=max_k {
            let h = shannon_entropy_bits(&current);
            entropies.push(h);

            if derivative_sums.len() <= k {
                derivative_sums.push(0.0);
                derivative_counts.push(0);
            }
            derivative_sums[k] += h;
            derivative_counts[k] += 1;

            if k < max_k {
                current = binary_derivative(&current);
            }
        }

        let mut bien_weighted = 0.0;
        let mut tbien_weighted = 0.0;
        let mut tbien_weight_total = 0.0;
        for (k, &h) in entropies.iter().enumerate() {
            let bien_weight = 2f64.powi(k as i32);
            let tbien_weight = (k as f64 + 2.0).log2();
            bien_weighted += bien_weight * h;
            tbien_weighted += tbien_weight * h;
            tbien_weight_total += tbien_weight;
        }

        let bien_normalizer = 2f64.powi((max_k + 1) as i32) - 1.0;
        let chunk_bien = if bien_normalizer > 0.0 {
            bien_weighted / bien_normalizer
        } else {
            0.0
        };
        let chunk_tbien = if tbien_weight_total > 0.0 {
            tbien_weighted / tbien_weight_total
        } else {
            0.0
        };

        bien_sum += chunk_bien;
        tbien_sum += chunk_tbien;
        chunk_count += 1;
    }

    if chunk_count == 0 {
        return BiEntropyResult {
            bien: 0.0,
            tbien: 0.0,
            derivative_entropies: vec![],
            num_derivatives: 0,
            is_valid: false,
        };
    }

    let derivative_entropies: Vec<f64> = derivative_sums
        .iter()
        .zip(derivative_counts.iter())
        .map(|(&sum, &count)| if count > 0 { sum / count as f64 } else { 0.0 })
        .collect();

    BiEntropyResult {
        bien: bien_sum / chunk_count as f64,
        tbien: tbien_sum / chunk_count as f64,
        num_derivatives: derivative_entropies.len(),
        derivative_entropies,
        is_valid: true,
    }
}

fn compress_bytes(data: &[u8]) -> usize {
    use std::io::Write;
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    let _ = encoder.write_all(data);
    encoder.finish().map(|v| v.len()).unwrap_or(data.len())
}

pub fn epiplexity(data: &[u8]) -> EpiplexityResult {
    // Guard: insufficient data
    if data.len() < 20 {
        return EpiplexityResult {
            compression_ratio: 0.0,
            structural_info: 0.0,
            remaining_entropy: 0.0,
            delta_compression_ratio: 0.0,
            is_valid: false,
        };
    }

    // Compression ratio of raw data
    let compressed_size = compress_bytes(data) as f64;
    let raw_size = data.len() as f64;
    let compression_ratio = compressed_size / raw_size;

    // Structural information (epiplexity) = 1 - compression_ratio
    let structural_info = 1.0 - compression_ratio;

    // Remaining entropy = compression_ratio (unpredictability)
    let remaining_entropy = compression_ratio;

    // Delta compression: differences between adjacent bytes
    let delta: Vec<u8> = data.windows(2).map(|w| w[1].wrapping_sub(w[0])).collect();

    let delta_compressed_size = compress_bytes(&delta) as f64;
    let delta_size = delta.len() as f64;
    let delta_compression_ratio = if delta_size > 0.0 {
        delta_compressed_size / delta_size
    } else {
        0.0
    };

    EpiplexityResult {
        compression_ratio,
        structural_info,
        remaining_entropy,
        delta_compression_ratio,
        is_valid: true,
    }
}

fn time_delay_embedding(data: &[f64], m: usize, tau: usize) -> Vec<Vec<f64>> {
    let n = data.len();
    if n < m * tau {
        return vec![];
    }
    let num_vectors = n - (m - 1) * tau;
    (0..num_vectors)
        .map(|i| (0..m).map(|j| data[i + j * tau]).collect())
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct LyapunovResult {
    pub lyapunov_exponent: f64,
    pub divergence_curve: Vec<f64>,
    pub embedding_dim: usize,
    pub delay: usize,
    pub r_squared: f64,
    pub is_valid: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CorrelationDimResult {
    pub dimension: f64,
    pub r_squared: f64,
    pub embedding_dim_used: usize,
    pub num_points_used: usize,
    pub is_valid: bool,
}

pub fn lyapunov_exponent(data: &[u8]) -> LyapunovResult {
    const EMBEDDING_DIM: usize = 3;
    const TAU: usize = 1;
    const MAX_ITER: usize = 50;
    const THEILER_WINDOW: usize = 3;

    let invalid = || LyapunovResult {
        lyapunov_exponent: f64::NAN,
        divergence_curve: vec![],
        embedding_dim: EMBEDDING_DIM,
        delay: TAU,
        r_squared: 0.0,
        is_valid: false,
    };

    if data.len() < 500 {
        return invalid();
    }

    let data_f64: Vec<f64> = data.iter().map(|&b| b as f64).collect();
    let sampled: Vec<f64> = if data_f64.len() > 5000 {
        let step = (data_f64.len() / 5000).max(1);
        data_f64.iter().step_by(step).take(5000).copied().collect()
    } else {
        data_f64
    };

    let embedded = time_delay_embedding(&sampled, EMBEDDING_DIM, TAU);
    if embedded.len() < 100 {
        return invalid();
    }

    let reference_indices: Vec<usize> = if embedded.len() > 200 {
        let step = (embedded.len() / 200).max(1);
        (0..embedded.len()).step_by(step).take(200).collect()
    } else {
        (0..embedded.len()).collect()
    };

    let mut divergence_sums = vec![0.0; MAX_ITER];
    let mut divergence_counts = vec![0usize; MAX_ITER];

    for &i in &reference_indices {
        let mut nearest_neighbor = None;
        let mut min_distance = f64::INFINITY;

        for j in 0..embedded.len() {
            if i.abs_diff(j) <= THEILER_WINDOW {
                continue;
            }
            let d = math::euclidean_distance(&embedded[i], &embedded[j]);
            if d <= 1e-10 {
                continue;
            }
            if d < min_distance {
                min_distance = d;
                nearest_neighbor = Some(j);
            }
        }

        let Some(j) = nearest_neighbor else {
            continue;
        };

        for step in 0..MAX_ITER {
            if i + step >= embedded.len() || j + step >= embedded.len() {
                break;
            }
            let d = math::euclidean_distance(&embedded[i + step], &embedded[j + step]);
            if d > 1e-10 {
                divergence_sums[step] += d.ln();
                divergence_counts[step] += 1;
            }
        }
    }

    let mut steps = Vec::new();
    let mut mean_log_divergence = Vec::new();
    for step in 0..MAX_ITER {
        if divergence_counts[step] >= 5 {
            steps.push(step as f64);
            mean_log_divergence.push(divergence_sums[step] / divergence_counts[step] as f64);
        }
    }

    if steps.len() < 5 {
        return invalid();
    }

    let fit_len = steps.len().min(15);
    let (slope, _intercept, r_squared) =
        math::linear_regression(&steps[..fit_len], &mean_log_divergence[..fit_len]);
    if !slope.is_finite() {
        return invalid();
    }

    LyapunovResult {
        lyapunov_exponent: slope,
        divergence_curve: mean_log_divergence,
        embedding_dim: EMBEDDING_DIM,
        delay: TAU,
        r_squared,
        is_valid: true,
    }
}

pub fn correlation_dimension(data: &[u8]) -> CorrelationDimResult {
    const MAX_EMBEDDING: usize = 8;
    const SUBSAMPLE_MAX: usize = 500;
    const NUM_RADII: usize = 15;

    if data.len() < 1000 {
        return CorrelationDimResult {
            dimension: f64::NAN,
            r_squared: 0.0,
            embedding_dim_used: 0,
            num_points_used: 0,
            is_valid: false,
        };
    }

    let data_f64: Vec<f64> = data.iter().map(|&b| b as f64).collect();
    let subsampled: Vec<f64> = if data_f64.len() > SUBSAMPLE_MAX {
        let step = (data_f64.len() / SUBSAMPLE_MAX).max(1);
        data_f64
            .iter()
            .step_by(step)
            .take(SUBSAMPLE_MAX)
            .copied()
            .collect()
    } else {
        data_f64
    };

    let mut accepted = Vec::new();

    for m in 2..=MAX_EMBEDDING {
        let embedded = time_delay_embedding(&subsampled, m, 1);
        if embedded.len() < 10 {
            continue;
        }

        let n = embedded.len();
        let total_pairs = n * (n - 1) / 2;
        if total_pairs == 0 {
            continue;
        }

        let mut distances = Vec::with_capacity(total_pairs);
        for i in 0..n {
            for j in (i + 1)..n {
                distances.push(math::euclidean_distance(&embedded[i], &embedded[j]));
            }
        }

        distances.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        if distances.len() < 100 {
            continue;
        }

        let r_min = distances[distances.len() / 100];
        let r_max = distances[distances.len() / 2];
        if r_min <= 0.0 || r_max <= r_min {
            continue;
        }

        let mut log_r = Vec::with_capacity(NUM_RADII);
        let mut log_c = Vec::with_capacity(NUM_RADII);
        let ratio = r_max / r_min;

        for k in 0..NUM_RADII {
            let r = r_min * ratio.powf(k as f64 / (NUM_RADII - 1) as f64);
            let count = distances.partition_point(|&d| d < r);
            let c = count as f64 / total_pairs as f64;
            if c > 0.0 {
                log_r.push(r.ln());
                log_c.push(c.ln());
            }
        }

        if log_r.len() < 2 {
            continue;
        }

        let (slope, _intercept, r_squared) = math::linear_regression(&log_r, &log_c);
        if r_squared > 0.9 && slope.is_finite() && slope > 0.0 {
            accepted.push((m, slope, r_squared));
        }
    }

    if accepted.is_empty() {
        return CorrelationDimResult {
            dimension: f64::NAN,
            r_squared: 0.0,
            embedding_dim_used: 0,
            num_points_used: subsampled.len(),
            is_valid: false,
        };
    }

    let take_n = accepted.len().min(3);
    let avg_d2 = accepted[accepted.len() - take_n..]
        .iter()
        .map(|(_, slope, _)| *slope)
        .sum::<f64>()
        / take_n as f64;

    let (last_m, _last_slope, last_r2) = accepted[accepted.len() - 1];

    CorrelationDimResult {
        dimension: avg_d2,
        r_squared: last_r2,
        embedding_dim_used: last_m,
        num_points_used: subsampled.len(),
        is_valid: true,
    }
}

/// Aggregated chaos theory analysis results for a data stream.
#[derive(Debug, Clone, Serialize)]
pub struct ChaosAnalysis {
    pub hurst: HurstResult,
    pub lyapunov: LyapunovResult,
    pub correlation_dimension: CorrelationDimResult,
    pub bientropy: BiEntropyResult,
    pub epiplexity: EpiplexityResult,
}

/// Run all 5 chaos theory analysis methods on the given data.
///
/// This is a separate entry point from `full_analysis()` - chaos methods are
/// computationally expensive (O(n^2)) and should only be run when explicitly requested.
pub fn chaos_analysis(data: &[u8]) -> ChaosAnalysis {
    ChaosAnalysis {
        hurst: hurst_exponent(data),
        lyapunov: lyapunov_exponent(data),
        correlation_dimension: correlation_dimension(data),
        bientropy: bientropy(data),
        epiplexity: epiplexity(data),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn random_data_seeded(n: usize, seed: u64) -> Vec<u8> {
        let mut data = Vec::with_capacity(n);
        let mut state: u64 = seed;
        for _ in 0..n {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            data.push((state >> 33) as u8);
        }
        data
    }

    #[test]
    fn test_hurst_random() {
        let data = random_data_seeded(10000, 0xdeadbeef);
        let result = hurst_exponent(&data);

        assert!(result.is_valid);
        assert!(result.hurst_exponent >= 0.35 && result.hurst_exponent <= 0.65);
    }

    #[test]
    fn test_hurst_constant() {
        let data = vec![42u8; 1000];
        let result = hurst_exponent(&data);

        assert!(!result.is_valid);
    }

    #[test]
    fn test_bientropy_random() {
        let data = random_data_seeded(10000, 0xdeadbeef);
        let result = bientropy(&data);

        assert!(result.is_valid);
        assert!(result.bien > 0.90);
        assert!(result.tbien > 0.90);
    }

    #[test]
    fn test_bientropy_constant() {
        let data = vec![0u8; 1000];
        let result = bientropy(&data);

        assert!(result.is_valid);
        assert!(result.bien < 0.05);
    }

    #[test]
    fn test_bientropy_alternating() {
        let data = vec![0xAAu8; 1000];
        let result = bientropy(&data);

        assert!(result.is_valid);
        assert!(result.bien < 0.50);
    }

    #[test]
    fn test_epiplexity_random() {
        let data = random_data_seeded(10000, 0xdeadbeef);
        let result = epiplexity(&data);

        assert!(result.is_valid);
        assert!(
            result.compression_ratio > 0.95,
            "Random data should have high compression ratio (low compressibility)"
        );
    }

    #[test]
    fn test_epiplexity_patterned() {
        // Repeating [0, 1, 2, 3] pattern — highly compressible
        let data: Vec<u8> = (0..10000u32).map(|i| (i % 4) as u8).collect();
        let result = epiplexity(&data);

        assert!(result.is_valid);
        assert!(
            result.compression_ratio < 0.10,
            "Patterned data should have low compression ratio (high compressibility)"
        );
    }

    #[test]
    fn test_lyapunov_random() {
        let data = random_data_seeded(10000, 0xdeadbeef);
        let result = lyapunov_exponent(&data);

        assert!(result.is_valid);
        assert!(result.lyapunov_exponent.abs() < 0.3);
    }

    #[test]
    fn test_lyapunov_logistic() {
        let mut x = 0.1f64;
        let mut data = Vec::with_capacity(5000);
        for _ in 0..5000 {
            x = 4.0 * x * (1.0 - x);
            data.push((x * 255.0) as u8);
        }

        let result = lyapunov_exponent(&data);
        assert!(result.is_valid);
        assert!(
            result.lyapunov_exponent > 0.3,
            "expected logistic Lyapunov > 0.3, got {}",
            result.lyapunov_exponent
        );
    }

    #[test]
    fn test_lyapunov_short() {
        let data = random_data_seeded(100, 0x12345678);
        let result = lyapunov_exponent(&data);

        assert!(!result.is_valid);
    }

    #[test]
    fn test_corrdim_random() {
        let data = random_data_seeded(5000, 0xdeadbeef);
        let result = correlation_dimension(&data);
        assert!(
            result.is_valid,
            "correlation_dimension should be valid for 5000 random bytes"
        );
        assert!(
            result.dimension > 2.0,
            "random data should have D2 > 2.0, got {}",
            result.dimension
        );
    }

    #[test]
    fn test_corrdim_short() {
        let data = random_data_seeded(500, 0x12345678);
        let result = correlation_dimension(&data);
        assert!(
            !result.is_valid,
            "correlation_dimension should be invalid for 500 bytes (below 1000 minimum)"
        );
    }

    #[test]
    fn test_empty_input() {
        let data: &[u8] = &[];
        let h = hurst_exponent(data);
        let l = lyapunov_exponent(data);
        let c = correlation_dimension(data);
        let b = bientropy(data);
        let e = epiplexity(data);
        assert!(!h.is_valid);
        assert!(!l.is_valid);
        assert!(!c.is_valid);
        assert!(!b.is_valid);
        assert!(!e.is_valid);
    }

    #[test]
    fn test_single_byte() {
        let data: &[u8] = &[42];
        let h = hurst_exponent(data);
        let l = lyapunov_exponent(data);
        let c = correlation_dimension(data);
        let e = epiplexity(data);
        assert!(!h.is_valid);
        assert!(!l.is_valid);
        assert!(!c.is_valid);
        assert!(!e.is_valid);
    }

    #[test]
    fn test_chaos_analysis_orchestrator() {
        let data = random_data_seeded(5000, 0xdeadbeef);
        let result = chaos_analysis(&data);
        assert!(result.hurst.is_valid);
        assert!(result.lyapunov.is_valid);
        assert!(result.correlation_dimension.is_valid);
        assert!(result.bientropy.is_valid);
        assert!(result.epiplexity.is_valid);
    }

    #[test]
    fn test_performance_100k() {
        let data = random_data_seeded(100_000, 0xCAFEBABE);
        let _result = chaos_analysis(&data);
    }
}

//! NIST SP 800-22 inspired randomness test battery.
//!
//! Provides 31 statistical tests for evaluating the quality of random byte sequences.
//! Each test returns a [`TestResult`] with a p-value (where applicable), a pass/fail
//! determination, and a letter grade (A through F).

use flate2::Compression;
use flate2::write::ZlibEncoder;
use rustfft::{FftPlanner, num_complex::Complex};
use statrs::distribution::{ChiSquared, ContinuousCDF, DiscreteCDF, Normal, Poisson};
use statrs::function::erf::erfc;
use std::collections::HashMap;
use std::f64::consts::PI;
use std::io::Write;

// ═══════════════════════════════════════════════════════════════════════════════
// Core types
// ═══════════════════════════════════════════════════════════════════════════════

/// Result of a single randomness test.
#[derive(Debug, Clone)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    pub p_value: Option<f64>,
    pub statistic: f64,
    pub details: String,
    pub grade: char,
}

impl TestResult {
    /// Assign a letter grade based on p-value.
    ///
    /// - A: p >= 0.1
    /// - B: p >= 0.01
    /// - C: p >= 0.001
    /// - D: p >= 0.0001
    /// - F: otherwise or None
    pub fn grade_from_p(p: Option<f64>) -> char {
        match p {
            Some(p) if p >= 0.1 => 'A',
            Some(p) if p >= 0.01 => 'B',
            Some(p) if p >= 0.001 => 'C',
            Some(p) if p >= 0.0001 => 'D',
            _ => 'F',
        }
    }

    /// Determine pass/fail from p-value against a threshold (default 0.01).
    pub fn pass_from_p(p: Option<f64>, threshold: f64) -> bool {
        match p {
            Some(p) => p >= threshold,
            None => false,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

/// Unpack a byte slice into individual bits (MSB first per byte).
fn to_bits(data: &[u8]) -> Vec<u8> {
    let mut bits = Vec::with_capacity(data.len() * 8);
    for &byte in data {
        for shift in (0..8).rev() {
            bits.push((byte >> shift) & 1);
        }
    }
    bits
}

/// Return a failing `TestResult` when data is too short.
fn insufficient(name: &str, needed: usize, got: usize) -> TestResult {
    TestResult {
        name: name.to_string(),
        passed: false,
        p_value: None,
        statistic: 0.0,
        details: format!("Insufficient data: need {needed}, got {got}"),
        grade: 'F',
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 1. FREQUENCY TESTS
// ═══════════════════════════════════════════════════════════════════════════════

/// Test 1: Monobit frequency -- proportion of 1s vs 0s should be ~50%.
pub fn monobit_frequency(data: &[u8]) -> TestResult {
    let name = "Monobit Frequency";
    let bits = to_bits(data);
    let n = bits.len();
    if n < 100 {
        return insufficient(name, 100, n);
    }
    let s: i64 = bits
        .iter()
        .map(|&b| if b == 1 { 1i64 } else { -1i64 })
        .sum();
    let s_obs = (s as f64).abs() / (n as f64).sqrt();
    let p = erfc(s_obs / 2.0_f64.sqrt());
    TestResult {
        name: name.to_string(),
        passed: TestResult::pass_from_p(Some(p), 0.01),
        p_value: Some(p),
        statistic: s_obs,
        details: format!("S={s}, n={n}"),
        grade: TestResult::grade_from_p(Some(p)),
    }
}

/// Test 2: Block frequency -- frequency within 128-bit blocks. Chi-squared test.
pub fn block_frequency(data: &[u8]) -> TestResult {
    let name = "Block Frequency";
    let block_size: usize = 128;
    let bits = to_bits(data);
    let n = bits.len();
    let num_blocks = n / block_size;
    if num_blocks < 10 {
        return insufficient(name, block_size * 10, n);
    }
    let mut chi2 = 0.0;
    for i in 0..num_blocks {
        let start = i * block_size;
        let ones: usize = bits[start..start + block_size]
            .iter()
            .map(|&b| b as usize)
            .sum();
        let proportion = ones as f64 / block_size as f64;
        chi2 += (proportion - 0.5) * (proportion - 0.5);
    }
    chi2 *= 4.0 * block_size as f64;
    let dist = ChiSquared::new(num_blocks as f64).unwrap();
    let p = dist.sf(chi2);
    TestResult {
        name: name.to_string(),
        passed: TestResult::pass_from_p(Some(p), 0.01),
        p_value: Some(p),
        statistic: chi2,
        details: format!("blocks={num_blocks}, M={block_size}"),
        grade: TestResult::grade_from_p(Some(p)),
    }
}

/// Test 3: Byte frequency -- chi-squared on byte value distribution (256 bins).
pub fn byte_frequency(data: &[u8]) -> TestResult {
    let name = "Byte Frequency";
    let n = data.len();
    if n < 256 {
        return insufficient(name, 256, n);
    }
    let mut hist = [0u64; 256];
    for &b in data {
        hist[b as usize] += 1;
    }
    let expected = n as f64 / 256.0;
    let chi2: f64 = hist
        .iter()
        .map(|&c| {
            let diff = c as f64 - expected;
            diff * diff / expected
        })
        .sum();
    let dist = ChiSquared::new(255.0).unwrap();
    let p = dist.sf(chi2);
    TestResult {
        name: name.to_string(),
        passed: TestResult::pass_from_p(Some(p), 0.01),
        p_value: Some(p),
        statistic: chi2,
        details: format!("n={n}, expected_per_bin={expected:.1}"),
        grade: TestResult::grade_from_p(Some(p)),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. RUNS TESTS
// ═══════════════════════════════════════════════════════════════════════════════

/// Test 4: Runs test -- number of uninterrupted runs of 0s or 1s.
pub fn runs_test(data: &[u8]) -> TestResult {
    let name = "Runs Test";
    let bits = to_bits(data);
    let n = bits.len();
    if n < 100 {
        return insufficient(name, 100, n);
    }
    let ones: usize = bits.iter().map(|&b| b as usize).sum();
    let prop = ones as f64 / n as f64;
    if (prop - 0.5).abs() >= 2.0 / (n as f64).sqrt() {
        return TestResult {
            name: name.to_string(),
            passed: false,
            p_value: Some(0.0),
            statistic: 0.0,
            details: format!("Pre-test failed: proportion={prop:.4}"),
            grade: 'F',
        };
    }
    let mut runs: usize = 1;
    for i in 1..n {
        if bits[i] != bits[i - 1] {
            runs += 1;
        }
    }
    let expected = 2.0 * n as f64 * prop * (1.0 - prop) + 1.0;
    let std = 2.0 * (2.0 * n as f64).sqrt() * prop * (1.0 - prop);
    if std < 1e-10 {
        return TestResult {
            name: name.to_string(),
            passed: false,
            p_value: Some(0.0),
            statistic: 0.0,
            details: "Zero variance".to_string(),
            grade: 'F',
        };
    }
    let z = (runs as f64 - expected).abs() / std;
    let p = erfc(z / 2.0_f64.sqrt());
    TestResult {
        name: name.to_string(),
        passed: TestResult::pass_from_p(Some(p), 0.01),
        p_value: Some(p),
        statistic: z,
        details: format!("runs={runs}, expected={expected:.0}"),
        grade: TestResult::grade_from_p(Some(p)),
    }
}

/// Test 5: Longest run of ones -- within 8-bit blocks, chi-squared against theoretical probs.
pub fn longest_run_of_ones(data: &[u8]) -> TestResult {
    let name = "Longest Run of Ones";
    let bits = to_bits(data);
    let n = bits.len();
    if n < 128 {
        return insufficient(name, 128, n);
    }
    let block_size = 8;
    let num_blocks = n / block_size;

    // For each block, find the longest run of 1s
    let mut observed = [0u64; 4]; // bins: 0, 1, 2, >=3
    for i in 0..num_blocks {
        let start = i * block_size;
        let block = &bits[start..start + block_size];
        let mut max_run = 0u32;
        let mut current_run = 0u32;
        for &bit in block {
            if bit == 1 {
                current_run += 1;
                if current_run > max_run {
                    max_run = current_run;
                }
            } else {
                current_run = 0;
            }
        }
        match max_run {
            0 => observed[0] += 1,
            1 => observed[1] += 1,
            2 => observed[2] += 1,
            _ => observed[3] += 1,
        }
    }

    // Theoretical probabilities for M=8
    let probs = [0.2148, 0.3672, 0.2305, 0.1875];
    let mut chi2 = 0.0;
    for i in 0..4 {
        let expected = probs[i] * num_blocks as f64;
        if expected > 0.0 {
            let diff = observed[i] as f64 - expected;
            chi2 += diff * diff / expected;
        }
    }
    let dist = ChiSquared::new(3.0).unwrap();
    let p = dist.sf(chi2);
    TestResult {
        name: name.to_string(),
        passed: TestResult::pass_from_p(Some(p), 0.01),
        p_value: Some(p),
        statistic: chi2,
        details: format!("blocks={num_blocks}, M={block_size}"),
        grade: TestResult::grade_from_p(Some(p)),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. SERIAL TESTS
// ═══════════════════════════════════════════════════════════════════════════════

/// Helper: compute psi-squared for the serial test.
fn psi_sq(bits: &[u8], n: usize, m: usize) -> f64 {
    if m < 1 {
        return 0.0;
    }
    let num_patterns = 1usize << m;
    let mut counts = vec![0u64; num_patterns];
    for i in 0..n {
        let mut val = 0usize;
        for j in 0..m {
            val = (val << 1) | bits[(i + j) % n] as usize;
        }
        counts[val] += 1;
    }
    let sum_sq: f64 = counts.iter().map(|&c| (c as f64) * (c as f64)).sum();
    sum_sq * (num_patterns as f64) / (n as f64) - n as f64
}

/// Test 6: Serial test -- frequency of overlapping m-bit patterns (m=4).
pub fn serial_test(data: &[u8]) -> TestResult {
    let name = "Serial Test";
    let m = 4usize;
    let mut bits = to_bits(data);
    let mut n = bits.len();
    if n > 20000 {
        bits.truncate(20000);
        n = 20000;
    }
    if n < (1 << m) + 10 {
        return insufficient(name, (1 << m) + 10, n);
    }

    let psi_m = psi_sq(&bits, n, m);
    let psi_m1 = psi_sq(&bits, n, m - 1);
    let _psi_m2 = if m >= 2 { psi_sq(&bits, n, m - 2) } else { 0.0 };
    let delta1 = psi_m - psi_m1;

    let df = (1u64 << (m - 1)) as f64;
    let dist = ChiSquared::new(df).unwrap();
    let p = dist.sf(delta1);
    TestResult {
        name: name.to_string(),
        passed: TestResult::pass_from_p(Some(p), 0.01),
        p_value: Some(p),
        statistic: delta1,
        details: format!("m={m}, n_bits={n}"),
        grade: TestResult::grade_from_p(Some(p)),
    }
}

/// Test 7: Approximate entropy -- compare m and m+1 bit pattern frequencies (m=3).
pub fn approximate_entropy(data: &[u8]) -> TestResult {
    let name = "Approximate Entropy";
    let m = 3usize;
    let mut bits = to_bits(data);
    let mut n = bits.len();
    if n > 20000 {
        bits.truncate(20000);
        n = 20000;
    }
    if n < 64 {
        return insufficient(name, 64, n);
    }

    let phi = |block_len: usize| -> f64 {
        let num_patterns = 1usize << block_len;
        let mut counts = vec![0u64; num_patterns];
        for i in 0..n {
            let mut val = 0usize;
            for j in 0..block_len {
                val = (val << 1) | bits[(i + j) % n] as usize;
            }
            counts[val] += 1;
        }
        let mut sum = 0.0;
        for &c in &counts {
            if c > 0 {
                let p = c as f64 / n as f64;
                sum += p * p.log2();
            }
        }
        sum
    };

    let phi_m = phi(m);
    let phi_m1 = phi(m + 1);
    let apen = phi_m - phi_m1;
    // chi2 = 2 * n * (log2(2) - apen) = 2 * n * (1.0 - apen)
    let chi2 = 2.0 * n as f64 * (1.0 - apen);

    let df = (1u64 << m) as f64;
    let dist = ChiSquared::new(df).unwrap();
    let p = dist.sf(chi2);
    TestResult {
        name: name.to_string(),
        passed: TestResult::pass_from_p(Some(p), 0.01),
        p_value: Some(p),
        statistic: chi2,
        details: format!("ApEn={apen:.6}, m={m}"),
        grade: TestResult::grade_from_p(Some(p)),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. SPECTRAL TESTS
// ═══════════════════════════════════════════════════════════════════════════════

/// Test 8: DFT spectral -- detect periodic features via FFT.
pub fn dft_spectral(data: &[u8]) -> TestResult {
    let name = "DFT Spectral";
    let bits = to_bits(data);
    let n = bits.len();
    if n < 64 {
        return insufficient(name, 64, n);
    }

    let mut buffer: Vec<Complex<f64>> = bits
        .iter()
        .map(|&b| Complex {
            re: if b == 1 { 1.0 } else { -1.0 },
            im: 0.0,
        })
        .collect();

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(n);
    fft.process(&mut buffer);

    let half = n / 2;
    let magnitudes: Vec<f64> = buffer[..half].iter().map(|c| c.norm()).collect();

    let threshold = (2.995732274 * n as f64).sqrt();
    let n0 = 0.95 * half as f64;
    let n1 = magnitudes.iter().filter(|&&m| m < threshold).count() as f64;
    let d = (n1 - n0) / (n as f64 * 0.95 * 0.05 / 4.0).sqrt();
    let p = erfc(d.abs() / 2.0_f64.sqrt());
    TestResult {
        name: name.to_string(),
        passed: TestResult::pass_from_p(Some(p), 0.01),
        p_value: Some(p),
        statistic: d,
        details: format!("peaks_below_threshold={}/{half}", n1 as u64),
        grade: TestResult::grade_from_p(Some(p)),
    }
}

/// Test 9: Spectral flatness -- geometric/arithmetic mean ratio of power spectrum.
pub fn spectral_flatness(data: &[u8]) -> TestResult {
    let name = "Spectral Flatness";
    let n = data.len();
    if n < 64 {
        return insufficient(name, 64, n);
    }

    let mean_val: f64 = data.iter().map(|&b| b as f64).sum::<f64>() / n as f64;
    let mut buffer: Vec<Complex<f64>> = data
        .iter()
        .map(|&b| Complex {
            re: b as f64 - mean_val,
            im: 0.0,
        })
        .collect();

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(n);
    fft.process(&mut buffer);

    // Power spectrum, skip DC bin (index 0)
    let half = n / 2;
    if half < 2 {
        return insufficient(name, 64, n);
    }
    let power: Vec<f64> = buffer[1..half]
        .iter()
        .map(|c| c.norm_sqr() + 1e-15)
        .collect();

    if power.is_empty() {
        return insufficient(name, 64, n);
    }

    let log_sum: f64 = power.iter().map(|&p| p.ln()).sum();
    let geo_mean = (log_sum / power.len() as f64).exp();
    let arith_mean: f64 = power.iter().sum::<f64>() / power.len() as f64;
    let flatness = geo_mean / arith_mean;

    let passed = flatness > 0.5;
    let grade = if flatness > 0.8 {
        'A'
    } else if flatness > 0.6 {
        'B'
    } else if flatness > 0.4 {
        'C'
    } else if flatness > 0.2 {
        'D'
    } else {
        'F'
    };
    TestResult {
        name: name.to_string(),
        passed,
        p_value: None,
        statistic: flatness,
        details: format!("flatness={flatness:.4} (1.0=white noise)"),
        grade,
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. ENTROPY TESTS
// ═══════════════════════════════════════════════════════════════════════════════

/// Test 10: Shannon entropy -- bits per byte (max 8.0).
pub fn shannon_entropy(data: &[u8]) -> TestResult {
    let name = "Shannon Entropy";
    let n = data.len();
    if n < 16 {
        return insufficient(name, 16, n);
    }
    let mut hist = [0u64; 256];
    for &b in data {
        hist[b as usize] += 1;
    }
    let mut h = 0.0;
    for &c in &hist {
        if c > 0 {
            let p = c as f64 / n as f64;
            h -= p * p.log2();
        }
    }
    let ratio = h / 8.0;
    let grade = if ratio > 0.95 {
        'A'
    } else if ratio > 0.85 {
        'B'
    } else if ratio > 0.7 {
        'C'
    } else if ratio > 0.5 {
        'D'
    } else {
        'F'
    };
    TestResult {
        name: name.to_string(),
        passed: ratio > 0.85,
        p_value: None,
        statistic: h,
        details: format!("{h:.4} / 8.0 bits ({:.1}%)", ratio * 100.0),
        grade,
    }
}

/// Test 11: Min-entropy (NIST SP 800-90B): -log2(p_max).
pub fn min_entropy(data: &[u8]) -> TestResult {
    let name = "Min-Entropy";
    let n = data.len();
    if n < 16 {
        return insufficient(name, 16, n);
    }
    let mut hist = [0u64; 256];
    for &b in data {
        hist[b as usize] += 1;
    }
    let p_max = *hist.iter().max().unwrap() as f64 / n as f64;
    let h_min = -(p_max + 1e-15).log2();
    let ratio = h_min / 8.0;
    let grade = if ratio > 0.9 {
        'A'
    } else if ratio > 0.75 {
        'B'
    } else if ratio > 0.5 {
        'C'
    } else if ratio > 0.25 {
        'D'
    } else {
        'F'
    };
    TestResult {
        name: name.to_string(),
        passed: ratio > 0.7,
        p_value: None,
        statistic: h_min,
        details: format!("{h_min:.4} / 8.0 bits ({:.1}%)", ratio * 100.0),
        grade,
    }
}

/// Test 12: Permutation entropy -- complexity of ordinal patterns (order=4).
pub fn permutation_entropy(data: &[u8]) -> TestResult {
    let name = "Permutation Entropy";
    let order = 4usize;
    let n = data.len();
    if n < order + 10 {
        return insufficient(name, order + 10, n);
    }
    let arr: Vec<f64> = data.iter().map(|&b| b as f64).collect();

    let mut patterns: HashMap<Vec<usize>, u64> = HashMap::new();
    for i in 0..n - order {
        let window = &arr[i..i + order];
        let mut indices: Vec<usize> = (0..order).collect();
        indices.sort_by(|&a, &b| {
            window[a]
                .partial_cmp(&window[b])
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.cmp(&b))
        });
        *patterns.entry(indices).or_insert(0) += 1;
    }

    let total: u64 = patterns.values().sum();
    let mut h = 0.0;
    for &c in patterns.values() {
        let p = c as f64 / total as f64;
        h -= p * p.log2();
    }
    // factorial(4) = 24
    let h_max = 24.0_f64.log2();
    let normalized = if h_max > 0.0 { h / h_max } else { 0.0 };
    let grade = if normalized > 0.95 {
        'A'
    } else if normalized > 0.85 {
        'B'
    } else if normalized > 0.7 {
        'C'
    } else if normalized > 0.5 {
        'D'
    } else {
        'F'
    };
    TestResult {
        name: name.to_string(),
        passed: normalized > 0.85,
        p_value: None,
        statistic: normalized,
        details: format!("PE={h:.4}/{h_max:.4} = {normalized:.4}"),
        grade,
    }
}

/// Test 13: Compression ratio -- zlib compression ratio (random ~ 1.0+).
pub fn compression_ratio(data: &[u8]) -> TestResult {
    let name = "Compression Ratio";
    let n = data.len();
    if n < 32 {
        return insufficient(name, 32, n);
    }
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(data).unwrap();
    let compressed = encoder.finish().unwrap();
    let ratio = compressed.len() as f64 / n as f64;
    let grade = if ratio > 0.95 {
        'A'
    } else if ratio > 0.85 {
        'B'
    } else if ratio > 0.7 {
        'C'
    } else if ratio > 0.5 {
        'D'
    } else {
        'F'
    };
    TestResult {
        name: name.to_string(),
        passed: ratio > 0.85,
        p_value: None,
        statistic: ratio,
        details: format!("{}/{n} = {ratio:.4}", compressed.len()),
        grade,
    }
}

/// Test 14: Kolmogorov complexity -- compression at levels 1 and 9, compute complexity and spread.
pub fn kolmogorov_complexity(data: &[u8]) -> TestResult {
    let name = "Kolmogorov Complexity";
    let n = data.len();
    if n < 32 {
        return insufficient(name, 32, n);
    }

    let compress_at = |level: u32| -> usize {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(level));
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap().len()
    };

    let c1 = compress_at(1);
    let c9 = compress_at(9);
    let complexity = c9 as f64 / n as f64;
    let spread = (c1 as f64 - c9 as f64) / n as f64;
    let grade = if complexity > 0.95 {
        'A'
    } else if complexity > 0.85 {
        'B'
    } else if complexity > 0.7 {
        'C'
    } else if complexity > 0.5 {
        'D'
    } else {
        'F'
    };
    TestResult {
        name: name.to_string(),
        passed: complexity > 0.85,
        p_value: None,
        statistic: complexity,
        details: format!("K~={complexity:.4}, spread={spread:.4}"),
        grade,
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. CORRELATION TESTS
// ═══════════════════════════════════════════════════════════════════════════════

/// Test 15: Autocorrelation -- at lags 1-50. Count violations of 2/sqrt(n) threshold.
pub fn autocorrelation(data: &[u8]) -> TestResult {
    let name = "Autocorrelation";
    let max_lag = 50usize;
    let n = data.len();
    if n < max_lag + 10 {
        return insufficient(name, max_lag + 10, n);
    }
    let arr: Vec<f64> = data.iter().map(|&b| b as f64).collect();
    let mean: f64 = arr.iter().sum::<f64>() / n as f64;
    let var: f64 = arr.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n as f64;
    if var < 1e-10 {
        return TestResult {
            name: name.to_string(),
            passed: false,
            p_value: None,
            statistic: 1.0,
            details: "Zero variance".to_string(),
            grade: 'F',
        };
    }
    let threshold = 2.0 / (n as f64).sqrt();
    let mut max_corr = 0.0f64;
    let mut violations = 0u64;
    for lag in 1..=max_lag.min(n - 1) {
        let mut sum = 0.0;
        let count = n - lag;
        for i in 0..count {
            sum += (arr[i] - mean) * (arr[i + lag] - mean);
        }
        let c = sum / (count as f64 * var);
        if c.abs() > max_corr {
            max_corr = c.abs();
        }
        if c.abs() > threshold {
            violations += 1;
        }
    }
    let expected_violations = 0.05 * max_lag as f64;
    let lambda = expected_violations.max(1.0);
    let p = if violations > 0 {
        let poisson = Poisson::new(lambda).unwrap();
        poisson.sf(violations - 1)
    } else {
        1.0
    };
    TestResult {
        name: name.to_string(),
        passed: TestResult::pass_from_p(Some(p), 0.01),
        p_value: Some(p),
        statistic: max_corr,
        details: format!("violations={violations}/{max_lag}, max|r|={max_corr:.4}"),
        grade: TestResult::grade_from_p(Some(p)),
    }
}

/// Test 16: Serial correlation -- adjacent value correlation. Z-test.
pub fn serial_correlation(data: &[u8]) -> TestResult {
    let name = "Serial Correlation";
    let n = data.len();
    if n < 20 {
        return insufficient(name, 20, n);
    }
    let arr: Vec<f64> = data.iter().map(|&b| b as f64).collect();
    let mean: f64 = arr.iter().sum::<f64>() / n as f64;
    let var: f64 = arr.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n as f64;
    if var < 1e-10 {
        return TestResult {
            name: name.to_string(),
            passed: false,
            p_value: None,
            statistic: 1.0,
            details: "Zero variance".to_string(),
            grade: 'F',
        };
    }
    let mut sum = 0.0;
    for i in 0..n - 1 {
        sum += (arr[i] - mean) * (arr[i + 1] - mean);
    }
    let r = sum / ((n - 1) as f64 * var);
    let z = r * (n as f64).sqrt();
    let norm = Normal::standard();
    let p = 2.0 * (1.0 - norm.cdf(z.abs()));
    TestResult {
        name: name.to_string(),
        passed: TestResult::pass_from_p(Some(p), 0.01),
        p_value: Some(p),
        statistic: r.abs(),
        details: format!("r={r:.6}, z={z:.4}"),
        grade: TestResult::grade_from_p(Some(p)),
    }
}

/// Test 17: Lag-N correlation -- correlation at lags [1, 2, 4, 8, 16, 32].
pub fn lag_n_correlation(data: &[u8]) -> TestResult {
    let name = "Lag-N Correlation";
    let lags: &[usize] = &[1, 2, 4, 8, 16, 32];
    let n = data.len();
    let max_lag = *lags.iter().max().unwrap();
    if n < max_lag + 10 {
        return insufficient(name, max_lag + 10, n);
    }
    let arr: Vec<f64> = data.iter().map(|&b| b as f64).collect();
    let mean: f64 = arr.iter().sum::<f64>() / n as f64;
    let var: f64 = arr.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n as f64;
    if var < 1e-10 {
        return TestResult {
            name: name.to_string(),
            passed: false,
            p_value: None,
            statistic: 1.0,
            details: "Zero variance".to_string(),
            grade: 'F',
        };
    }
    let threshold = 2.0 / (n as f64).sqrt();
    let mut max_corr = 0.0f64;
    let mut details_parts = Vec::new();
    for &lag in lags {
        if lag >= n {
            continue;
        }
        let mut sum = 0.0;
        let count = n - lag;
        for i in 0..count {
            sum += (arr[i] - mean) * (arr[i + lag] - mean);
        }
        let c = sum / (count as f64 * var);
        if c.abs() > max_corr {
            max_corr = c.abs();
        }
        details_parts.push(format!("lag{lag}={c:.4}"));
    }
    let passed = max_corr < threshold;
    let grade = if max_corr < threshold * 0.5 {
        'A'
    } else if max_corr < threshold {
        'B'
    } else if max_corr < threshold * 2.0 {
        'C'
    } else if max_corr < threshold * 4.0 {
        'D'
    } else {
        'F'
    };
    TestResult {
        name: name.to_string(),
        passed,
        p_value: None,
        statistic: max_corr,
        details: details_parts.join(", "),
        grade,
    }
}

/// Test 18: Cross-correlation -- even vs odd byte independence. Pearson r.
pub fn cross_correlation(data: &[u8]) -> TestResult {
    let name = "Cross-Correlation";
    let n = data.len();
    if n < 100 {
        return insufficient(name, 100, n);
    }
    let even: Vec<f64> = data.iter().step_by(2).map(|&b| b as f64).collect();
    let odd: Vec<f64> = data.iter().skip(1).step_by(2).map(|&b| b as f64).collect();
    let min_len = even.len().min(odd.len());
    if min_len < 2 {
        return insufficient(name, 100, n);
    }
    let even = &even[..min_len];
    let odd = &odd[..min_len];

    let mean_e: f64 = even.iter().sum::<f64>() / min_len as f64;
    let mean_o: f64 = odd.iter().sum::<f64>() / min_len as f64;
    let mut cov = 0.0;
    let mut var_e = 0.0;
    let mut var_o = 0.0;
    for i in 0..min_len {
        let de = even[i] - mean_e;
        let do_ = odd[i] - mean_o;
        cov += de * do_;
        var_e += de * de;
        var_o += do_ * do_;
    }
    let denom = (var_e * var_o).sqrt();
    if denom < 1e-10 {
        return TestResult {
            name: name.to_string(),
            passed: false,
            p_value: None,
            statistic: 0.0,
            details: "Zero variance in one or both halves".to_string(),
            grade: 'F',
        };
    }
    let r = cov / denom;

    // For large n, t ~ N(0,1)
    let t = r * ((min_len as f64 - 2.0) / (1.0 - r * r).max(1e-15)).sqrt();
    let norm = Normal::standard();
    let p = 2.0 * (1.0 - norm.cdf(t.abs()));
    TestResult {
        name: name.to_string(),
        passed: TestResult::pass_from_p(Some(p), 0.01),
        p_value: Some(p),
        statistic: r.abs(),
        details: format!("r={r:.6} (even vs odd bytes)"),
        grade: TestResult::grade_from_p(Some(p)),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. DISTRIBUTION TESTS
// ═══════════════════════════════════════════════════════════════════════════════

/// Test 19: Kolmogorov-Smirnov test vs uniform distribution.
pub fn ks_test(data: &[u8]) -> TestResult {
    let name = "Kolmogorov-Smirnov";
    let n = data.len();
    if n < 50 {
        return insufficient(name, 50, n);
    }
    let mut normalized: Vec<f64> = data.iter().map(|&b| b as f64 / 255.0).collect();
    normalized.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    // KS statistic: max |F_n(x) - F(x)|
    let mut d_max = 0.0f64;
    let nf = n as f64;
    for (i, &x) in normalized.iter().enumerate() {
        let f_n_plus = (i + 1) as f64 / nf;
        let f_n_minus = i as f64 / nf;
        let f_x = x.clamp(0.0, 1.0);
        let d1 = (f_n_plus - f_x).abs();
        let d2 = (f_n_minus - f_x).abs();
        d_max = d_max.max(d1).max(d2);
    }

    // Asymptotic KS p-value (Kolmogorov distribution)
    let sqrt_n = nf.sqrt();
    let lambda = (sqrt_n + 0.12 + 0.11 / sqrt_n) * d_max;
    let mut p = 0.0;
    for k in 1..=100i32 {
        let sign = if k % 2 == 0 { -1.0 } else { 1.0 };
        p += sign * (-2.0 * (k as f64 * lambda).powi(2)).exp();
    }
    p = (2.0 * p).clamp(0.0, 1.0);

    TestResult {
        name: name.to_string(),
        passed: TestResult::pass_from_p(Some(p), 0.01),
        p_value: Some(p),
        statistic: d_max,
        details: format!("D={d_max:.6}, n={n}"),
        grade: TestResult::grade_from_p(Some(p)),
    }
}

/// Test 20: Anderson-Darling -- A-squared statistic for uniform. Critical values:
/// 1.933 (5%), 2.492 (2.5%), 3.857 (1%).
pub fn anderson_darling(data: &[u8]) -> TestResult {
    let name = "Anderson-Darling";
    let n = data.len();
    if n < 50 {
        return insufficient(name, 50, n);
    }
    // Map bytes to (0, 1): (value + 0.5) / 256
    let mut sorted: Vec<f64> = data.iter().map(|&b| (b as f64 + 0.5) / 256.0).collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let nf = n as f64;
    let mut s = 0.0;
    for i in 0..n {
        let idx = (i + 1) as f64;
        let u = sorted[i].clamp(1e-15, 1.0 - 1e-15);
        let u_rev = sorted[n - 1 - i].clamp(1e-15, 1.0 - 1e-15);
        s += (2.0 * idx - 1.0) * (u.ln() + (1.0 - u_rev).ln());
    }
    let a2 = -nf - s / nf;
    let a2_star = a2 * (1.0 + 0.75 / nf + 2.25 / (nf * nf));

    let passed = a2_star < 2.492;
    let grade = if a2_star < 1.248 {
        'A'
    } else if a2_star < 1.933 {
        'B'
    } else if a2_star < 2.492 {
        'C'
    } else if a2_star < 3.857 {
        'D'
    } else {
        'F'
    };
    TestResult {
        name: name.to_string(),
        passed,
        p_value: None,
        statistic: a2_star,
        details: format!("A^2*={a2_star:.4}, 5% critical=2.492"),
        grade,
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. PATTERN TESTS
// ═══════════════════════════════════════════════════════════════════════════════

/// Test 21: Overlapping template -- frequency of overlapping bit pattern (1,1,1,1).
pub fn overlapping_template(data: &[u8]) -> TestResult {
    let name = "Overlapping Template";
    let template: &[u8] = &[1, 1, 1, 1];
    let m = template.len();
    let bits = to_bits(data);
    let n = bits.len();
    if n < 1000 {
        return insufficient(name, 1000, n);
    }

    let mut count = 0u64;
    for i in 0..=n - m {
        if bits[i..i + m] == *template {
            count += 1;
        }
    }
    let expected = (n - m + 1) as f64 / (1u64 << m) as f64;
    let std = (expected * (1.0 - 1.0 / (1u64 << m) as f64)).sqrt();
    if std < 1e-10 {
        return TestResult {
            name: name.to_string(),
            passed: false,
            p_value: None,
            statistic: 0.0,
            details: "Zero std".to_string(),
            grade: 'F',
        };
    }
    let z = (count as f64 - expected) / std;
    let norm = Normal::standard();
    let p = 2.0 * (1.0 - norm.cdf(z.abs()));
    TestResult {
        name: name.to_string(),
        passed: TestResult::pass_from_p(Some(p), 0.01),
        p_value: Some(p),
        statistic: z.abs(),
        details: format!("count={count}, expected={expected:.0}"),
        grade: TestResult::grade_from_p(Some(p)),
    }
}

/// Test 22: Non-overlapping template -- non-overlapping occurrences of (0,0,1,1).
pub fn non_overlapping_template(data: &[u8]) -> TestResult {
    let name = "Non-overlapping Template";
    let template: &[u8] = &[0, 0, 1, 1];
    let m = template.len();
    let bits = to_bits(data);
    let n = bits.len();
    if n < 1000 {
        return insufficient(name, 1000, n);
    }

    let mut count = 0u64;
    let mut i = 0;
    while i + m <= n {
        if bits[i..i + m] == *template {
            count += 1;
            i += m;
        } else {
            i += 1;
        }
    }
    let expected = n as f64 / (1u64 << m) as f64;
    let var =
        n as f64 * (1.0 / (1u64 << m) as f64 - (2.0 * m as f64 - 1.0) / (1u64 << (2 * m)) as f64);
    let var = if var <= 0.0 { 1.0 } else { var };
    let z = (count as f64 - expected) / var.sqrt();
    let norm = Normal::standard();
    let p = 2.0 * (1.0 - norm.cdf(z.abs()));
    TestResult {
        name: name.to_string(),
        passed: TestResult::pass_from_p(Some(p), 0.01),
        p_value: Some(p),
        statistic: z.abs(),
        details: format!("count={count}, expected={expected:.0}"),
        grade: TestResult::grade_from_p(Some(p)),
    }
}

/// Test 23: Maurer's universal statistical test (L=6, Q=640).
pub fn maurers_universal(data: &[u8]) -> TestResult {
    let name = "Maurer's Universal";
    let l = 6usize;
    let q = 640usize;
    let bits = to_bits(data);
    let n_bits = bits.len();
    let total_blocks = n_bits / l;
    if total_blocks <= q {
        return insufficient(name, (q + 100) * l, n_bits);
    }
    let k = total_blocks - q;
    if k < 100 || q < 10 * (1 << l) {
        return insufficient(name, (q + 100) * l, n_bits);
    }

    let num_patterns = 1usize << l;
    let mut table = vec![0usize; num_patterns];

    // Initialization phase
    for i in 0..q {
        let mut block = 0usize;
        for j in 0..l {
            block = (block << 1) | bits[i * l + j] as usize;
        }
        table[block] = i + 1;
    }

    // Test phase
    let mut total = 0.0f64;
    for i in q..q + k {
        let mut block = 0usize;
        for j in 0..l {
            block = (block << 1) | bits[i * l + j] as usize;
        }
        let prev = table[block];
        let distance = if prev > 0 {
            (i + 1 - prev) as f64
        } else {
            (i + 1) as f64
        };
        total += distance.log2();
        table[block] = i + 1;
    }

    let fn_val = total / k as f64;
    let expected = 5.2177052;
    let variance = 2.954;
    let sigma = (variance / k as f64).sqrt();
    let z = (fn_val - expected).abs() / sigma.max(1e-10);
    let p = erfc(z / 2.0_f64.sqrt());
    TestResult {
        name: name.to_string(),
        passed: TestResult::pass_from_p(Some(p), 0.01),
        p_value: Some(p),
        statistic: fn_val,
        details: format!("fn={fn_val:.4}, expected={expected:.4}, L={l}"),
        grade: TestResult::grade_from_p(Some(p)),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. ADVANCED TESTS
// ═══════════════════════════════════════════════════════════════════════════════

/// GF(2) Gaussian elimination to compute binary matrix rank.
fn gf2_rank(matrix: &[u8], rows: usize, cols: usize) -> usize {
    let mut m: Vec<Vec<u8>> = (0..rows)
        .map(|r| matrix[r * cols..(r + 1) * cols].to_vec())
        .collect();
    let mut rank = 0;
    for col in 0..cols {
        let mut pivot = None;
        for (row, m_row) in m.iter().enumerate().take(rows).skip(rank) {
            if m_row[col] == 1 {
                pivot = Some(row);
                break;
            }
        }
        let pivot = match pivot {
            Some(p) => p,
            None => continue,
        };
        m.swap(rank, pivot);
        for row in 0..rows {
            if row != rank && m[row][col] == 1 {
                let rank_row = m[rank].clone();
                for (m_c, r_c) in m[row].iter_mut().zip(rank_row.iter()) {
                    *m_c ^= r_c;
                }
            }
        }
        rank += 1;
    }
    rank
}

/// Test 24: Binary matrix rank -- GF(2) Gaussian elimination on 32x32 binary matrices.
pub fn binary_matrix_rank(data: &[u8]) -> TestResult {
    let name = "Binary Matrix Rank";
    let bits = to_bits(data);
    let n = bits.len();
    let m_size = 32;
    let q_size = 32;
    let bits_per_matrix = m_size * q_size;
    let num_matrices = n / bits_per_matrix;
    if num_matrices < 38 {
        return insufficient(name, 38 * bits_per_matrix, n);
    }

    let mut full_rank = 0u64;
    let mut rank_m1 = 0u64;
    let min_dim = m_size.min(q_size);
    for i in 0..num_matrices {
        let start = i * bits_per_matrix;
        let matrix = &bits[start..start + bits_per_matrix];
        let rank = gf2_rank(matrix, m_size, q_size);
        if rank == min_dim {
            full_rank += 1;
        } else if rank == min_dim - 1 {
            rank_m1 += 1;
        }
    }
    let rest = num_matrices as u64 - full_rank - rank_m1;
    let n_f = num_matrices as f64;

    let p_full = 0.2888;
    let p_m1 = 0.5776;
    let p_rest = 0.1336;
    let chi2 = (full_rank as f64 - n_f * p_full).powi(2) / (n_f * p_full)
        + (rank_m1 as f64 - n_f * p_m1).powi(2) / (n_f * p_m1)
        + (rest as f64 - n_f * p_rest).powi(2) / (n_f * p_rest);

    let dist = ChiSquared::new(2.0).unwrap();
    let p = dist.sf(chi2);
    TestResult {
        name: name.to_string(),
        passed: TestResult::pass_from_p(Some(p), 0.01),
        p_value: Some(p),
        statistic: chi2,
        details: format!("N={num_matrices}, full={full_rank}, full-1={rank_m1}"),
        grade: TestResult::grade_from_p(Some(p)),
    }
}

/// Berlekamp-Massey algorithm for binary sequences. Returns the LFSR complexity.
fn berlekamp_massey(seq: &[u8]) -> usize {
    let n = seq.len();
    let mut c = vec![0u8; n];
    let mut b = vec![0u8; n];
    c[0] = 1;
    b[0] = 1;
    let mut l: usize = 0;
    let mut m: isize = -1;

    for ni in 0..n {
        let mut d: u8 = seq[ni];
        for i in 1..=l {
            d ^= c[i] & seq[ni - i];
        }
        if d == 1 {
            let t = c.clone();
            let shift = (ni as isize - m) as usize;
            for i in shift..n {
                c[i] ^= b[i - shift];
            }
            if l <= ni / 2 {
                l = ni + 1 - l;
                m = ni as isize;
                b = t;
            }
        }
    }
    l
}

/// Test 25: Linear complexity -- Berlekamp-Massey LFSR complexity on 200-bit blocks.
pub fn linear_complexity(data: &[u8]) -> TestResult {
    let name = "Linear Complexity";
    let block_size = 200usize;
    let bits = to_bits(data);
    let n = bits.len();
    let num_blocks = n / block_size;
    if num_blocks < 6 {
        return insufficient(name, 6 * block_size, n);
    }

    let mut complexities = Vec::with_capacity(num_blocks);
    for i in 0..num_blocks {
        let start = i * block_size;
        let block = &bits[start..start + block_size];
        complexities.push(berlekamp_massey(block));
    }

    let m = block_size as f64;
    let sign = if block_size.is_multiple_of(2) {
        1.0
    } else {
        -1.0
    };
    let mu = m / 2.0 + (9.0 + sign) / 36.0 - (m / 3.0 + 2.0 / 9.0) / 2.0_f64.powf(m);

    let t_vals: Vec<f64> = complexities
        .iter()
        .map(|&c| sign * (c as f64 - mu) + 2.0 / 9.0)
        .collect();

    let mut observed = [0u64; 7];
    for &t in &t_vals {
        let bin = if t <= -2.5 {
            0
        } else if t <= -1.5 {
            1
        } else if t <= -0.5 {
            2
        } else if t <= 0.5 {
            3
        } else if t <= 1.5 {
            4
        } else if t <= 2.5 {
            5
        } else {
            6
        };
        observed[bin] += 1;
    }

    let mut probs = [0.010882, 0.03534, 0.08884, 0.5, 0.08884, 0.03534, 0.010882];
    let sum_rest: f64 = probs[..6].iter().sum();
    probs[6] = 1.0 - sum_rest;

    let mut chi2 = 0.0;
    let n_f = num_blocks as f64;
    for i in 0..7 {
        let expected = probs[i] * n_f;
        if expected > 0.0 {
            let diff = observed[i] as f64 - expected;
            chi2 += diff * diff / expected;
        }
    }

    let dist = ChiSquared::new(6.0).unwrap();
    let p = dist.sf(chi2);
    let mean_c: f64 = complexities.iter().map(|&c| c as f64).sum::<f64>() / num_blocks as f64;
    TestResult {
        name: name.to_string(),
        passed: TestResult::pass_from_p(Some(p), 0.01),
        p_value: Some(p),
        statistic: chi2,
        details: format!("N={num_blocks}, mean_complexity={mean_c:.1}"),
        grade: TestResult::grade_from_p(Some(p)),
    }
}

/// Test 26: Cumulative sums (CUSUM) -- detect drift/bias.
pub fn cusum_test(data: &[u8]) -> TestResult {
    let name = "Cumulative Sums";
    let bits = to_bits(data);
    let n = bits.len();
    if n < 100 {
        return insufficient(name, 100, n);
    }

    let mut cumsum = Vec::with_capacity(n);
    let mut s: i64 = 0;
    for &bit in &bits {
        s += if bit == 1 { 1 } else { -1 };
        cumsum.push(s);
    }
    let z = cumsum.iter().map(|&x| x.unsigned_abs()).max().unwrap() as f64;
    if z < 1e-10 {
        return TestResult {
            name: name.to_string(),
            passed: true,
            p_value: Some(1.0),
            statistic: 0.0,
            details: format!("max|S|=0, n={n}"),
            grade: 'A',
        };
    }

    let nf = n as f64;
    let sqrt_n = nf.sqrt();
    let norm = Normal::standard();
    let k_start = ((-nf / z + 1.0) / 4.0).floor() as i64;
    let k_end = ((nf / z - 1.0) / 4.0).ceil() as i64;
    let mut s_val = 0.0;
    for k in k_start..=k_end {
        let kf = k as f64;
        s_val += norm.cdf((4.0 * kf + 1.0) * z / sqrt_n) - norm.cdf((4.0 * kf - 1.0) * z / sqrt_n);
    }
    let p = (1.0 - s_val).clamp(0.0, 1.0);
    TestResult {
        name: name.to_string(),
        passed: TestResult::pass_from_p(Some(p), 0.01),
        p_value: Some(p),
        statistic: z,
        details: format!("max|S|={z:.1}, n={n}"),
        grade: TestResult::grade_from_p(Some(p)),
    }
}

/// Test 27: Random excursions -- cycles in cumulative sum random walk.
pub fn random_excursions(data: &[u8]) -> TestResult {
    let name = "Random Excursions";
    let bits = to_bits(data);
    let n = bits.len();
    if n < 1000 {
        return insufficient(name, 1000, n);
    }

    // Build cumulative sum with leading and trailing zeros
    let mut cumsum = Vec::with_capacity(n + 2);
    cumsum.push(0i64);
    let mut s: i64 = 0;
    for &bit in &bits {
        s += if bit == 1 { 1 } else { -1 };
        cumsum.push(s);
    }
    cumsum.push(0);

    let zeros: Vec<usize> = cumsum
        .iter()
        .enumerate()
        .filter_map(|(i, &v)| if v == 0 { Some(i) } else { None })
        .collect();

    let j = if !zeros.is_empty() {
        zeros.len() - 1
    } else {
        0
    };

    if j < 500 {
        return TestResult {
            name: name.to_string(),
            passed: true,
            p_value: None,
            statistic: j as f64,
            details: format!("Only {j} cycles (need 500 for reliable test)"),
            grade: 'B',
        };
    }

    let expected_cycles = (n as f64) / (2.0 * PI * n as f64).sqrt();
    let ratio = j as f64 / expected_cycles.max(1.0);
    let passed = ratio > 0.5 && ratio < 2.0;
    let grade = if ratio > 0.8 && ratio < 1.2 {
        'A'
    } else if ratio > 0.6 && ratio < 1.5 {
        'B'
    } else if passed {
        'C'
    } else {
        'F'
    };
    TestResult {
        name: name.to_string(),
        passed,
        p_value: None,
        statistic: j as f64,
        details: format!("cycles={j}, expected~={expected_cycles:.0}"),
        grade,
    }
}

/// Test 28: Birthday spacing -- spacing between repeated values, Poisson test.
pub fn birthday_spacing(data: &[u8]) -> TestResult {
    let name = "Birthday Spacing";
    let n = data.len();
    if n < 100 {
        return insufficient(name, 100, n);
    }

    let values: Vec<u64> = if n >= 200 {
        let half = n / 2;
        (0..half)
            .map(|i| data[i * 2] as u64 * 256 + data[i * 2 + 1] as u64)
            .collect()
    } else {
        data.iter().map(|&b| b as u64).collect()
    };

    let m = values.len();
    let mut sorted = values.clone();
    sorted.sort();

    let mut spacings: Vec<u64> = Vec::with_capacity(m.saturating_sub(1));
    for i in 1..m {
        spacings.push(sorted[i] - sorted[i - 1]);
    }
    spacings.sort();

    let mut dups = 0u64;
    for i in 1..spacings.len() {
        if spacings[i] == spacings[i - 1] {
            dups += 1;
        }
    }

    let d = sorted.last().copied().unwrap_or(1).max(1) as f64;
    let mf = m as f64;
    let lambda = (mf * mf * mf / (4.0 * d)).max(0.01);

    let p = if lambda > 0.0 {
        let poisson = Poisson::new(lambda).unwrap();
        let p_upper = if dups > 0 { poisson.sf(dups - 1) } else { 1.0 };
        let p_lower = poisson.cdf(dups);
        p_upper.max(p_lower).min(1.0)
    } else {
        1.0
    };

    TestResult {
        name: name.to_string(),
        passed: TestResult::pass_from_p(Some(p), 0.01),
        p_value: Some(p),
        statistic: dups as f64,
        details: format!("duplicates={dups}, lambda={lambda:.2}, m={m}"),
        grade: TestResult::grade_from_p(Some(p)),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. PRACTICAL TESTS
// ═══════════════════════════════════════════════════════════════════════════════

/// Test 29: Bit avalanche -- adjacent bytes should differ by ~4 bits (50%).
pub fn bit_avalanche(data: &[u8]) -> TestResult {
    let name = "Bit Avalanche";
    let n = data.len();
    if n < 100 {
        return insufficient(name, 100, n);
    }

    let mut total_diffs = 0u64;
    let pairs = n - 1;
    for i in 0..pairs {
        total_diffs += (data[i] ^ data[i + 1]).count_ones() as u64;
    }
    let mean_diff = total_diffs as f64 / pairs as f64;
    let expected = 4.0;
    let std = 2.0_f64.sqrt(); // binomial std for n=8, p=0.5
    let z = (mean_diff - expected).abs() / (std / (pairs as f64).sqrt());
    let norm = Normal::standard();
    let p = 2.0 * (1.0 - norm.cdf(z));
    TestResult {
        name: name.to_string(),
        passed: TestResult::pass_from_p(Some(p), 0.01),
        p_value: Some(p),
        statistic: mean_diff,
        details: format!("mean_diff={mean_diff:.3}/8 bits, expected=4.0"),
        grade: TestResult::grade_from_p(Some(p)),
    }
}

/// Test 30: Monte Carlo pi -- estimate pi using (x,y) pairs in unit circle.
pub fn monte_carlo_pi(data: &[u8]) -> TestResult {
    let name = "Monte Carlo Pi";
    let n = data.len();
    if n < 200 {
        return insufficient(name, 200, n);
    }
    let pairs = n / 2;
    let mut inside = 0u64;
    for i in 0..pairs {
        let x = data[i] as f64 / 255.0;
        let y = data[pairs + i] as f64 / 255.0;
        if x * x + y * y <= 1.0 {
            inside += 1;
        }
    }
    let pi_est = 4.0 * inside as f64 / pairs as f64;
    let error = (pi_est - PI).abs() / PI;
    let grade = if error < 0.01 {
        'A'
    } else if error < 0.03 {
        'B'
    } else if error < 0.1 {
        'C'
    } else if error < 0.2 {
        'D'
    } else {
        'F'
    };
    TestResult {
        name: name.to_string(),
        passed: error < 0.05,
        p_value: None,
        statistic: pi_est,
        details: format!("pi~={pi_est:.6}, error={:.4}%", error * 100.0),
        grade,
    }
}

/// Test 31: Mean and variance -- mean (~127.5) and variance (~5461.25) of uniform bytes.
pub fn mean_variance(data: &[u8]) -> TestResult {
    let name = "Mean & Variance";
    let n = data.len();
    if n < 50 {
        return insufficient(name, 50, n);
    }
    let arr: Vec<f64> = data.iter().map(|&b| b as f64).collect();
    let nf = n as f64;
    let mean: f64 = arr.iter().sum::<f64>() / nf;
    let var: f64 = arr.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / nf;

    let expected_mean = 127.5;
    let expected_var = (256.0 * 256.0 - 1.0) / 12.0; // 5461.25

    let z_mean = (mean - expected_mean).abs() / (expected_var / nf).sqrt();
    let norm = Normal::standard();
    let p_mean = 2.0 * (1.0 - norm.cdf(z_mean));

    let chi2_var = (nf - 1.0) * var / expected_var;
    let chi_dist = ChiSquared::new(nf - 1.0).unwrap();
    let p_var = 2.0 * chi_dist.cdf(chi2_var).min(chi_dist.sf(chi2_var));

    let p = p_mean.min(p_var);
    TestResult {
        name: name.to_string(),
        passed: TestResult::pass_from_p(Some(p), 0.01),
        p_value: Some(p),
        statistic: z_mean,
        details: format!("mean={mean:.2} (exp 127.5), var={var:.1} (exp {expected_var:.1})"),
        grade: TestResult::grade_from_p(Some(p)),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Test battery
// ═══════════════════════════════════════════════════════════════════════════════

/// Run the complete 31-test battery on a byte slice.
pub fn run_all_tests(data: &[u8]) -> Vec<TestResult> {
    let tests: Vec<fn(&[u8]) -> TestResult> = vec![
        // Frequency (3)
        monobit_frequency,
        block_frequency,
        byte_frequency,
        // Runs (2)
        runs_test,
        longest_run_of_ones,
        // Serial (2)
        serial_test,
        approximate_entropy,
        // Spectral (2)
        dft_spectral,
        spectral_flatness,
        // Entropy (5)
        shannon_entropy,
        min_entropy,
        permutation_entropy,
        compression_ratio,
        kolmogorov_complexity,
        // Correlation (4)
        autocorrelation,
        serial_correlation,
        lag_n_correlation,
        cross_correlation,
        // Distribution (2)
        ks_test,
        anderson_darling,
        // Pattern (3)
        overlapping_template,
        non_overlapping_template,
        maurers_universal,
        // Advanced (5)
        binary_matrix_rank,
        linear_complexity,
        cusum_test,
        random_excursions,
        birthday_spacing,
        // Practical (3)
        bit_avalanche,
        monte_carlo_pi,
        mean_variance,
    ];

    tests
        .iter()
        .map(|test_fn| {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| test_fn(data))) {
                Ok(result) => result,
                Err(_) => TestResult {
                    name: "Unknown".to_string(),
                    passed: false,
                    p_value: None,
                    statistic: 0.0,
                    details: "Test panicked".to_string(),
                    grade: 'F',
                },
            }
        })
        .collect()
}

/// Calculate overall quality score (0-100) from test results.
///
/// Each grade maps to a score: A=100, B=75, C=50, D=25, F=0.
/// Returns the average across all tests.
pub fn calculate_quality_score(results: &[TestResult]) -> f64 {
    if results.is_empty() {
        return 0.0;
    }
    let total: f64 = results
        .iter()
        .map(|r| match r.grade {
            'A' => 100.0,
            'B' => 75.0,
            'C' => 50.0,
            'D' => 25.0,
            _ => 0.0,
        })
        .sum();
    total / results.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generate pseudo-random data for testing (simple LCG).
    fn pseudo_random(n: usize) -> Vec<u8> {
        let mut data = Vec::with_capacity(n);
        let mut state: u64 = 0xDEAD_BEEF_CAFE_BABE;
        for _ in 0..n {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            data.push((state >> 33) as u8);
        }
        data
    }

    #[test]
    fn test_to_bits() {
        let data = [0b10110001u8];
        let bits = to_bits(&data);
        assert_eq!(bits, vec![1, 0, 1, 1, 0, 0, 0, 1]);
    }

    #[test]
    fn test_grade_from_p() {
        assert_eq!(TestResult::grade_from_p(Some(0.5)), 'A');
        assert_eq!(TestResult::grade_from_p(Some(0.05)), 'B');
        assert_eq!(TestResult::grade_from_p(Some(0.005)), 'C');
        assert_eq!(TestResult::grade_from_p(Some(0.0005)), 'D');
        assert_eq!(TestResult::grade_from_p(Some(0.00000001)), 'F');
        assert_eq!(TestResult::grade_from_p(None), 'F');
    }

    #[test]
    fn test_pass_from_p() {
        assert!(TestResult::pass_from_p(Some(0.05), 0.01));
        assert!(!TestResult::pass_from_p(Some(0.005), 0.01));
        assert!(!TestResult::pass_from_p(None, 0.01));
    }

    #[test]
    fn test_insufficient_data() {
        let data = [0u8; 5];
        let result = monobit_frequency(&data);
        assert!(!result.passed);
        assert!(result.details.contains("Insufficient"));
    }

    #[test]
    fn test_constant_data_fails() {
        let data = vec![0u8; 1000];
        let results = run_all_tests(&data);
        let passed_count = results.iter().filter(|r| r.passed).count();
        assert!(passed_count < results.len() / 2);
    }

    #[test]
    fn test_pseudo_random_passes() {
        let data = pseudo_random(10000);
        let results = run_all_tests(&data);
        let passed_count = results.iter().filter(|r| r.passed).count();
        assert!(
            passed_count > results.len() / 2,
            "Only {passed_count}/{} tests passed",
            results.len()
        );
    }

    #[test]
    fn test_quality_score() {
        let results = vec![
            TestResult {
                name: "A".into(),
                passed: true,
                p_value: Some(0.5),
                statistic: 0.0,
                details: String::new(),
                grade: 'A',
            },
            TestResult {
                name: "F".into(),
                passed: false,
                p_value: Some(0.0),
                statistic: 0.0,
                details: String::new(),
                grade: 'F',
            },
        ];
        let score = calculate_quality_score(&results);
        assert!((score - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_all_31_tests_present() {
        let data = pseudo_random(10000);
        let results = run_all_tests(&data);
        assert_eq!(results.len(), 31);
    }

    #[test]
    fn test_monobit_basic() {
        let data = pseudo_random(1000);
        let result = monobit_frequency(&data);
        assert!(result.p_value.is_some());
    }

    #[test]
    fn test_shannon_entropy_random() {
        let data = pseudo_random(10000);
        let result = shannon_entropy(&data);
        assert!(
            result.statistic > 7.0,
            "Shannon entropy too low: {}",
            result.statistic
        );
    }

    #[test]
    fn test_compression_ratio_random() {
        let data = pseudo_random(10000);
        let result = compression_ratio(&data);
        assert!(
            result.statistic > 0.9,
            "Compression ratio too low: {}",
            result.statistic
        );
    }

    #[test]
    fn test_calculate_quality_score_empty() {
        assert_eq!(calculate_quality_score(&[]), 0.0);
    }
}

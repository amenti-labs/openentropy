//! Shared math and statistics utilities.
//!
//! Centralised implementations of mathematical functions used across multiple
//! analysis modules (analysis, comparison, chaos, and future additions).
//! All functions are `pub(crate)` — exposed to sibling modules but not to
//! downstream crate consumers.

use std::f64::consts::PI;

// ---------------------------------------------------------------------------
// Log-gamma (Lanczos approximation)
// ---------------------------------------------------------------------------

/// Log-gamma function via the Lanczos approximation (g = 7, n = 9).
///
/// Returns ln(Γ(x)). Used by chi-squared p-value / survival functions and
/// BiEntropy binomial coefficients.
pub(crate) fn ln_gamma(x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    let g = 7.0;
    let c = [
        0.999_999_999_999_809_9,
        676.5203681218851,
        -1259.1392167224028,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507343278686905,
        -0.13857109526572012,
        9.984_369_578_019_572e-6,
        1.5056327351493116e-7,
    ];

    let x = x - 1.0;
    let mut sum = c[0];
    for (i, &coeff) in c[1..].iter().enumerate() {
        sum += coeff / (x + i as f64 + 1.0);
    }
    let t = x + g + 0.5;
    0.5 * (2.0 * PI).ln() + (t.ln() * (x + 0.5)) - t + sum.ln()
}

// ---------------------------------------------------------------------------
// Chi-squared distribution functions
// ---------------------------------------------------------------------------

/// Approximate chi-squared p-value using the regularized incomplete gamma
/// function (series expansion).
///
/// Returns P(X > chi2 | df) — the upper-tail probability.
/// Suitable when chi2 is not extremely large relative to df.
pub(crate) fn chi_squared_p_value(chi2: f64, df: usize) -> f64 {
    let a = df as f64 / 2.0;
    let x = chi2 / 2.0;

    if x < 0.0 {
        return 1.0;
    }

    // Series expansion for the regularized lower incomplete gamma.
    let mut sum = 0.0;
    let mut term = 1.0 / a;
    sum += term;
    for n in 1..200 {
        term *= x / (a + n as f64);
        sum += term;
        if term.abs() < 1e-12 {
            break;
        }
    }
    let lower_gamma = (-x + a * x.ln() - ln_gamma(a)).exp() * sum;
    (1.0 - lower_gamma).clamp(0.0, 1.0)
}

/// Upper-tail probability (survival function) for chi-squared distribution.
///
/// P(X > chi2 | df) using series expansion for small x and Lentz continued
/// fraction for large x. More numerically stable than [`chi_squared_p_value`]
/// for extreme values.
pub(crate) fn chi_squared_survival(chi2: f64, df: usize) -> f64 {
    if df == 0 || chi2 < 0.0 {
        return 1.0;
    }
    if chi2 == 0.0 {
        return 1.0;
    }
    let a = df as f64 / 2.0;
    let x = chi2 / 2.0;

    // For extremely large x relative to a, the survival is effectively 0.
    let log_prefix = -x + a * x.ln() - ln_gamma(a);
    if log_prefix < -700.0 {
        return 0.0;
    }

    if x < a + 1.0 {
        // Series expansion for the regularized lower incomplete gamma P(a,x).
        // Survival = 1 - P(a,x).
        let mut sum = 0.0;
        let mut term = 1.0 / a;
        sum += term;
        for n in 1..300 {
            term *= x / (a + n as f64);
            sum += term;
            if term.abs() < 1e-14 {
                break;
            }
        }
        let log_p = -x + a * x.ln() - ln_gamma(a) + sum.ln();
        (1.0 - log_p.exp()).clamp(0.0, 1.0)
    } else {
        // Continued fraction (Lentz) for the upper incomplete gamma Q(a,x).
        let b0 = x - a + 1.0;
        let mut f = if b0.abs() < 1e-30 { 1e-30 } else { b0 };
        let mut c = f;
        let mut d = 0.0_f64;

        for n in 1..300 {
            let nf = n as f64;
            let a_n = nf * (a - nf);
            let b_n = b0 + 2.0 * nf;

            d = b_n + a_n * d;
            if d.abs() < 1e-30 {
                d = 1e-30;
            }
            d = 1.0 / d;

            c = b_n + a_n / c;
            if c.abs() < 1e-30 {
                c = 1e-30;
            }

            let delta = c * d;
            f *= delta;
            if (delta - 1.0).abs() < 1e-14 {
                break;
            }
        }

        // Q(a,x) = exp(-x) * x^a / Gamma(a) / CF
        let log_q = -x + a * x.ln() - ln_gamma(a) - f.ln();
        log_q.exp().clamp(0.0, 1.0)
    }
}

// ---------------------------------------------------------------------------
// Complementary error function
// ---------------------------------------------------------------------------

/// Complementary error function, erfc(x) = 1 - erf(x).
///
/// Uses Abramowitz & Stegun approximation 7.1.26 (maximum error < 1.5e-7).
pub(crate) fn erfc(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.3275911 * x.abs());
    let poly = t
        * (0.254829592
            + t * (-0.284496736 + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))));
    let result = poly * (-x * x).exp();
    if x >= 0.0 { result } else { 2.0 - result }
}

// ---------------------------------------------------------------------------
// Linear regression
// ---------------------------------------------------------------------------

/// Ordinary least-squares linear regression.
///
/// Returns `(slope, intercept, r_squared)`. If input is degenerate (fewer
/// than 2 points or zero variance in x), returns `(NaN, NaN|mean_y, 0.0)`.
pub(crate) fn linear_regression(x: &[f64], y: &[f64]) -> (f64, f64, f64) {
    if x.len() != y.len() || x.len() < 2 {
        return (f64::NAN, f64::NAN, 0.0);
    }

    let n = x.len() as f64;
    let mean_x = x.iter().sum::<f64>() / n;
    let mean_y = y.iter().sum::<f64>() / n;

    let mut numerator = 0.0;
    let mut denominator = 0.0;
    for (&xi, &yi) in x.iter().zip(y.iter()) {
        let dx = xi - mean_x;
        numerator += dx * (yi - mean_y);
        denominator += dx * dx;
    }

    if denominator.abs() < 1e-12 {
        return (f64::NAN, mean_y, 0.0);
    }

    let slope = numerator / denominator;
    let intercept = mean_y - slope * mean_x;

    let mut ss_res = 0.0;
    let mut ss_tot = 0.0;
    for (&xi, &yi) in x.iter().zip(y.iter()) {
        let y_hat = slope * xi + intercept;
        ss_res += (yi - y_hat).powi(2);
        ss_tot += (yi - mean_y).powi(2);
    }

    let r_squared = if ss_tot < 1e-12 {
        0.0
    } else {
        1.0 - (ss_res / ss_tot)
    };

    (slope, intercept, r_squared)
}

// ---------------------------------------------------------------------------
// Euclidean distance
// ---------------------------------------------------------------------------

/// Euclidean distance between two equal-length vectors.
///
/// If lengths differ, uses the shorter length. Returns 0.0 for empty input.
pub(crate) fn euclidean_distance(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }

    let mut sum = 0.0;
    for i in 0..n {
        let d = a[i] - b[i];
        sum += d * d;
    }
    sum.sqrt()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ln_gamma_basic() {
        // Γ(1) = 0! = 1 → ln(1) = 0
        assert!((ln_gamma(1.0)).abs() < 1e-10);
        // Γ(2) = 1! = 1 → ln(1) = 0
        assert!((ln_gamma(2.0)).abs() < 1e-10);
        // Γ(5) = 4! = 24 → ln(24) ≈ 3.178
        assert!((ln_gamma(5.0) - (24.0_f64).ln()).abs() < 1e-6);
    }

    #[test]
    fn ln_gamma_non_positive() {
        assert_eq!(ln_gamma(0.0), 0.0);
        assert_eq!(ln_gamma(-1.0), 0.0);
    }

    #[test]
    fn chi_squared_p_value_basic() {
        // Very small chi2 → p-value near 1
        let p = chi_squared_p_value(0.1, 2);
        assert!(p > 0.9);
        // Very large chi2 → p-value near 0
        let p = chi_squared_p_value(100.0, 2);
        assert!(p < 0.01);
    }

    #[test]
    fn chi_squared_survival_basic() {
        let p = chi_squared_survival(0.0, 10);
        assert!((p - 1.0).abs() < 1e-10);
        let p = chi_squared_survival(1000.0, 10);
        assert!(p < 1e-10);
    }

    #[test]
    fn chi_squared_survival_midrange() {
        // Chi2 = 18.31, df = 10 → p ≈ 0.05
        let p = chi_squared_survival(18.31, 10);
        assert!((p - 0.05).abs() < 0.01);
        // Chi2 = 23.21, df = 10 → p ≈ 0.01
        let p = chi_squared_survival(23.21, 10);
        assert!((p - 0.01).abs() < 0.005);
    }

    #[test]
    fn erfc_basic() {
        // erfc(0) = 1
        assert!((erfc(0.0) - 1.0).abs() < 1e-6);
        // erfc(large) ≈ 0
        assert!(erfc(5.0) < 1e-6);
        // erfc(1.0) ≈ 0.1573
        assert!((erfc(1.0) - 0.1573).abs() < 0.001);
    }

    #[test]
    fn linear_regression_basic() {
        // Perfect line: y = 2x + 1
        let x = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let y = vec![3.0, 5.0, 7.0, 9.0, 11.0];
        let (slope, intercept, r_sq) = linear_regression(&x, &y);
        assert!((slope - 2.0).abs() < 1e-10);
        assert!((intercept - 1.0).abs() < 1e-10);
        assert!((r_sq - 1.0).abs() < 1e-10);
    }

    #[test]
    fn linear_regression_degenerate() {
        let (slope, _, _) = linear_regression(&[1.0], &[2.0]);
        assert!(slope.is_nan());
    }

    #[test]
    fn euclidean_distance_basic() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![3.0, 4.0, 0.0];
        assert!((euclidean_distance(&a, &b) - 5.0).abs() < 1e-10);
    }

    #[test]
    fn euclidean_distance_empty() {
        assert_eq!(euclidean_distance(&[], &[]), 0.0);
    }
}

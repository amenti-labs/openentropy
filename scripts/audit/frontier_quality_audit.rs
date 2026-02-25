//! Frontier source quality audit — H∞ screening.
//!
//! Applies the Feb-14 quality criteria to all untested frontier sources:
//!   CUT    H∞ < 0.5 at N samples
//!   DEMOTE H∞ < 1.5 or autocorr(lag-1) > 0.5
//!   KEEP   otherwise
//!
//! Moved from crates/openentropy-core/examples/ to scripts/audit/.
//! To run: temporarily move back to examples/ or add as a [[bin]] target.

use openentropy_core::sources::frontier::*;
use openentropy_core::EntropySource;

struct Stats {
    h_inf: f64,
    h_shannon: f64,
    autocorr_1: f64,
    cv: f64,
    n: usize,
}

fn compute_stats(data: &[u8]) -> Stats {
    let n = data.len();
    if n == 0 {
        return Stats { h_inf: 0.0, h_shannon: 0.0, autocorr_1: 0.0, cv: 0.0, n: 0 };
    }

    // Frequency counts
    let mut freq = [0u32; 256];
    for &b in data { freq[b as usize] += 1; }

    // Max probability → H∞
    let max_p = *freq.iter().max().unwrap() as f64 / n as f64;
    let h_inf = -max_p.log2();

    // Shannon entropy
    let h_shannon: f64 = freq.iter()
        .filter(|&&c| c > 0)
        .map(|&c| { let p = c as f64 / n as f64; -p * p.log2() })
        .sum();

    // Autocorrelation lag-1 (on raw byte values)
    let mean: f64 = data.iter().map(|&b| b as f64).sum::<f64>() / n as f64;
    let var: f64 = data.iter().map(|&b| (b as f64 - mean).powi(2)).sum::<f64>() / n as f64;
    let autocorr_1 = if var > 0.0 && n > 1 {
        let cov: f64 = data.windows(2)
            .map(|w| (w[0] as f64 - mean) * (w[1] as f64 - mean))
            .sum::<f64>() / (n - 1) as f64;
        cov / var
    } else { 0.0 };

    // CV
    let cv = if mean > 0.0 { 100.0 * var.sqrt() / mean } else { 0.0 };

    Stats { h_inf, h_shannon, autocorr_1, cv, n }
}

fn verdict(s: &Stats, available: bool) -> &'static str {
    if !available { return "SKIP (unavailable)"; }
    if s.n < 100 { return "SKIP (no data)"; }
    if s.h_inf < 0.5 { return "CUT  ✗"; }
    if s.h_inf < 1.5 || s.autocorr_1.abs() > 0.5 { return "DEMOTE ⚠"; }
    "KEEP ✓"
}

fn audit<S: EntropySource>(src: &S, n: usize) {
    let name = src.info().name;
    let avail = src.is_available();

    if !avail {
        println!("{:<32} SKIP (unavailable)", name);
        return;
    }

    let data = src.collect(n);
    let s = compute_stats(&data);
    let v = verdict(&s, avail);

    println!(
        "{:<32} H∞={:4.2}  Hsh={:4.2}  ac1={:+5.3}  CV={:5.1}%  n={}  {}",
        name, s.h_inf, s.h_shannon, s.autocorr_1, s.cv, s.n, v
    );
}

fn main() {
    println!("=== Frontier Source Quality Audit ===");
    println!("Criteria: CUT H∞<0.5 | DEMOTE H∞<1.5 or |ac1|>0.5 | KEEP otherwise\n");
    println!(
        "{:<32} {:<8} {:<8} {:<7} {:<9} {}",
        "Source", "H∞", "Hshannon", "ac(1)", "CV", "Verdict"
    );
    println!("{}", "─".repeat(90));

    // ── Sources needing first-time quality testing ──────────────────────────
    // N=5000 for fast sources, N=500 for slow (NL inference, USB, timer coalesc)

    // rndr_trap_timing: hangs at N=5000 — mark for investigation
    // proc_info_timing: SIGSEGV at N=5000 — mark for investigation

    // ── Reference (previously validated) ──────────────────────────────────
    println!("\n── Reference baselines ──");
    audit(&DVFSRaceSource,            2_000);
    audit(&APRRJitTimingSource,       2_000);
    // DualClockDomain and SITVA require JIT/threads — skip from automated audit
}

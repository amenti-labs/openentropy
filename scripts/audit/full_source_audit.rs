//! Comprehensive source audit — deduplication and physics review.
//!
//! Moved from crates/openentropy-core/examples/ to scripts/audit/.
//! To run: temporarily move back to examples/ or add as a [[bin]] target.

use openentropy_core::{detect_available_sources, EntropySource};
use std::collections::HashMap;

fn compute_stats(data: &[u8]) -> (f64, f64, f64) {
    let n = data.len();
    if n == 0 { return (0.0, 0.0, 0.0); }
    let mut freq = [0u32; 256];
    for &b in data { freq[b as usize] += 1; }
    let max_p = *freq.iter().max().unwrap() as f64 / n as f64;
    let h_inf = -max_p.log2().max(-30.0);
    let h_sh: f64 = freq.iter().filter(|&&c| c > 0)
        .map(|&c| { let p = c as f64 / n as f64; -p * p.log2() }).sum();
    let mean = data.iter().map(|&b| b as f64).sum::<f64>() / n as f64;
    let var = data.iter().map(|&b| (b as f64 - mean).powi(2)).sum::<f64>() / n as f64;
    let cv = if mean > 0.0 { 100.0 * var.sqrt() / mean } else { 0.0 };
    (h_inf, h_sh, cv)
}

fn pearson(a: &[u8], b: &[u8]) -> f64 {
    let n = a.len().min(b.len());
    if n < 2 { return 0.0; }
    let ma = a[..n].iter().map(|&x| x as f64).sum::<f64>() / n as f64;
    let mb = b[..n].iter().map(|&x| x as f64).sum::<f64>() / n as f64;
    let cov: f64 = a[..n].iter().zip(b[..n].iter())
        .map(|(&x, &y)| (x as f64 - ma) * (y as f64 - mb)).sum::<f64>();
    let sa = a[..n].iter().map(|&x| (x as f64 - ma).powi(2)).sum::<f64>().sqrt();
    let sb = b[..n].iter().map(|&x| (x as f64 - mb).powi(2)).sum::<f64>().sqrt();
    if sa * sb == 0.0 { return 0.0; }
    (cov / (sa * sb)).abs()
}

fn main() {
    let sources = detect_available_sources();
    let n_sources = sources.len();
    println!("=== OPENENTROPY FULL SOURCE AUDIT ===");
    println!("{n_sources} sources detected\n");

    // ── 1. Full source listing ───────────────────────────────────────────────
    println!("{:<32} {:<14} {:<8} {:<8} {}", 
             "Name", "Category", "H∞", "Hshannon", "Composite");
    println!("{}", "─".repeat(80));

    let mut samples: HashMap<String, Vec<u8>> = HashMap::new();
    let mut names_by_category: HashMap<String, Vec<String>> = HashMap::new();

    // Known to SIGSEGV or hang in isolated test — skip collect(), mark for manual review
    const SKIP_COLLECT: &[&str] = &[
        "nl_inference_timing",  // SIGSEGV (ObjC runtime in test harness)
        "proc_info_timing",     // SIGSEGV
    ];

    for src in &sources {
        let info = src.info();
        if !src.is_available() {
            println!("{:<32} {:<14} UNAVAILABLE", info.name, format!("{:?}", info.category));
            continue;
        }

        if SKIP_COLLECT.iter().any(|&s| s == info.name) {
            let cat = format!("{:?}", info.category);
            println!("{:<32} {:<14} {:<8} {:<8} {} ← SKIP (known crash/hang)",
                     info.name, cat, "?", "?", if info.composite { "YES" } else { "no" });
            names_by_category.entry(cat).or_default().push(info.name.to_string());
            continue;
        }
        
        // Collect small sample — enough for stats, fast enough to not timeout
        let n = match info.name {
            "timer_coalescing" | "usb_enumeration"
            | "nvme_latency" | "fsync_journal" | "keychain_timing"
            | "kqueue_events" => 200,
            "sitva" | "dual_clock_domain" => 500,
            _ => 1000,
        };

        let data = src.collect(n);
        let (h_inf, h_sh, _cv) = compute_stats(&data);
        
        let cat = format!("{:?}", info.category);
        names_by_category.entry(cat.clone()).or_default().push(info.name.to_string());
        
        if !data.is_empty() {
            samples.insert(info.name.to_string(), data);
        }

        let flag = if h_inf < 0.5 { " ← LOW H∞" } 
                   else if h_inf < 1.5 { " ← WEAK" } 
                   else { "" };

        println!("{:<32} {:<14} {:<8.2} {:<8.2} {}{}",
                 info.name, cat, h_inf, h_sh,
                 if info.composite { "YES" } else { "no" },
                 flag);
    }

    // ── 2. Sources by category ───────────────────────────────────────────────
    println!("\n=== BY CATEGORY ===");
    let mut cats: Vec<_> = names_by_category.iter().collect();
    cats.sort_by_key(|(k, _)| k.as_str());
    for (cat, names) in &cats {
        println!("{cat:<18} ({}) {}", names.len(), names.join(", "));
    }

    // ── 3. Duplicate detection: name similarity ──────────────────────────────
    println!("\n=== POTENTIAL DUPLICATES (shared keyword in name) ===");
    let all_names: Vec<&str> = sources.iter().map(|s| s.info().name).collect();
    let keywords = ["aes", "timing", "smc", "usb", "pll", "mach", "clock", "cache", "crypto"];
    for kw in &keywords {
        let matches: Vec<_> = all_names.iter().filter(|n| n.contains(kw)).collect();
        if matches.len() > 1 {
            println!("  '{kw}': {}", matches.iter().map(|s| **s).collect::<Vec<_>>().join(", "));
        }
    }

    // ── 4. Cross-correlation screen (>0.3 = likely redundant) ────────────────
    println!("\n=== CROSS-CORRELATION > 0.30 (N=500) ===");
    let sample_names: Vec<String> = samples.keys().cloned().collect();
    let mut found_corr = false;
    for i in 0..sample_names.len() {
        for j in (i+1)..sample_names.len() {
            let na = &sample_names[i];
            let nb = &sample_names[j];
            let a = &samples[na];
            let b = &samples[nb];
            let r = pearson(a, b);
            if r > 0.30 {
                println!("  r={:.3}  {} ↔ {}", r, na, nb);
                found_corr = true;
            }
        }
    }
    if !found_corr { println!("  None found — good."); }

    println!("\n=== AUDIT COMPLETE ===");
    println!("Total: {n_sources} sources");
}

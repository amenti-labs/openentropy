use std::time::Instant;

pub fn run(
    samples: usize,
    source_name: Option<&str>,
    output_path: Option<&str>,
    conditioning: &str,
    include_telemetry: bool,
) {
    let telemetry = super::telemetry::TelemetryCapture::start(include_telemetry);
    let mode = super::parse_conditioning(conditioning);
    // Use make_pool which defaults to fast sources
    let pool = super::make_pool(source_name);
    let infos = pool.source_infos();

    // Get sources from platform detection, filtered same way
    let all_sources = openentropy_core::platform::detect_available_sources();
    let sources: Vec<_> = if let Some(name) = source_name {
        all_sources
            .into_iter()
            .filter(|s| s.name().to_lowercase().contains(&name.to_lowercase()))
            .collect()
    } else {
        let fast_names: Vec<_> = infos.iter().map(|i| i.name.clone()).collect();
        all_sources
            .into_iter()
            .filter(|s| fast_names.iter().any(|n| n == s.name()))
            .collect()
    };

    if sources.is_empty() {
        if let Some(name) = source_name {
            eprintln!("Source '{name}' not found.");
        } else {
            eprintln!("No sources found.");
        }
        std::process::exit(1);
    }

    println!(
        "🔬 Running full test battery on {} source(s), {} samples each...\n",
        sources.len(),
        samples
    );

    let mut all_results = Vec::new();

    for src in &sources {
        let info = src.info();
        print!("  Collecting from {}...", info.name);

        let t0 = Instant::now();
        let raw_data = src.collect(samples);
        let data = openentropy_core::conditioning::condition(&raw_data, raw_data.len(), mode);
        print!(" {} bytes", data.len());

        if data.is_empty() {
            println!(" ✗ no data");
            continue;
        }

        let results = openentropy_tests::run_all_tests(&data);
        let elapsed = t0.elapsed().as_secs_f64();
        let score = openentropy_tests::calculate_quality_score(&results);
        let passed = results.iter().filter(|r| r.passed).count();

        println!(
            " → {:.0}/100 ({}/{} passed) [{:.1}s]",
            score,
            passed,
            results.len(),
            elapsed
        );

        all_results.push((info.name.to_string(), data, results));
    }

    if all_results.is_empty() {
        eprintln!("No sources produced data.");
        std::process::exit(1);
    }

    // Generate report
    let telemetry_report = telemetry.finish_and_print("report");
    let report = generate_report(&all_results, telemetry_report.as_ref());

    if let Some(path) = output_path {
        if let Err(e) = std::fs::write(path, &report) {
            eprintln!("Failed to write report to {path}: {e}");
        } else {
            println!("\n📄 Report saved to: {path}");
        }
    }

    // Summary table
    println!("\n{}", "=".repeat(60));
    println!(
        "{:<25} {:>6} {:>6} {:>8}",
        "Source", "Score", "Grade", "Pass"
    );
    println!("{}", "-".repeat(60));

    let mut sorted_indices: Vec<usize> = (0..all_results.len()).collect();
    sorted_indices.sort_by(|&a, &b| {
        let sa = openentropy_tests::calculate_quality_score(&all_results[a].2);
        let sb = openentropy_tests::calculate_quality_score(&all_results[b].2);
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });

    for &idx in &sorted_indices {
        let (ref name, _, ref results) = all_results[idx];
        let score = openentropy_tests::calculate_quality_score(results);
        let grade = if score >= 80.0 {
            'A'
        } else if score >= 60.0 {
            'B'
        } else if score >= 40.0 {
            'C'
        } else if score >= 20.0 {
            'D'
        } else {
            'F'
        };
        let passed = results.iter().filter(|r| r.passed).count();
        println!(
            "  {:<23} {:>5.1} {:>6} {:>4}/{}",
            name,
            score,
            grade,
            passed,
            results.len()
        );
    }
}

fn generate_report(
    results: &[(String, Vec<u8>, Vec<openentropy_tests::TestResult>)],
    telemetry: Option<&openentropy_core::TelemetryWindowReport>,
) -> String {
    let mut report = String::new();
    report.push_str("# OpenEntropy — NIST Randomness Test Report\n\n");
    report.push_str(&format!("Generated: {}\n\n", chrono_now()));
    if let Some(t) = telemetry {
        report.push_str("## Telemetry Context (`telemetry_v1`)\n\n");
        report.push_str(&format!(
            "- Elapsed: {:.2}s\n- Host: {}/{}\n- CPU count: {}\n- Metrics observed: {}\n\n",
            t.elapsed_ms as f64 / 1000.0,
            t.end.os,
            t.end.arch,
            t.end.cpu_count,
            t.end.metrics.len()
        ));
    }

    for (name, data, tests) in results {
        let score = openentropy_tests::calculate_quality_score(tests);
        let passed = tests.iter().filter(|r| r.passed).count();
        report.push_str(&format!("## {name}\n\n"));
        report.push_str(&format!(
            "- Samples: {} bytes\n- Score: {:.1}/100\n- Passed: {}/{}\n\n",
            data.len(),
            score,
            passed,
            tests.len()
        ));

        report.push_str("| Test | P | Grade | p-value | Statistic | Details |\n");
        report.push_str("|------|---|-------|---------|-----------|--------|\n");
        for t in tests {
            let ok = if t.passed { "✓" } else { "✗" };
            let pval = t
                .p_value
                .map(|p| format!("{p:.6}"))
                .unwrap_or_else(|| "—".to_string());
            report.push_str(&format!(
                "| {} | {} | {} | {} | {:.4} | {} |\n",
                t.name, ok, t.grade, pval, t.statistic, t.details
            ));
        }
        report.push_str("\n---\n\n");
    }

    report
}

fn chrono_now() -> String {
    // Simple timestamp without chrono dependency
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("Unix timestamp: {}", dur.as_secs())
}

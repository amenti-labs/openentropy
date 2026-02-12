use std::time::Instant;

pub fn run(samples: usize, source_name: Option<&str>, output_path: Option<&str>) {
    // Use make_pool which defaults to fast sources
    let filter = source_name.or(None);
    let pool = super::make_pool(filter);
    let infos = pool.source_infos();

    // Get sources from platform detection, filtered same way
    let all_sources = esoteric_core::platform::detect_available_sources();
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
        let data = src.collect(samples);
        print!(" {} bytes", data.len());

        if data.is_empty() {
            println!(" ✗ no data");
            continue;
        }

        let results = esoteric_tests::run_all_tests(&data);
        let elapsed = t0.elapsed().as_secs_f64();
        let score = esoteric_tests::calculate_quality_score(&results);
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
    let report = generate_report(&all_results);

    if let Some(path) = output_path {
        std::fs::write(path, &report).unwrap();
        println!("\n📄 Report saved to: {path}");
    }

    // Summary table
    println!("\n{}", "=".repeat(60));
    println!(
        "{:<25} {:>6} {:>6} {:>8}",
        "Source", "Score", "Grade", "Pass"
    );
    println!("{}", "-".repeat(60));

    let mut sorted = all_results.clone();
    sorted.sort_by(|a, b| {
        let sa = esoteric_tests::calculate_quality_score(&a.2);
        let sb = esoteric_tests::calculate_quality_score(&b.2);
        sb.partial_cmp(&sa).unwrap()
    });

    for (name, _, results) in &sorted {
        let score = esoteric_tests::calculate_quality_score(results);
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

fn generate_report(results: &[(String, Vec<u8>, Vec<esoteric_tests::TestResult>)]) -> String {
    let mut report = String::new();
    report.push_str("# Esoteric Entropy — NIST Randomness Test Report\n\n");
    report.push_str(&format!("Generated: {}\n\n", chrono_now()));

    for (name, data, tests) in results {
        let score = esoteric_tests::calculate_quality_score(tests);
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

pub fn run(source_filter: Option<&str>, conditioning: &str) {
    let _mode = super::parse_conditioning(conditioning);
    let pool = super::make_pool(source_filter);
    let infos = pool.source_infos();
    let count = infos.len();

    if source_filter.is_none() {
        println!("Benchmarking {count} fast sources (use --sources all for everything)...\n");
    } else {
        println!("Benchmarking {count} sources...\n");
    }

    // Collect once to warm up all sources
    pool.collect_all();

    let mut standalone_results = Vec::new();
    let mut composite_results = Vec::new();
    let health = pool.health_report();

    for (src, snap) in health.sources.iter().zip(infos.iter()) {
        let min_h = src.min_entropy.max(0.0);
        let grade = openentropy_core::grade_min_entropy(min_h);

        let entry = (
            src.name.clone(),
            grade,
            src.entropy,
            min_h,
            src.time,
            src.bytes,
        );

        if snap.composite {
            println!(
                "  {} {:<25} H={:.3}  H\u{221E}={:.3}  {:.2}s  {}B  [COMPOSITE]",
                grade, src.name, src.entropy, min_h, src.time, src.bytes
            );
            composite_results.push(entry);
        } else {
            println!(
                "  {} {:<25} H={:.3}  H\u{221E}={:.3}  {:.2}s  {}B",
                grade, src.name, src.entropy, min_h, src.time, src.bytes
            );
            standalone_results.push(entry);
        }
    }

    let mut all_results = standalone_results;
    all_results.extend(composite_results);
    all_results.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal));

    println!("\n{}", "=".repeat(68));
    println!(
        "{:<25} {:>5} {:>8} {:>8} {:>8}",
        "Source", "Grade", "Shannon", "Min-H\u{221E}", "Time"
    );
    println!("{}", "-".repeat(68));
    for (name, grade, entropy, min_entropy, time, _) in &all_results {
        println!(
            "{:<25} {:>5} {:>7.3} {:>7.3} {:>7.2}s",
            name, grade, entropy, min_entropy, time
        );
    }
    println!("\nGrade is based on min-entropy (H\u{221E}), not Shannon.");
    println!(
        "H\u{221E} is the conservative estimate \u{2014} reflects worst-case guessing probability."
    );
}

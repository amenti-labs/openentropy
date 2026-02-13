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

    let mut results = Vec::new();
    let health = pool.health_report();

    for src in &health.sources {
        let grade = if src.min_entropy >= 7.5 {
            "A"
        } else if src.min_entropy >= 6.0 {
            "B"
        } else if src.min_entropy >= 4.0 {
            "C"
        } else if src.min_entropy >= 2.0 {
            "D"
        } else {
            "F"
        };

        let min_h = src.min_entropy.max(0.0);
        println!(
            "  {} {:<25} H={:.3}  H∞={:.3}  {:.2}s  {}B",
            grade, src.name, src.entropy, min_h, src.time, src.bytes
        );
        results.push((src.name.clone(), grade, src.entropy, min_h, src.time, src.bytes));
    }

    results.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap());

    println!("\n{}", "=".repeat(68));
    println!(
        "{:<25} {:>5} {:>8} {:>8} {:>8}",
        "Source", "Grade", "Shannon", "Min-H∞", "Time"
    );
    println!("{}", "-".repeat(68));
    for (name, grade, entropy, min_entropy, time, _) in &results {
        println!(
            "{:<25} {:>5} {:>7.3} {:>7.3} {:>7.2}s",
            name, grade, entropy, min_entropy, time
        );
    }
    println!("\nGrade is based on min-entropy (H∞), not Shannon.");
    println!("H∞ is the conservative estimate — reflects worst-case guessing probability.");
}

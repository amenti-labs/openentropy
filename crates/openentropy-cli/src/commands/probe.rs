use openentropy_core::conditioning::quick_quality;
use openentropy_core::platform::detect_available_sources;
use std::time::Instant;

pub fn run(source_name: &str) {
    let sources = detect_available_sources();
    let matches: Vec<_> = sources
        .into_iter()
        .filter(|s| {
            s.name()
                .to_lowercase()
                .contains(&source_name.to_lowercase())
        })
        .collect();

    if matches.is_empty() {
        eprintln!(
            "Source '{}' not found. Run 'scan' to list sources.",
            source_name
        );
        std::process::exit(1);
    }

    let src = &matches[0];
    let info = src.info();
    println!("Probing: {}", info.name);
    println!("  {}", info.description);
    println!();

    let t0 = Instant::now();
    let data = src.collect(5000);
    let elapsed = t0.elapsed();

    if data.is_empty() {
        println!("  No data collected.");
        return;
    }

    let quality = quick_quality(&data);
    println!("  Grade:           {}", quality.grade);
    println!("  Samples:         {}", quality.samples);
    println!(
        "  Shannon entropy: {:.4} / 8.0 bits",
        quality.shannon_entropy
    );
    println!("  Compression:     {:.4}", quality.compression_ratio);
    println!("  Unique values:   {}", quality.unique_values);
    println!("  Time:            {:.3}s", elapsed.as_secs_f64());
}

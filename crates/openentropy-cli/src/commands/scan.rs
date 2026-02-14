use openentropy_core::platform::{detect_available_sources, platform_info};

pub fn run() {
    let info = platform_info();
    println!("Platform: {} {} (Rust)", info.system, info.machine);
    println!();

    let sources = detect_available_sources();

    let standalone: Vec<_> = sources.iter().filter(|s| !s.info().composite).collect();
    let composite: Vec<_> = sources.iter().filter(|s| s.info().composite).collect();

    println!("Found {} available entropy source(s):\n", sources.len());
    for src in &standalone {
        let info = src.info();
        println!("  \u{2705} {:<25} {}", info.name, info.description);
    }

    if !composite.is_empty() {
        println!("\nComposite sources (combine multiple sources above):\n");
        for src in &composite {
            let info = src.info();
            println!("  \u{1F504} {:<25} {}", info.name, info.description);
        }
    }

    if sources.is_empty() {
        println!("  (none found)");
    }
}

use esoteric_core::platform::{detect_available_sources, platform_info};

pub fn run() {
    let info = platform_info();
    println!("Platform: {} {} (Rust)", info.system, info.machine);
    println!();

    let sources = detect_available_sources();
    println!("Found {} available entropy source(s):\n", sources.len());
    for src in &sources {
        let info = src.info();
        println!("  ✅ {:<25} {}", info.name, info.description);
    }
    if sources.is_empty() {
        println!("  (none found)");
    }
}

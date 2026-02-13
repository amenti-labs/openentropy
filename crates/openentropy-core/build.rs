fn main() {
    // Link Accelerate framework on macOS for AMX coprocessor entropy source (cblas_sgemm).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-lib=framework=Accelerate");
    }
}

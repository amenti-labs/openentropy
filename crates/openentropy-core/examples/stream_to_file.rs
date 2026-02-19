//! Stream entropy to a file.
//!
//! Collects conditioned random bytes and writes them to a file.
//!
//! Run: `cargo run --example stream_to_file`

use std::fs::File;
use std::io::Write;

use openentropy_core::EntropyPool;

fn main() {
    let output_path = "entropy_output.bin";
    let total_bytes: usize = 4096;
    let chunk_size: usize = 256;

    let pool = EntropyPool::auto();
    println!(
        "Streaming {total_bytes} bytes to {output_path} ({} sources)",
        pool.source_count()
    );

    let mut file = File::create(output_path).expect("Failed to create output file");
    let mut written = 0;

    while written < total_bytes {
        let n = chunk_size.min(total_bytes - written);
        let bytes = pool.get_random_bytes(n);
        file.write_all(&bytes).expect("Failed to write");
        written += bytes.len();
        print!("\r  {written}/{total_bytes} bytes written");
    }

    println!("\nDone. Wrote {written} bytes to {output_path}");
}

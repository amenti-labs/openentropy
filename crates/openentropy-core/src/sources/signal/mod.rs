mod compression_timing;
mod hash_timing;
mod spotlight_timing;

pub use compression_timing::CompressionTimingSource;
pub use hash_timing::HashTimingSource;
pub use spotlight_timing::SpotlightTimingSource;

use crate::source::EntropySource;

pub fn sources() -> Vec<Box<dyn EntropySource>> {
    vec![
        Box::new(CompressionTimingSource),
        Box::new(HashTimingSource),
        Box::new(SpotlightTimingSource),
    ]
}

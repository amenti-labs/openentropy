mod gpu_divergence;
mod iosurface_crossing;
mod nl_inference_timing;

pub use gpu_divergence::GPUDivergenceSource;
pub use iosurface_crossing::IOSurfaceCrossingSource;
pub use nl_inference_timing::NLInferenceTimingSource;

use crate::source::EntropySource;

pub fn sources() -> Vec<Box<dyn EntropySource>> {
    vec![
        Box::new(GPUDivergenceSource),
        Box::new(IOSurfaceCrossingSource),
        Box::new(NLInferenceTimingSource),
    ]
}

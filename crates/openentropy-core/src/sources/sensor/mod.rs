mod audio;
mod bluetooth;
mod camera;
mod smc_highvar_timing;

pub use audio::{AudioNoiseConfig, AudioNoiseSource};
pub use bluetooth::BluetoothNoiseSource;
pub use camera::{CameraNoiseConfig, CameraNoiseSource};
pub use smc_highvar_timing::SMCHighVarTimingSource;

use crate::source::EntropySource;

pub fn sources() -> Vec<Box<dyn EntropySource>> {
    vec![
        Box::new(AudioNoiseSource::default()),
        Box::new(BluetoothNoiseSource),
        Box::new(CameraNoiseSource::default()),
        Box::new(SMCHighVarTimingSource),
    ]
}

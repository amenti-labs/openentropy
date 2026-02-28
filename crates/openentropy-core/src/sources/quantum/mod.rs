pub mod qcicada_source;

pub use qcicada_source::{QCicadaConfig, QCicadaSource};

use crate::source::EntropySource;

pub fn sources() -> Vec<Box<dyn EntropySource>> {
    vec![Box::new(QCicadaSource::default())]
}

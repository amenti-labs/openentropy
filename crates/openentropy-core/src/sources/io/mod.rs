mod disk;
mod fsync_journal;
mod nvme_iokit_sensors;
mod nvme_passthrough_linux;
mod nvme_raw_device;
mod usb_enumeration;

pub use disk::DiskIOSource;
pub use fsync_journal::FsyncJournalSource;
pub use nvme_iokit_sensors::NvmeIokitSensorsSource;
pub use nvme_passthrough_linux::NvmePassthroughLinuxSource;
pub use nvme_raw_device::NvmeRawDeviceSource;
pub use usb_enumeration::USBEnumerationSource;

use crate::source::EntropySource;

pub fn sources() -> Vec<Box<dyn EntropySource>> {
    vec![
        Box::new(DiskIOSource),
        Box::new(FsyncJournalSource),
        Box::new(NvmeIokitSensorsSource),
        Box::new(NvmePassthroughLinuxSource),
        Box::new(NvmeRawDeviceSource),
        Box::new(USBEnumerationSource),
    ]
}

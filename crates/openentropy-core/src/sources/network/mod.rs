mod dns_timing;
mod tcp_connect;
mod wifi_rssi;

pub use dns_timing::DNSTimingSource;
pub use tcp_connect::TCPConnectSource;
pub use wifi_rssi::WiFiRSSISource;

use crate::source::EntropySource;

pub fn sources() -> Vec<Box<dyn EntropySource>> {
    vec![
        Box::new(DNSTimingSource::new()),
        Box::new(TCPConnectSource::new()),
        Box::new(WiFiRSSISource::new()),
    ]
}

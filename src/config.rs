use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use hyper::http::uri::PathAndQuery;

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct Config {
    /// Listen address of metrics. Used by the client to gather prometheus metrics.
    /// Default: `127.0.0.1:10000`
    pub listen_address: SocketAddr,

    /// Path to the metrics if specified. Any path will work otherwise
    /// Default: None
    #[cfg_attr(
        feature = "serde",
        serde(default, with = "crate::utils::serde_optional_url")
    )]
    pub metrics_path: Option<PathAndQuery>,

    /// Metrics update interval in seconds. Default: 10
    pub collection_interval_sec: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 10000),
            metrics_path: None,
            collection_interval_sec: 10,
        }
    }
}

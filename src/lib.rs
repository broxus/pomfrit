//! # Simple Prometheus metrics exporter with hot reload
//!
//! Example:
//! ```rust
//! use pomfrit::formatter::*;
//!
//! /// Your metrics as a struct
//! struct MyMetrics<'a> {
//!     ctx: &'a str,
//!     some_diff: u32,
//!     some_time: u32,
//! }
//!
//! /// Describe how your metrics will be displayed
//! impl std::fmt::Display for MyMetrics<'_> {
//!     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//!         f.begin_metric("some_diff")
//!             .label("label1", self.ctx)
//!             .label("label2", "some value")
//!             .value(self.some_diff)?;
//!
//!         f.begin_metric("some_time")
//!             .label("label1", self.ctx)
//!             .value(self.some_time)
//!     }
//! }
//!
//! async fn my_app() {
//!     // Create inactive exporter
//!     let (exporter, writer) = pomfrit::create_exporter(None).await.unwrap();
//!
//!     // Spawn task which will run in background and write metrics
//!     writer.spawn(|buf| {
//!         buf.write(MyMetrics {
//!             ctx: "asd",
//!             some_diff: 123,
//!             some_time: 456,
//!         }).write(MyMetrics {
//!             ctx: "qwe",
//!             some_diff: 111,
//!             some_time: 444,
//!         });
//!     });
//!
//!     // ...
//!
//!     // Reload exporter config
//!     exporter.reload(Some(pomfrit::Config {
//!         collection_interval_sec: 10,
//!         ..Default::default()
//!     })).await.unwrap();
//! }
//! ```

////////////////////////////////////////////////////////////////////////////////

#[cfg(any(feature = "http1", feature = "http2"))]
pub use crate::config::*;
#[cfg(any(feature = "http1", feature = "http2"))]
pub use crate::exporter::*;

#[cfg(any(feature = "http1", feature = "http2"))]
mod config;
#[cfg(any(feature = "http1", feature = "http2"))]
mod exporter;
#[cfg(any(feature = "http1", feature = "http2"))]
mod utils;

pub mod formatter;

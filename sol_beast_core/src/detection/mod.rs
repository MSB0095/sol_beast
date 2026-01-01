// New token detection module
// This module provides WebSocket-level filtering and detection logic for new pump.fun tokens

mod detector;
mod metrics;
mod filters;

pub use detector::{NewTokenDetector, DetectionConfig, DetectionResult};
pub use metrics::{DetectionMetrics, MetricsSnapshot};
pub use filters::{LogFilter, should_process_log_notification};

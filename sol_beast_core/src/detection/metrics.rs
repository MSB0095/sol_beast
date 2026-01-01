// Detection metrics tracking
use std::sync::atomic::{AtomicU64, Ordering};
use serde::{Deserialize, Serialize};

/// Tracks detection performance metrics
#[derive(Debug)]
pub struct DetectionMetrics {
    /// Total transactions received from WebSocket
    pub total_received: AtomicU64,
    /// Transactions filtered out early (non-creation)
    pub filtered_early: AtomicU64,
    /// Transactions passed to detection pipeline
    pub passed_to_pipeline: AtomicU64,
    /// Successfully detected new tokens
    pub tokens_detected: AtomicU64,
    /// Detection failures (parse errors, RPC failures, etc.)
    pub detection_failures: AtomicU64,
    /// Duplicate signatures (already seen)
    pub duplicates: AtomicU64,
}

impl DetectionMetrics {
    pub fn new() -> Self {
        Self {
            total_received: AtomicU64::new(0),
            filtered_early: AtomicU64::new(0),
            passed_to_pipeline: AtomicU64::new(0),
            tokens_detected: AtomicU64::new(0),
            detection_failures: AtomicU64::new(0),
            duplicates: AtomicU64::new(0),
        }
    }

    pub fn record_received(&self) {
        self.total_received.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_filtered(&self) {
        self.filtered_early.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_passed(&self) {
        self.passed_to_pipeline.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_detected(&self) {
        self.tokens_detected.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_failure(&self) {
        self.detection_failures.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_duplicate(&self) {
        self.duplicates.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            total_received: self.total_received.load(Ordering::Relaxed),
            filtered_early: self.filtered_early.load(Ordering::Relaxed),
            passed_to_pipeline: self.passed_to_pipeline.load(Ordering::Relaxed),
            tokens_detected: self.tokens_detected.load(Ordering::Relaxed),
            detection_failures: self.detection_failures.load(Ordering::Relaxed),
            duplicates: self.duplicates.load(Ordering::Relaxed),
        }
    }

    pub fn filter_effectiveness_percent(&self) -> f64 {
        let total = self.total_received.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }
        let filtered = self.filtered_early.load(Ordering::Relaxed);
        (filtered as f64 / total as f64) * 100.0
    }

    pub fn success_rate_percent(&self) -> f64 {
        let passed = self.passed_to_pipeline.load(Ordering::Relaxed);
        if passed == 0 {
            return 0.0;
        }
        let detected = self.tokens_detected.load(Ordering::Relaxed);
        (detected as f64 / passed as f64) * 100.0
    }
}

impl Default for DetectionMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Immutable snapshot of detection metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub total_received: u64,
    pub filtered_early: u64,
    pub passed_to_pipeline: u64,
    pub tokens_detected: u64,
    pub detection_failures: u64,
    pub duplicates: u64,
}

impl MetricsSnapshot {
    pub fn filter_effectiveness_percent(&self) -> f64 {
        if self.total_received == 0 {
            return 0.0;
        }
        (self.filtered_early as f64 / self.total_received as f64) * 100.0
    }

    pub fn success_rate_percent(&self) -> f64 {
        if self.passed_to_pipeline == 0 {
            return 0.0;
        }
        (self.tokens_detected as f64 / self.passed_to_pipeline as f64) * 100.0
    }

    pub fn duplicate_rate_percent(&self) -> f64 {
        if self.total_received == 0 {
            return 0.0;
        }
        (self.duplicates as f64 / self.total_received as f64) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_tracking() {
        let metrics = DetectionMetrics::new();
        
        metrics.record_received();
        metrics.record_received();
        metrics.record_filtered();
        metrics.record_passed();
        metrics.record_detected();
        
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.total_received, 2);
        assert_eq!(snapshot.filtered_early, 1);
        assert_eq!(snapshot.passed_to_pipeline, 1);
        assert_eq!(snapshot.tokens_detected, 1);
    }

    #[test]
    fn test_filter_effectiveness() {
        let metrics = DetectionMetrics::new();
        
        // 100 received, 90 filtered
        for _ in 0..100 {
            metrics.record_received();
        }
        for _ in 0..90 {
            metrics.record_filtered();
        }
        
        let effectiveness = metrics.filter_effectiveness_percent();
        assert!((effectiveness - 90.0).abs() < 0.01);
    }
}

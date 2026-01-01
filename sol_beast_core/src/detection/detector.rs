// New token detector with WebSocket-level filtering
use crate::error::CoreError;
use crate::rpc_client::RpcClient;
use crate::metadata::HttpClient;
use crate::settings::Settings;
use crate::pipeline::{process_new_token, DetectedTokenResult};
use super::metrics::DetectionMetrics;
use super::filters::{LogFilter, should_process_log_notification};
use serde_json::Value;
use log::{debug, info, warn};
use std::sync::Arc;

/// Configuration for new token detection
#[derive(Debug, Clone)]
pub struct DetectionConfig {
    /// Pump.fun program ID to monitor
    pub pump_fun_program: String,
    /// Enable fallback sampling for missed tokens (experimental)
    pub enable_fallback_sampling: bool,
    /// Sample rate for fallback (e.g., 0.05 = 5% of non-creation events)
    pub fallback_sample_rate: f64,
}

impl DetectionConfig {
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            pump_fun_program: settings.pump_fun_program.clone(),
            enable_fallback_sampling: false, // Disabled by default
            fallback_sample_rate: 0.05,
        }
    }
}

/// Result of detection attempt
#[derive(Debug, Clone)]
pub struct DetectionResult {
    /// The detected token result from pipeline
    pub token: DetectedTokenResult,
    /// Whether this was detected via primary filter or fallback
    pub source: DetectionSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectionSource {
    /// Detected via WebSocket log filter (primary path)
    LogFilter,
    /// Detected via fallback sampling (experimental)
    FallbackSampling,
}

/// New token detector with WebSocket-level filtering
/// 
/// This detector implements the core logic for identifying new pump.fun token
/// creations with early filtering at the WebSocket level to minimize processing
/// overhead.
pub struct NewTokenDetector {
    config: DetectionConfig,
    filter: LogFilter,
    metrics: Arc<DetectionMetrics>,
}

impl NewTokenDetector {
    pub fn new(config: DetectionConfig) -> Self {
        let filter = LogFilter::new(config.pump_fun_program.clone());
        let metrics = Arc::new(DetectionMetrics::new());
        
        Self {
            config,
            filter,
            metrics,
        }
    }

    /// Get reference to detection metrics
    pub fn metrics(&self) -> &Arc<DetectionMetrics> {
        &self.metrics
    }

    /// Check if a WebSocket notification should be processed
    /// 
    /// This performs early filtering to identify token creation events before
    /// fetching full transaction data.
    /// 
    /// Returns:
    /// - `Ok(Some(signature))` if this is a creation event to process
    /// - `Ok(None)` if this should be filtered out
    /// - `Err` if there's a parsing error
    pub fn should_process_notification(
        &self,
        notification_json: &Value,
    ) -> Result<Option<String>, CoreError> {
        self.metrics.record_received();
        
        match should_process_log_notification(notification_json, &self.filter) {
            Ok(Some(signature)) => {
                debug!("Log filter identified creation event: {}", signature);
                self.metrics.record_passed();
                Ok(Some(signature))
            }
            Ok(None) => {
                self.metrics.record_filtered();
                
                // Experimental: fallback sampling
                if self.config.enable_fallback_sampling {
                    if rand::random::<f64>() < self.config.fallback_sample_rate {
                        // Sample this transaction as fallback
                        if let Ok(Some(sig)) = self.try_extract_signature(notification_json) {
                            debug!("Fallback sampling selected: {}", sig);
                            self.metrics.record_passed();
                            return Ok(Some(sig));
                        }
                    }
                }
                
                Ok(None)
            }
            Err(e) => {
                warn!("Error filtering notification: {}", e);
                Err(CoreError::ParseError(e))
            }
        }
    }

    /// Detect new token from signature
    /// 
    /// This runs the full detection pipeline after a signature has been
    /// identified as potentially being a new token creation.
    pub async fn detect_new_token<R: RpcClient + ?Sized, H: HttpClient + ?Sized>(
        &self,
        signature: String,
        rpc_client: &R,
        http_client: &H,
        settings: &Settings,
    ) -> Result<DetectionResult, CoreError> {
        info!("Detecting new token from signature: {}", signature);
        
        // Run through the detection pipeline
        match process_new_token(
            signature.clone(),
            rpc_client,
            http_client,
            settings,
        ).await {
            Ok(token) => {
                self.metrics.record_detected();
                info!(
                    "Successfully detected new token: {} (mint: {})",
                    token.name.as_deref().unwrap_or("Unknown"),
                    token.mint
                );
                
                Ok(DetectionResult {
                    token,
                    source: DetectionSource::LogFilter,
                })
            }
            Err(e) => {
                self.metrics.record_failure();
                warn!("Failed to detect token from signature {}: {:?}", signature, e);
                Err(e)
            }
        }
    }

    /// Try to extract signature from any notification (for fallback sampling)
    fn try_extract_signature(&self, notification_json: &Value) -> Result<Option<String>, CoreError> {
        let signature = notification_json
            .get("params")
            .and_then(|p| p.get("result"))
            .and_then(|r| r.get("value"))
            .and_then(|v| v.get("signature"))
            .and_then(|s| s.as_str())
            .map(|s| s.to_string());
        
        Ok(signature)
    }

    /// Log current detection metrics
    pub fn log_metrics(&self) {
        let snapshot = self.metrics.snapshot();
        info!(
            "Detection metrics: received={} filtered={} ({:.1}%) passed={} detected={} failures={} duplicates={}",
            snapshot.total_received,
            snapshot.filtered_early,
            snapshot.filter_effectiveness_percent(),
            snapshot.passed_to_pipeline,
            snapshot.tokens_detected,
            snapshot.detection_failures,
            snapshot.duplicates
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_detector_creation() {
        let config = DetectionConfig {
            pump_fun_program: "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P".to_string(),
            enable_fallback_sampling: false,
            fallback_sample_rate: 0.05,
        };
        
        let detector = NewTokenDetector::new(config);
        let snapshot = detector.metrics().snapshot();
        
        assert_eq!(snapshot.total_received, 0);
        assert_eq!(snapshot.filtered_early, 0);
    }

    #[test]
    fn test_should_process_creation_notification() {
        let config = DetectionConfig {
            pump_fun_program: "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P".to_string(),
            enable_fallback_sampling: false,
            fallback_sample_rate: 0.05,
        };
        
        let detector = NewTokenDetector::new(config);
        
        let notification = json!({
            "jsonrpc": "2.0",
            "method": "logsNotification",
            "params": {
                "result": {
                    "value": {
                        "signature": "test123",
                        "err": null,
                        "logs": [
                            "Program log: Instruction: Create"
                        ]
                    }
                }
            }
        });
        
        let result = detector.should_process_notification(&notification).unwrap();
        assert_eq!(result, Some("test123".to_string()));
        
        let snapshot = detector.metrics().snapshot();
        assert_eq!(snapshot.total_received, 1);
        assert_eq!(snapshot.passed_to_pipeline, 1);
    }

    #[test]
    fn test_should_filter_non_creation() {
        let config = DetectionConfig {
            pump_fun_program: "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P".to_string(),
            enable_fallback_sampling: false,
            fallback_sample_rate: 0.05,
        };
        
        let detector = NewTokenDetector::new(config);
        
        let notification = json!({
            "jsonrpc": "2.0",
            "method": "logsNotification",
            "params": {
                "result": {
                    "value": {
                        "signature": "test456",
                        "err": null,
                        "logs": [
                            "Program log: Instruction: Buy"
                        ]
                    }
                }
            }
        });
        
        let result = detector.should_process_notification(&notification).unwrap();
        assert_eq!(result, None);
        
        let snapshot = detector.metrics().snapshot();
        assert_eq!(snapshot.total_received, 1);
        assert_eq!(snapshot.filtered_early, 1);
        assert_eq!(snapshot.passed_to_pipeline, 0);
    }
}

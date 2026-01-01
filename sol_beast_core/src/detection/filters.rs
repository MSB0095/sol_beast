// WebSocket-level filters for new token detection
use serde_json::Value;
use log::debug;

/// Filter configuration for log notifications
#[derive(Debug, Clone)]
pub struct LogFilter {
    /// Program ID to filter for (pump.fun)
    pub program_id: String,
    /// Log patterns that indicate token creation
    pub creation_patterns: Vec<String>,
}

impl LogFilter {
    pub fn new(program_id: String) -> Self {
        Self {
            program_id,
            creation_patterns: vec![
                "Program log: Instruction: Create".to_string(),
                "Program log: Instruction: create".to_string(),
            ],
        }
    }

    /// Check if a log entry matches creation patterns
    pub fn matches_creation_pattern(&self, log: &str) -> bool {
        self.creation_patterns.iter().any(|pattern| log.contains(pattern))
    }
}

/// Pre-filter WebSocket log notifications to identify token creation events
/// 
/// This function performs early filtering at the WebSocket level to reduce
/// the number of transactions that need to be processed by the full pipeline.
/// 
/// Returns:
/// - `Ok(Some(signature))` if this is a creation event that should be processed
/// - `Ok(None)` if this should be filtered out
/// - `Err(msg)` if there's a parsing error
pub fn should_process_log_notification(
    notification_json: &Value,
    filter: &LogFilter,
) -> Result<Option<String>, String> {
    // Check if this is a logsNotification
    let method = notification_json
        .get("method")
        .and_then(|m| m.as_str())
        .ok_or("Missing method field")?;
    
    if method != "logsNotification" {
        return Ok(None);
    }
    
    // Extract the notification value
    let params = notification_json
        .get("params")
        .ok_or("Missing params")?;
    
    let result = params
        .get("result")
        .ok_or("Missing result in params")?;
    
    let value = result
        .get("value")
        .ok_or("Missing value in result")?;
    
    // Check for errors - skip failed transactions
    if let Some(err) = value.get("err") {
        if !err.is_null() {
            debug!("Skipping failed transaction with error: {:?}", err);
            return Ok(None);
        }
    }
    
    // Extract signature
    let signature = value
        .get("signature")
        .and_then(|s| s.as_str())
        .ok_or("Missing signature")?
        .to_string();
    
    // Check logs for creation patterns
    if let Some(logs) = value.get("logs").and_then(|l| l.as_array()) {
        for log_val in logs {
            if let Some(log) = log_val.as_str() {
                if filter.matches_creation_pattern(log) {
                    debug!("Detected creation pattern in log for signature: {}", signature);
                    return Ok(Some(signature));
                }
            }
        }
    }
    
    // No creation pattern found
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_creation_pattern_matching() {
        let filter = LogFilter::new("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P".to_string());
        
        assert!(filter.matches_creation_pattern("Program log: Instruction: Create"));
        assert!(filter.matches_creation_pattern("Program log: Instruction: create"));
        assert!(!filter.matches_creation_pattern("Program log: Instruction: Buy"));
        assert!(!filter.matches_creation_pattern("Some other log"));
    }

    #[test]
    fn test_should_process_creation_event() {
        let filter = LogFilter::new("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P".to_string());
        
        let notification = json!({
            "jsonrpc": "2.0",
            "method": "logsNotification",
            "params": {
                "result": {
                    "value": {
                        "signature": "5xF...abc",
                        "err": null,
                        "logs": [
                            "Program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P invoke [1]",
                            "Program log: Instruction: Create",
                            "Program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P consumed 12345 compute units"
                        ]
                    }
                }
            }
        });
        
        let result = should_process_log_notification(&notification, &filter).unwrap();
        assert_eq!(result, Some("5xF...abc".to_string()));
    }

    #[test]
    fn test_should_filter_non_creation() {
        let filter = LogFilter::new("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P".to_string());
        
        let notification = json!({
            "jsonrpc": "2.0",
            "method": "logsNotification",
            "params": {
                "result": {
                    "value": {
                        "signature": "5xF...xyz",
                        "err": null,
                        "logs": [
                            "Program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P invoke [1]",
                            "Program log: Instruction: Buy",
                            "Program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P consumed 12345 compute units"
                        ]
                    }
                }
            }
        });
        
        let result = should_process_log_notification(&notification, &filter).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_should_filter_failed_transaction() {
        let filter = LogFilter::new("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P".to_string());
        
        let notification = json!({
            "jsonrpc": "2.0",
            "method": "logsNotification",
            "params": {
                "result": {
                    "value": {
                        "signature": "5xF...err",
                        "err": {"InstructionError": [0, "Custom"]},
                        "logs": [
                            "Program log: Instruction: Create"
                        ]
                    }
                }
            }
        });
        
        let result = should_process_log_notification(&notification, &filter).unwrap();
        assert_eq!(result, None);
    }
}

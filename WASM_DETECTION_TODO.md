# WASM Detection Improvements TODO

## Current State

The WASM monitor (`sol_beast_wasm/src/monitor.rs`) currently:
- ✅ Subscribes to pump.fun program logs via `logsSubscribe`
- ⚠️ Processes ALL logsNotification events
- ❌ Does NOT filter for "Instruction: Create" pattern before processing
- ❌ Does NOT use the new `NewTokenDetector` from core

## Required Changes

### 1. Add Creation Pattern Filtering

Update the message handler (around line 257-283) to filter for creation events:

```rust
if method == "logsNotification" {
    // Increment pump.fun message count
    increment_counter(&pump_msg_count_for_handler);
    
    if let Some(params) = value.get("params").and_then(|p| p.get("result")).and_then(|r| r.get("value")) {
        // Extract logs and signature
        let logs = params.get("logs").and_then(|l| l.as_array());
        let signature = params.get("signature").and_then(|s| s.as_str());
        let err = params.get("err");
        
        // Skip failed transactions
        if let Some(e) = err {
            if !e.is_null() {
                increment_counter(&filtered_count_for_handler);
                return;
            }
        }
        
        // CHECK FOR CREATION PATTERN
        if let (Some(logs), Some(sig)) = (logs, signature) {
            let mut is_create = false;
            for log_val in logs {
                if let Some(log) = log_val.as_str() {
                    if log.contains("Program log: Instruction: Create") {
                        is_create = true;
                        break;
                    }
                }
            }
            
            if is_create {
                increment_counter(&create_count_for_handler);
                info!("CREATE instruction detected: {}", sig);
                
                // Call signature callback only for creation events
                if let Some(cb) = &sig_callback {
                    cb(sig.to_string());
                }
            } else {
                // Not a creation - filter it out
                increment_counter(&filtered_count_for_handler);
            }
        }
    }
}
```

### 2. Use NewTokenDetector (Optional Enhancement)

For better consistency with CLI, consider refactoring to use `NewTokenDetector`:

```rust
// Create detector in Monitor::new()
use sol_beast_core::detection::{NewTokenDetector, DetectionConfig};

pub struct Monitor {
    // ... existing fields ...
    detector: Arc<NewTokenDetector>,
}

// In message handler:
match detector.should_process_notification(&value) {
    Ok(Some(signature)) => {
        // Process creation event
        if let Some(cb) = &sig_callback {
            cb(signature);
        }
    }
    Ok(None) => {
        // Filtered out
        increment_counter(&filtered_count_for_handler);
    }
    Err(e) => {
        error!("Error filtering: {:?}", e);
    }
}
```

### 3. Add Metrics Logging

Add periodic metrics logging similar to CLI:

```rust
// Every 60 seconds, log detection metrics
if current_count % (STATUS_LOG_FREQUENCY * 12) == 0 {
    let snapshot = detector.metrics().snapshot();
    log_cb_for_msg(
        "info".to_string(),
        "Detection Metrics".to_string(),
        format!(
            "Received: {}\nFiltered: {} ({:.1}%)\nProcessed: {}\nDetected: {}",
            snapshot.total_received,
            snapshot.filtered_early,
            snapshot.filter_effectiveness_percent(),
            snapshot.passed_to_pipeline,
            snapshot.tokens_detected
        )
    );
}
```

## Benefits

After implementing these changes, WASM will:
- ✅ Filter 95-99% of pump.fun logs before processing
- ✅ Match CLI filtering effectiveness
- ✅ Reduce unnecessary processing and RPC calls
- ✅ Provide consistent metrics across CLI and WASM

## Testing

1. Build WASM with changes: `./build-wasm.sh`
2. Test in browser with real WebSocket connection
3. Verify filtering metrics show >95% effectiveness
4. Confirm new tokens are still detected

## Priority

**Medium** - The WASM monitor already uses `logsSubscribe` which provides good filtering. This improvement adds the second level of filtering (creation pattern matching) for consistency with CLI and better performance.

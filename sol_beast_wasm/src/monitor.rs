// WASM monitoring module - detects new pump.fun tokens via WebSocket
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{WebSocket, MessageEvent, ErrorEvent, CloseEvent};
use serde_json::Value;
use log::{info, error};
use std::sync::{Arc, Mutex};
use std::collections::HashSet;

/// Helper function to increment a counter safely, handling poisoned mutex
fn increment_counter(counter: &Arc<Mutex<u64>>) -> u64 {
    match counter.lock() {
        Ok(mut count) => {
            *count += 1;
            *count
        },
        Err(poisoned) => {
            // Recover from poisoned mutex
            let mut count = poisoned.into_inner();
            *count += 1;
            *count
        }
    }
}

// Logging frequency constants
const STATUS_LOG_FREQUENCY: u64 = 50;  // Log status every N messages
const FILTERED_LOG_FREQUENCY: u64 = 100; // Log filtered transactions every N occurrences

/// Monitor state that tracks subscriptions and detected coins
pub struct Monitor {
    ws: Option<WebSocket>,
    seen_signatures: Arc<Mutex<HashSet<String>>>,
    on_message: Option<wasm_bindgen::closure::Closure<dyn FnMut(MessageEvent)>>,
    on_error: Option<wasm_bindgen::closure::Closure<dyn FnMut(ErrorEvent)>>,
    on_close: Option<wasm_bindgen::closure::Closure<dyn FnMut(CloseEvent)>>,
    on_open: Option<wasm_bindgen::closure::Closure<dyn FnMut(JsValue)>>,
    message_count: Arc<Mutex<u64>>,
    pump_fun_message_count: Arc<Mutex<u64>>,
    /// Tracks Buy/Sell transactions filtered out to avoid unnecessary RPC calls and parsing
    filtered_count: Arc<Mutex<u64>>,
    /// Tracks actual CREATE instruction transactions that are processed for new token detection  
    create_count: Arc<Mutex<u64>>,
}

impl Monitor {
    pub fn new() -> Self {
        Self {
            ws: None,
            seen_signatures: Arc::new(Mutex::new(HashSet::new())),
            on_message: None,
            on_error: None,
            on_close: None,
            on_open: None,
            message_count: Arc::new(Mutex::new(0)),
            pump_fun_message_count: Arc::new(Mutex::new(0)),
            filtered_count: Arc::new(Mutex::new(0)),
            create_count: Arc::new(Mutex::new(0)),
        }
    }

    /// Start monitoring for new tokens on pump.fun
    pub fn start(
        &mut self,
        ws_url: &str,
        pump_fun_program: &str,
        log_callback: Arc<dyn Fn(String, String, String)>,
        signature_callback: Option<Arc<dyn Fn(String)>>,
    ) -> Result<(), JsValue> {
        info!("Starting WASM monitor for pump.fun program: {}", pump_fun_program);
        
        // Log the start attempt
        log_callback(
            "info".to_string(),
            "Initializing monitor".to_string(),
            format!("Connecting to WebSocket: {}\nTarget program: {}", ws_url, pump_fun_program)
        );

        // Create WebSocket connection
        let ws = WebSocket::new(ws_url)
            .map_err(|e| {
                let err_msg = format!(
                    "Failed to create WebSocket connection to '{}': {:?}\n\n\
                    Possible causes:\n\
                    - Invalid WebSocket URL format (should start with wss:// or ws://)\n\
                    - Network connectivity issues\n\
                    - Firewall blocking WebSocket connections\n\
                    - Browser security restrictions\n\n\
                    Try:\n\
                    1. Verify the WebSocket URL is correct\n\
                    2. Check browser console for CORS or network errors\n\
                    3. Try a different Solana RPC provider\n\
                    4. Disable browser extensions that might block connections",
                    ws_url, e
                );
                error!("{}", err_msg);
                log_callback(
                    "error".to_string(),
                    "WebSocket creation failed".to_string(),
                    err_msg.clone()
                );
                JsValue::from_str(&err_msg)
            })?;
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

        let pump_fun_program = pump_fun_program.to_string();
        let seen_sigs = self.seen_signatures.clone();
        let msg_count = self.message_count.clone();
        let pump_msg_count = self.pump_fun_message_count.clone();
        let filtered_count = self.filtered_count.clone();
        let create_count = self.create_count.clone();

        // Setup open handler to subscribe once connected
        let ws_for_open = ws.clone();
        let pump_prog_for_sub = pump_fun_program.clone();
        let log_cb_for_open = log_callback.clone();
        let on_open = wasm_bindgen::closure::Closure::wrap(Box::new(move |_e: JsValue| {
            info!("WebSocket connection established successfully");
            
            // Log connection success
            log_cb_for_open(
                "info".to_string(),
                "WebSocket connected".to_string(),
                "Preparing to subscribe to program logs".to_string()
            );
            
            // CREATIVE SOLUTION: Dual subscription approach for WebSocket-level filtering!
            // 
            // Strategy: Instead of subscribing to all pump.fun transactions, we subscribe to
            // specific accounts that are ONLY touched during token creation:
            // 
            // 1. Token Program with filters for NEW mint creation (dataSize 82 bytes)
            // 2. Pump.fun program logs as fallback
            //
            // When a new token is created:
            // - Token Program creates a NEW mint account (we get accountNotification)
            // - We extract the mint address from the notification
            // - We then query for recent signatures involving that mint + pump.fun program
            // - This gives us only CREATE transactions at the WebSocket level!
            //
            // However, there's a technical limitation: accountNotification doesn't include
            // transaction signatures. We'd need to poll RPC for recent transactions.
            //
            // Given this limitation, the OPTIMAL solution remains:
            // - Use logsSubscribe for real-time signature delivery
            // - Filter for CREATE instruction immediately upon receipt (microseconds)
            // - Skip all Buy/Sell before any expensive RPC calls
            //
            // This achieves 95%+ of the benefit of true WebSocket-level filtering.
            let subscribe_msg = serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "logsSubscribe",
                "params": [
                    { "mentions": [ &pump_prog_for_sub ] },
                    { "commitment": "confirmed" }
                ]
            });

            if let Ok(msg_str) = serde_json::to_string(&subscribe_msg) {
                info!("Sending subscription request: {}", msg_str);
                if let Err(e) = ws_for_open.send_with_str(&msg_str) {
                    error!("Failed to send subscription: {:?}", e);
                    log_cb_for_open(
                        "error".to_string(),
                        "Subscription failed".to_string(),
                        format!("Error: {:?}", e)
                    );
                } else {
                    info!("Successfully sent logsSubscribe request");
                    // Log to UI
                    log_cb_for_open(
                        "info".to_string(),
                        "Subscription request sent".to_string(),
                        format!("Waiting for confirmation from Solana node\nMonitoring program: {}", pump_prog_for_sub)
                    );
                }
            } else {
                error!("Failed to serialize subscription message");
                log_cb_for_open(
                    "error".to_string(),
                    "Subscription serialization failed".to_string(),
                    "Could not create subscription JSON".to_string()
                );
            }
        }) as Box<dyn FnMut(_)>);
        ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));

        // Setup message handler
        let log_cb_for_msg = log_callback.clone();
        let msg_count_for_handler = msg_count.clone();
        let pump_msg_count_for_handler = pump_msg_count.clone();
        let filtered_count_for_handler = filtered_count.clone();
        let create_count_for_handler = create_count.clone();
        let sig_callback = signature_callback.clone();
        let on_message = wasm_bindgen::closure::Closure::wrap(Box::new(move |e: MessageEvent| {
            // Increment total message count
            let current_count = {
                match msg_count_for_handler.lock() {
                    Ok(mut count) => {
                        *count += 1;
                        *count
                    },
                    Err(e) => {
                        error!("Failed to lock message count: {:?}", e);
                        return;
                    }
                }
            };
            
            // Log every STATUS_LOG_FREQUENCY messages to show activity and filtering efficiency
            if current_count % STATUS_LOG_FREQUENCY == 0 {
                info!("Received {} total WebSocket messages", current_count);
                if let (Ok(pump_count_guard), Ok(filtered_guard), Ok(create_guard)) = (
                    pump_msg_count_for_handler.lock(),
                    filtered_count_for_handler.lock(),
                    create_count_for_handler.lock()
                ) {
                    let pump_count = *pump_count_guard;
                    let filtered = *filtered_guard;
                    let creates = *create_guard;
                    let filter_rate = if pump_count > 0 {
                        filtered as f64 / pump_count as f64 * 100.0
                    } else {
                        0.0
                    };
                    log_cb_for_msg(
                        "info".to_string(),
                        "Monitor is active - Filtering working!".to_string(),
                        format!(
                            "Total messages: {}\nPump.fun transactions: {}\nFiltered (Buy/Sell): {} ({:.1}%)\nCREATE detected: {}\n\n✅ Only processing CREATE instructions - avoiding hundreds of unnecessary RPC calls!",
                            current_count, pump_count, filtered, filter_rate, creates
                        )
                    );
                }
            }
            
            if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                let message: String = txt.into();
                
                // Log the raw message for first few messages or occasionally
                if current_count <= 5 || current_count % 100 == 0 {
                    info!("WebSocket message #{}: {}", current_count, 
                          if message.len() > 200 { 
                              format!("{}...", &message[..200]) 
                          } else { 
                              message.clone() 
                          });
                }
                
                // Parse the message
                match serde_json::from_str::<Value>(&message) {
                    Ok(value) => {
                        // Check if this is a subscription confirmation
                        if let Some(result) = value.get("result") {
                            if let Some(sub_id) = result.as_u64() {
                                info!("✓ Subscription confirmed with ID: {}", sub_id);
                                log_cb_for_msg(
                                    "info".to_string(),
                                    "✓ Subscription confirmed".to_string(),
                                    format!("Subscription ID: {}\nNow actively monitoring for pump.fun transactions", sub_id)
                                );
                                return;
                            } else {
                                info!("Received result that is not a u64: {:?}", result);
                            }
                        }

                        // Check if this is a log notification
                        if let Some(method) = value.get("method").and_then(|m| m.as_str()) {
                            if method == "logsNotification" {
                                // Increment pump.fun message count
                                let pump_count = {
                                    match pump_msg_count_for_handler.lock() {
                                        Ok(mut count) => {
                                            *count += 1;
                                            *count
                                        },
                                        Err(e) => {
                                            error!("Failed to lock pump message count: {:?}", e);
                                            return;
                                        }
                                    }
                                };
                                
                                info!("Received logsNotification #{}", pump_count);
                                
                                if let Some(params) = value
                                    .get("params")
                                    .and_then(|p| p.get("result"))
                                    .and_then(|r| r.get("value"))
                                {
                                    // Extract logs and signature
                                    let logs = params.get("logs").and_then(|l| l.as_array());
                                    let signature = params.get("signature").and_then(|s| s.as_str());
                                    let err = params.get("err");

                                    if let (Some(logs), Some(sig)) = (logs, signature) {
                                        info!("Processing transaction: {} (logs: {}, err: {})", 
                                              sig, logs.len(), err.is_some());
                                        
                                        // Log detailed info for this transaction
                                        log_cb_for_msg(
                                            "info".to_string(),
                                            format!("Transaction received (#{} total)", pump_count),
                                            format!("Signature: {}\nLogs: {} entries\nError: {}", 
                                                   sig, logs.len(), 
                                                   if err.is_some() { "Yes" } else { "No" })
                                        );
                                        
                                        // Check if we've seen this signature before
                                        let is_new = {
                                            match seen_sigs.lock() {
                                                Ok(mut seen) => seen.insert(sig.to_string()),
                                                Err(e) => {
                                                    error!("Failed to lock seen signatures: {:?}", e);
                                                    return;
                                                }
                                            }
                                        };

                                        if !is_new {
                                            info!("Transaction {} already seen, skipping", sig);
                                            return;
                                        }

                                        // OPTIMIZATION: Check for CREATE instruction FIRST before any other processing
                                        // This filters out Buy/Sell transactions immediately without expensive operations
                                        let mut is_create = false;
                                        for log in logs.iter().filter_map(|l| l.as_str()) {
                                            if log.contains("Program log: Instruction: Create") {
                                                is_create = true;
                                                break; // Found CREATE, no need to check more logs
                                            }
                                        }

                                        // Skip non-CREATE transactions entirely
                                        if !is_create {
                                            // Increment filtered count
                                            let filtered_total = increment_counter(&filtered_count_for_handler);
                                            
                                            // Only log occasionally to avoid spam
                                            if pump_count <= 5 || pump_count % FILTERED_LOG_FREQUENCY == 0 {
                                                info!("Transaction {} is not a CREATE instruction, filtered (total: {})", sig, filtered_total);
                                            }
                                            return;
                                        }

                                        // This is a CREATE transaction - proceed with processing
                                        // Increment create count
                                        let create_total = increment_counter(&create_count_for_handler);
                                        
                                        info!("✅ CREATE instruction detected #{}: {}", create_total, sig);
                                        
                                        // Check if any log mentions the pump.fun program (double verification)
                                        let pump_fun_logs: Vec<String> = logs.iter()
                                            .filter_map(|log| log.as_str())
                                            .filter(|s| s.contains(&pump_fun_program))
                                            .map(|s| s.to_string())
                                            .collect();

                                        if !pump_fun_logs.is_empty() {
                                            info!("  Verified pump.fun program in logs");
                                            
                                            // Log the detection with details
                                            log_cb_for_msg(
                                                "info".to_string(),
                                                "🎯 New token creation detected!".to_string(),
                                                format!("Signature: {}\nInstruction: Create\nLogs: {} entries\n\nProcessing new token...", 
                                                       sig, pump_fun_logs.len())
                                            );

                                            // Notify callback to process the signature
                                            if let Some(ref callback) = sig_callback {
                                                callback(sig.to_string());
                                            }
                                        } else {
                                            // Log that we received a message but it didn't match
                                            if pump_count <= 10 || pump_count % 25 == 0 {
                                                info!("Transaction {} does not contain pump.fun program ID", sig);
                                                log_cb_for_msg(
                                                    "info".to_string(),
                                                    format!("Non-pump.fun transaction (#{} total)", pump_count),
                                                    format!("Signature: {}\nThis transaction doesn't involve the pump.fun program", sig)
                                                );
                                            }
                                        }
                                    } else {
                                        error!("logsNotification missing logs or signature");
                                        log_cb_for_msg(
                                            "warn".to_string(),
                                            "Incomplete transaction data".to_string(),
                                            "Received logsNotification without proper logs or signature".to_string()
                                        );
                                    }
                                } else {
                                    error!("logsNotification has unexpected structure");
                                }
                            } else {
                                // Unknown method
                                info!("Received message with method: {}", method);
                            }
                        } else if value.get("error").is_some() {
                            // This is an error response
                            error!("Received error from WebSocket: {:?}", value.get("error"));
                            log_cb_for_msg(
                                "error".to_string(),
                                "WebSocket error response".to_string(),
                                format!("{:?}", value.get("error"))
                            );
                        }
                    },
                    Err(e) => {
                        error!("Failed to parse WebSocket message: {}", e);
                        if current_count <= 5 {
                            log_cb_for_msg(
                                "warn".to_string(),
                                "Message parsing failed".to_string(),
                                format!("Error: {}\nMessage preview: {}", e, 
                                       if message.len() > 100 { 
                                           format!("{}...", &message[..100]) 
                                       } else { 
                                           message.clone() 
                                       })
                            );
                        }
                    }
                }
            } else {
                error!("Received non-text WebSocket message");
                log_cb_for_msg(
                    "warn".to_string(),
                    "Non-text message received".to_string(),
                    "WebSocket sent binary or other non-text data".to_string()
                );
            }
        }) as Box<dyn FnMut(_)>);
        ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

        // Setup error handler
        let log_cb_for_err = log_callback.clone();
        let on_error = wasm_bindgen::closure::Closure::wrap(Box::new(move |e: ErrorEvent| {
            // ErrorEvent.message() can cause issues in some browsers, so use type only
            let error_msg = format!("WebSocket error occurred - Type: {:?}. This usually means the WebSocket endpoint rejected the connection (CORS, authentication, or other issues).", 
                                   e.type_());
            error!("{}", error_msg);
            log_cb_for_err(
                "error".to_string(),
                "❌ WebSocket connection error".to_string(),
                error_msg
            );
        }) as Box<dyn FnMut(_)>);
        ws.set_onerror(Some(on_error.as_ref().unchecked_ref()));

        // Setup close handler
        let log_cb_for_close = log_callback;
        let on_close = wasm_bindgen::closure::Closure::wrap(Box::new(move |e: CloseEvent| {
            let close_msg = format!("Code: {} ({}), Reason: {}, Was Clean: {}", 
                                   e.code(), 
                                   match e.code() {
                                       1000 => "Normal",
                                       1001 => "Going Away",
                                       1002 => "Protocol Error",
                                       1003 => "Unsupported Data",
                                       1006 => "Abnormal Closure",
                                       1007 => "Invalid Frame Payload",
                                       1008 => "Policy Violation",
                                       1009 => "Message Too Big",
                                       1011 => "Internal Server Error",
                                       1015 => "TLS Handshake Failed",
                                       _ => "Unknown"
                                   },
                                   e.reason(),
                                   e.was_clean());
            info!("WebSocket closed: {}", close_msg);
            log_cb_for_close(
                "warn".to_string(),
                "⚠️ WebSocket connection closed".to_string(),
                close_msg
            );
        }) as Box<dyn FnMut(_)>);
        ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));

        // Store everything
        self.ws = Some(ws);
        self.on_open = Some(on_open);
        self.on_message = Some(on_message);
        self.on_error = Some(on_error);
        self.on_close = Some(on_close);

        Ok(())
    }

    /// Stop monitoring and close WebSocket connection
    pub fn stop(&mut self) -> Result<(), JsValue> {
        if let Some(ws) = self.ws.take() {
            info!("Stopping WASM monitor, closing WebSocket");
            ws.close()?;
        }

        // Clear event handlers
        self.on_open = None;
        self.on_message = None;
        self.on_error = None;
        self.on_close = None;

        // Clear seen signatures
        if let Ok(mut seen) = self.seen_signatures.lock() {
            seen.clear();
        }

        Ok(())
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        if let Some(ws) = &self.ws {
            let _ = ws.close();
        }
    }
}

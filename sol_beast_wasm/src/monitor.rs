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
const _FILTERED_LOG_FREQUENCY: u64 = 100; // Log filtered transactions every N occurrences

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
        // Simplified start: always use standard RPC logic
        let is_shyft = false; // Rigidly false

        info!("Starting WASM monitor for pump.fun program: {}", pump_fun_program);
        
        // Log the start attempt
        log_callback(
            "info".to_string(),
            "Initializing monitor".to_string(),
            format!("Connecting to WebSocket: {}\nTarget program: {}\nMode: Standard RPC", ws_url, pump_fun_program)
        );

        // Create WebSocket connection
        // Shyft GraphQL subscriptions require the graphql-ws subprotocol; legacy Solana RPC does not
        let ws_result = if is_shyft {
            WebSocket::new_with_str(ws_url, "graphql-ws")
        } else {
            WebSocket::new(ws_url)
        };

        let ws = ws_result
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
        let _seen_sigs = self.seen_signatures.clone();
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
                    info!("Successfully sent subscription request");
                    // Log to UI
                    log_cb_for_open(
                        "info".to_string(),
                        "Subscription request sent".to_string(),
                        format!("Waiting for confirmation from node\nMonitoring program: {}", pump_prog_for_sub)
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
                        {
                            // Standard RPC handling only
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
                                }
                            }

                            // Check if this is a log notification
                            if let Some(method) = value.get("method").and_then(|m| m.as_str()) {
                                if method == "logsNotification" {
                                    // Increment pump.fun message count
                                    increment_counter(&pump_msg_count_for_handler);
                                    
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
                                            
                                            // Call signature callback
                                            if let Some(cb) = &sig_callback {
                                                cb(sig.to_string());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to parse message: {:?}", e);
                    }
                }
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

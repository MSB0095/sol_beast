// WASM monitoring module - detects new pump.fun tokens via WebSocket
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{WebSocket, MessageEvent, ErrorEvent, CloseEvent};
use serde_json::Value;
use log::{info, error};
use std::sync::{Arc, Mutex};
use std::collections::HashSet;

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
        }
    }

    /// Start monitoring for new tokens on pump.fun
    pub fn start(
        &mut self,
        ws_url: &str,
        pump_fun_program: &str,
        log_callback: Arc<Mutex<dyn FnMut(String, String, String)>>,
    ) -> Result<(), JsValue> {
        info!("Starting WASM monitor for pump.fun program: {}", pump_fun_program);
        
        // Log the start attempt
        if let Ok(mut cb) = log_callback.lock() {
            cb(
                "info".to_string(),
                "Initializing monitor".to_string(),
                format!("Connecting to WebSocket: {}\nTarget program: {}", ws_url, pump_fun_program)
            );
        }

        // Create WebSocket connection
        let ws = WebSocket::new(ws_url)
            .map_err(|e| {
                let err_msg = format!("Failed to create WebSocket: {:?}", e);
                error!("{}", err_msg);
                if let Ok(mut cb) = log_callback.lock() {
                    cb(
                        "error".to_string(),
                        "WebSocket creation failed".to_string(),
                        err_msg.clone()
                    );
                }
                JsValue::from_str(&err_msg)
            })?;
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

        let pump_fun_program = pump_fun_program.to_string();
        let seen_sigs = self.seen_signatures.clone();
        let msg_count = self.message_count.clone();
        let pump_msg_count = self.pump_fun_message_count.clone();

        // Setup open handler to subscribe once connected
        let ws_for_open = ws.clone();
        let pump_prog_for_sub = pump_fun_program.clone();
        let log_cb_for_open = log_callback.clone();
        let on_open = wasm_bindgen::closure::Closure::wrap(Box::new(move |_e: JsValue| {
            info!("WebSocket connection established successfully");
            
            // Log connection success
            if let Ok(mut cb) = log_cb_for_open.lock() {
                cb(
                    "info".to_string(),
                    "WebSocket connected".to_string(),
                    "Preparing to subscribe to program logs".to_string()
                );
            }
            
            // Subscribe to pump.fun program logs
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
                    if let Ok(mut cb) = log_cb_for_open.lock() {
                        cb(
                            "error".to_string(),
                            "Subscription failed".to_string(),
                            format!("Error: {:?}", e)
                        );
                    }
                } else {
                    info!("Successfully sent logsSubscribe request");
                    // Log to UI
                    if let Ok(mut cb) = log_cb_for_open.lock() {
                        cb(
                            "info".to_string(),
                            "Subscription request sent".to_string(),
                            format!("Waiting for confirmation from Solana node\nMonitoring program: {}", pump_prog_for_sub)
                        );
                    }
                }
            } else {
                error!("Failed to serialize subscription message");
                if let Ok(mut cb) = log_cb_for_open.lock() {
                    cb(
                        "error".to_string(),
                        "Subscription serialization failed".to_string(),
                        "Could not create subscription JSON".to_string()
                    );
                }
            }
        }) as Box<dyn FnMut(_)>);
        ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));

        // Setup message handler
        let log_cb_for_msg = log_callback.clone();
        let msg_count_for_handler = msg_count.clone();
        let pump_msg_count_for_handler = pump_msg_count.clone();
        let on_message = wasm_bindgen::closure::Closure::wrap(Box::new(move |e: MessageEvent| {
            // Increment total message count
            let current_count = {
                let mut count = msg_count_for_handler.lock()
                    .expect("Failed to lock message count");
                *count += 1;
                *count
            };
            
            // Log every 50 messages to show activity
            if current_count % 50 == 0 {
                info!("Received {} total WebSocket messages", current_count);
                if let Ok(mut cb) = log_cb_for_msg.lock() {
                    let pump_count = *pump_msg_count_for_handler.lock()
                        .expect("Failed to lock pump message count");
                    cb(
                        "info".to_string(),
                        "Monitor is active".to_string(),
                        format!("Total messages: {} | Pump.fun messages: {}", current_count, pump_count)
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
                            if result.is_u64() {
                                let sub_id = result.as_u64().unwrap();
                                info!("✓ Subscription confirmed with ID: {}", sub_id);
                                if let Ok(mut cb) = log_cb_for_msg.lock() {
                                    cb(
                                        "info".to_string(),
                                        "✓ Subscription confirmed".to_string(),
                                        format!("Subscription ID: {}\nNow actively monitoring for pump.fun transactions", sub_id)
                                    );
                                }
                                return;
                            }
                        }

                        // Check if this is a log notification
                        if let Some(method) = value.get("method").and_then(|m| m.as_str()) {
                            if method == "logsNotification" {
                                // Increment pump.fun message count
                                let pump_count = {
                                    let mut count = pump_msg_count_for_handler.lock()
                                        .expect("Failed to lock pump message count");
                                    *count += 1;
                                    *count
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
                                        if let Ok(mut cb) = log_cb_for_msg.lock() {
                                            cb(
                                                "info".to_string(),
                                                format!("Transaction received (#{} total)", pump_count),
                                                format!("Signature: {}\nLogs: {} entries\nError: {}", 
                                                       sig, logs.len(), 
                                                       if err.is_some() { "Yes" } else { "No" })
                                            );
                                        }
                                        
                                        // Check if we've seen this signature before
                                        let is_new = {
                                            let mut seen = seen_sigs.lock()
                                                .expect("Failed to lock seen signatures");
                                            seen.insert(sig.to_string())
                                        };

                                        if !is_new {
                                            info!("Transaction {} already seen, skipping", sig);
                                            return;
                                        }

                                        // Check if any log mentions the pump.fun program
                                        let pump_fun_logs: Vec<String> = logs.iter()
                                            .filter_map(|log| log.as_str())
                                            .filter(|s| s.contains(&pump_fun_program))
                                            .map(|s| s.to_string())
                                            .collect();

                                        if !pump_fun_logs.is_empty() {
                                            info!("✓ New pump.fun transaction detected: {}", sig);
                                            info!("  Matching logs: {:?}", pump_fun_logs);
                                            
                                            // Check for specific instruction types
                                            let mut instruction_types = Vec::new();
                                            for log in logs.iter().filter_map(|l| l.as_str()) {
                                                if log.contains("Program log: Instruction: Create") {
                                                    instruction_types.push("Create");
                                                } else if log.contains("Program log: Instruction: Buy") {
                                                    instruction_types.push("Buy");
                                                } else if log.contains("Program log: Instruction: Sell") {
                                                    instruction_types.push("Sell");
                                                }
                                            }
                                            
                                            let instr_info = if !instruction_types.is_empty() {
                                                format!("Instructions: {}", instruction_types.join(", "))
                                            } else {
                                                "No specific instructions detected".to_string()
                                            };
                                            
                                            // Log the detection with details
                                            if let Ok(mut cb) = log_cb_for_msg.lock() {
                                                cb(
                                                    "info".to_string(),
                                                    "🎯 New pump.fun transaction detected!".to_string(),
                                                    format!("Signature: {}\nLogs with pump.fun: {}\n{}\n\nNext steps:\n1. Fetch transaction details\n2. Extract token mint address\n3. Get token metadata\n4. Apply buy heuristics\n5. Execute purchase if conditions met", 
                                                           sig, pump_fun_logs.len(), instr_info)
                                                );
                                            }

                                            // In a full implementation, we would:
                                            // 1. Fetch transaction details via RPC
                                            // 2. Extract mint, creator, bonding curve addresses
                                            // 3. Fetch token metadata
                                            // 4. Apply buy heuristics
                                            // 5. Execute purchase if conditions met
                                        } else {
                                            // Log that we received a message but it didn't match
                                            if pump_count <= 10 || pump_count % 25 == 0 {
                                                info!("Transaction {} does not contain pump.fun program ID", sig);
                                                if let Ok(mut cb) = log_cb_for_msg.lock() {
                                                    cb(
                                                        "info".to_string(),
                                                        format!("Non-pump.fun transaction (#{} total)", pump_count),
                                                        format!("Signature: {}\nThis transaction doesn't involve the pump.fun program", sig)
                                                    );
                                                }
                                            }
                                        }
                                    } else {
                                        error!("logsNotification missing logs or signature");
                                        if let Ok(mut cb) = log_cb_for_msg.lock() {
                                            cb(
                                                "warn".to_string(),
                                                "Incomplete transaction data".to_string(),
                                                "Received logsNotification without proper logs or signature".to_string()
                                            );
                                        }
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
                            if let Ok(mut cb) = log_cb_for_msg.lock() {
                                cb(
                                    "error".to_string(),
                                    "WebSocket error response".to_string(),
                                    format!("{:?}", value.get("error"))
                                );
                            }
                        }
                    },
                    Err(e) => {
                        error!("Failed to parse WebSocket message: {}", e);
                        if current_count <= 5 {
                            if let Ok(mut cb) = log_cb_for_msg.lock() {
                                cb(
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
                }
            } else {
                error!("Received non-text WebSocket message");
                if let Ok(mut cb) = log_cb_for_msg.lock() {
                    cb(
                        "warn".to_string(),
                        "Non-text message received".to_string(),
                        "WebSocket sent binary or other non-text data".to_string()
                    );
                }
            }
        }) as Box<dyn FnMut(_)>);
        ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

        // Setup error handler
        let log_cb_for_err = log_callback.clone();
        let on_error = wasm_bindgen::closure::Closure::wrap(Box::new(move |e: ErrorEvent| {
            let error_msg = format!("WebSocket error - Type: {:?}, Message: {}", 
                                   e.type_(), e.message());
            error!("{}", error_msg);
            if let Ok(mut cb) = log_cb_for_err.lock() {
                cb(
                    "error".to_string(),
                    "❌ WebSocket error occurred".to_string(),
                    error_msg
                );
            }
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
            if let Ok(mut cb) = log_cb_for_close.lock() {
                cb(
                    "warn".to_string(),
                    "⚠️ WebSocket connection closed".to_string(),
                    close_msg
                );
            }
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
        self.seen_signatures.lock()
            .expect("Failed to lock seen signatures for cleanup")
            .clear();

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

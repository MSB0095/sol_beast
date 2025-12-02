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

        // Create WebSocket connection
        let ws = WebSocket::new(ws_url)
            .map_err(|e| JsValue::from_str(&format!("Failed to create WebSocket: {:?}", e)))?;
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

        let pump_fun_program = pump_fun_program.to_string();
        let seen_sigs = self.seen_signatures.clone();

        // Setup open handler to subscribe once connected
        let ws_for_open = ws.clone();
        let pump_prog_for_sub = pump_fun_program.clone();
        let log_cb_for_open = log_callback.clone();
        let on_open = wasm_bindgen::closure::Closure::wrap(Box::new(move |_e: JsValue| {
            info!("WebSocket connected, subscribing to logs...");
            
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
                if let Err(e) = ws_for_open.send_with_str(&msg_str) {
                    error!("Failed to send subscription: {:?}", e);
                } else {
                    info!("Sent logsSubscribe request for pump.fun program");
                    // Log to UI
                    if let Ok(mut cb) = log_cb_for_open.lock() {
                        cb(
                            "info".to_string(),
                            "Subscribed to pump.fun events".to_string(),
                            format!("Monitoring program: {}", pump_prog_for_sub)
                        );
                    }
                }
            }
        }) as Box<dyn FnMut(_)>);
        ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));

        // Setup message handler
        let log_cb_for_msg = log_callback.clone();
        let on_message = wasm_bindgen::closure::Closure::wrap(Box::new(move |e: MessageEvent| {
            if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                let message: String = txt.into();
                
                // Parse the message
                if let Ok(value) = serde_json::from_str::<Value>(&message) {
                    // Check if this is a subscription confirmation
                    if let Some(result) = value.get("result") {
                        if result.is_u64() {
                            let sub_id = result.as_u64().unwrap();
                            info!("Subscription confirmed with ID: {}", sub_id);
                            if let Ok(mut cb) = log_cb_for_msg.lock() {
                                cb(
                                    "info".to_string(),
                                    "Subscription active".to_string(),
                                    format!("Subscription ID: {}", sub_id)
                                );
                            }
                            return;
                        }
                    }

                    // Check if this is a log notification
                    if let Some(method) = value.get("method").and_then(|m| m.as_str()) {
                        if method == "logsNotification" {
                            if let Some(params) = value
                                .get("params")
                                .and_then(|p| p.get("result"))
                                .and_then(|r| r.get("value"))
                            {
                                // Extract logs and signature
                                let logs = params.get("logs").and_then(|l| l.as_array());
                                let signature = params.get("signature").and_then(|s| s.as_str());

                                if let (Some(logs), Some(sig)) = (logs, signature) {
                                    // Check if we've seen this signature before
                                    let is_new = {
                                        let mut seen = seen_sigs.lock()
                                            .expect("Failed to lock seen signatures");
                                        seen.insert(sig.to_string())
                                    };

                                    if is_new {
                                        // Check if any log mentions the pump.fun program
                                        let has_pump_fun = logs.iter().any(|log| {
                                            log.as_str()
                                                .map(|s| s.contains(&pump_fun_program))
                                                .unwrap_or(false)
                                        });

                                        if has_pump_fun {
                                            info!("New pump.fun transaction detected: {}", sig);
                                            
                                            // Log the detection
                                            if let Ok(mut cb) = log_cb_for_msg.lock() {
                                                cb(
                                                    "info".to_string(),
                                                    "New token detected".to_string(),
                                                    format!("Transaction signature: {}", sig)
                                                );
                                            }

                                            // In a full implementation, we would:
                                            // 1. Fetch transaction details via RPC
                                            // 2. Extract mint, creator, bonding curve addresses
                                            // 3. Fetch token metadata
                                            // 4. Apply buy heuristics
                                            // 5. Execute purchase if conditions met
                                            // For now, we just log the detection
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }) as Box<dyn FnMut(_)>);
        ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

        // Setup error handler
        let log_cb_for_err = log_callback.clone();
        let on_error = wasm_bindgen::closure::Closure::wrap(Box::new(move |e: ErrorEvent| {
            error!("WebSocket error: {:?}", e);
            if let Ok(mut cb) = log_cb_for_err.lock() {
                cb(
                    "error".to_string(),
                    "WebSocket error occurred".to_string(),
                    "Check console for details".to_string()
                );
            }
        }) as Box<dyn FnMut(_)>);
        ws.set_onerror(Some(on_error.as_ref().unchecked_ref()));

        // Setup close handler
        let log_cb_for_close = log_callback;
        let on_close = wasm_bindgen::closure::Closure::wrap(Box::new(move |e: CloseEvent| {
            info!("WebSocket closed: code={} reason={}", e.code(), e.reason());
            if let Ok(mut cb) = log_cb_for_close.lock() {
                cb(
                    "warn".to_string(),
                    "WebSocket connection closed".to_string(),
                    format!("Code: {}, Reason: {}", e.code(), e.reason())
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
